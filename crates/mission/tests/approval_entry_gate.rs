#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/host_hook.rs"]
mod host_hook;
use approval_fixture::{SESSION, git_workspace, linked, ready_ticket};
use mission::approval::{EntryPath, ReviewKind, ReviewVerdict};
use mission::{
    LifecycleRead, LifecycleState, LifecycleTransition, TransitionRequest, WaypointIdentity,
    WorkspaceLifecycle,
};
use std::fs;
use uuid::Uuid;

fn to(state: &str) -> TransitionRequest {
    TransitionRequest {
        op_id: Uuid::new_v4(),
        to_native_state: state.into(),
        author: Some("w4-test".into()),
    }
}
fn native(adapter: &WorkspaceLifecycle, ticket: Uuid) -> String {
    adapter
        .read_lifecycle(&WaypointIdentity::Ticket { ticket_id: ticket })
        .unwrap()
        .native_state
}
#[test]
fn independent_ticket_activation_stays_on_ordinary_ticket_authority() {
    let (_dir, ws) = git_workspace();
    let ticket = ready_ticket(&ws);
    let adapter = WorkspaceLifecycle::open(&ws);
    let identity = WaypointIdentity::Ticket { ticket_id: ticket };
    let snapshot = adapter
        .transition(&identity, &to("in-implementation"))
        .unwrap();
    assert_eq!(
        (snapshot.state, snapshot.native_state.as_str()),
        (LifecycleState::Active, "in-implementation")
    );
    adapter.transition(&identity, &to("on-hold")).unwrap();
}
#[test]
fn linked_activation_requires_approval_through_both_identities() {
    let f = linked();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    for identity in [
        WaypointIdentity::Ticket {
            ticket_id: f.ticket,
        },
        WaypointIdentity::Waypoint {
            mission_id: f.id,
            waypoint_id: "a".into(),
        },
    ] {
        for state in ["in-implementation", "in-review"] {
            assert_eq!(
                adapter
                    .transition(&identity, &to(state))
                    .unwrap_err()
                    .code(),
                "approval_required"
            );
        }
    }
    assert_eq!(native(&adapter, f.ticket), "ready");
}
#[test]
fn approved_linked_activation_and_resume_recheck_authority() {
    let f = linked();
    f.approve("c1", "evt-1");
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let waypoint = WaypointIdentity::Waypoint {
        mission_id: f.id,
        waypoint_id: "a".into(),
    };
    assert_eq!(
        adapter.read_lifecycle(&waypoint).unwrap().native_state,
        "ready"
    );
    let active = adapter
        .transition(&waypoint, &to("in-implementation"))
        .unwrap();
    assert_eq!(active.state, LifecycleState::Active);
    assert_eq!(native(&adapter, f.ticket), "in-implementation");
    adapter.transition(&waypoint, &to("on-hold")).unwrap();
    adapter
        .transition(&waypoint, &to("in-implementation"))
        .unwrap();
    adapter.transition(&waypoint, &to("on-hold")).unwrap();
    f.edit_b("semantic", "b changed");
    for identity in [
        WaypointIdentity::Ticket {
            ticket_id: f.ticket,
        },
        waypoint,
    ] {
        assert_eq!(
            adapter
                .transition(&identity, &to("in-implementation"))
                .unwrap_err()
                .code(),
            "approval_stale"
        );
    }
    assert_eq!(native(&adapter, f.ticket), "on-hold");
}
#[test]
fn absent_foreign_ticket_blocks_approved_entry() {
    let f = linked();
    f.approve("c1", "evt-1");
    fs::remove_dir_all(
        f.ws.join(".workflow-tools/ticket/tickets")
            .join(f.ticket.to_string()),
    )
    .unwrap();
    assert!(
        WorkspaceLifecycle::open(&f.ws)
            .transition(
                &WaypointIdentity::Waypoint {
                    mission_id: f.id,
                    waypoint_id: "a".into()
                },
                &to("in-implementation")
            )
            .is_err()
    );
}
#[test]
fn finished_session_after_grant_refuses_entry() {
    let f = linked();
    f.approve("c1", "evt-1");
    f.approval()
        .check_entry(f.id, EntryPath::Activation)
        .unwrap();
    f.session
        .finish_workflow(SESSION, vec![], vec![], None)
        .unwrap();
    assert_eq!(f.code(EntryPath::Activation), "approval_required");
    let adapter = WorkspaceLifecycle::open(&f.ws);
    assert_eq!(
        adapter
            .transition(
                &WaypointIdentity::Ticket {
                    ticket_id: f.ticket
                },
                &to("in-implementation")
            )
            .unwrap_err()
            .code(),
        "approval_required"
    );
    assert_eq!(native(&adapter, f.ticket), "ready");
}
#[test]
fn failing_latest_cold_review_revokes_challenge_eligibility() {
    let f = linked();
    f.reviewed();
    f.review(ReviewKind::ColdWalkthrough, ReviewVerdict::Fail, "cold-2")
        .unwrap();
    assert_eq!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "c1")
            .unwrap_err()
            .code(),
        "approval_required"
    );
    f.review(ReviewKind::ColdWalkthrough, ReviewVerdict::Pass, "cold-3")
        .unwrap();
    assert_eq!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "c1")
            .unwrap_err()
            .code(),
        "approval_required"
    );
    f.review(ReviewKind::Formal, ReviewVerdict::Pass, "formal")
        .unwrap();
    f.approval().prepare_challenge(f.id, SESSION, "c1").unwrap();
}
#[test]
fn tampered_challenge_binding_fails_closed_at_every_entry() {
    let f = linked();
    f.approve("c1", "evt-1");
    f.approval().check_entry(f.id, EntryPath::Resume).unwrap();
    let journal =
        f.ws.join(".workflow-tools/mission/missions")
            .join(f.id.to_string())
            .join("approvals/journal");
    for path in fs::read_dir(journal).unwrap().map(|e| e.unwrap().path()) {
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        if value["entry"]["type"] == "challenge" {
            value["entry"]["review_snapshot_digest"] = "0".repeat(64).into();
            fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        }
    }
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert_eq!(f.code(path), "approval_journal_corrupt");
    }
}
#[cfg(unix)]
#[test]
fn symlinked_mission_authority_fails_closed_for_activation() {
    let (_dir, ws) = git_workspace();
    let ticket = ready_ticket(&ws);
    let missions = ws.join(".workflow-tools/mission/missions");
    fs::create_dir_all(&missions).unwrap();
    std::os::unix::fs::symlink(&ws, missions.join(Uuid::new_v4().to_string())).unwrap();
    let adapter = WorkspaceLifecycle::open(&ws);
    assert_eq!(
        adapter
            .transition(
                &WaypointIdentity::Ticket { ticket_id: ticket },
                &to("in-implementation")
            )
            .unwrap_err()
            .code(),
        "approval_authority_unavailable"
    );
    assert_eq!(native(&adapter, ticket), "ready");
}

#[test]
fn ticket_prerequisite_blocks_approved_entry_until_provider_advances() {
    let f = linked();
    f.approve("c1", "evt-1");
    let store = ticket_api::storage::store::TicketStore::open(&f.ws).unwrap();
    let provider = store
        .create(
            None,
            "tracker-improvement",
            Some("Provider"),
            Some("ready"),
            std::collections::BTreeMap::new(),
            None,
            Some("body"),
        )
        .unwrap();
    let mut patch = std::collections::BTreeMap::new();
    patch.insert(
        "depends_on".to_string(),
        serde_json::json!([provider.to_string()]),
    );
    store
        .update(&f.ticket, patch, None, None, None, None)
        .unwrap();
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let waypoint = WaypointIdentity::Waypoint {
        mission_id: f.id,
        waypoint_id: "a".into(),
    };
    f.approval()
        .check_entry(f.id, EntryPath::Activation)
        .unwrap();
    let error = adapter
        .transition(&waypoint, &to("in-implementation"))
        .unwrap_err();
    assert_eq!(error.code(), "foreign_transition_rejected", "{error}");
    assert_eq!(native(&adapter, f.ticket), "ready");
    store
        .update(
            &provider,
            std::collections::BTreeMap::new(),
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        adapter
            .transition(&waypoint, &to("in-implementation"))
            .unwrap()
            .state,
        LifecycleState::Active
    );
}

#[test]
fn activation_waits_for_ticket_link_lock() {
    let (_dir, ws) = git_workspace();
    let ticket = ready_ticket(&ws);
    let held = mission::MissionLifecycle::open(&ws)
        .lock_ticket_links(ticket)
        .unwrap();
    let (rt, rr) = std::sync::mpsc::channel();
    let (dt, dr) = std::sync::mpsc::channel();
    let worker_ws = ws.clone();
    std::thread::spawn(move || {
        rt.send(()).unwrap();
        let result = WorkspaceLifecycle::open(&worker_ws)
            .transition(
                &WaypointIdentity::Ticket { ticket_id: ticket },
                &to("in-implementation"),
            )
            .map(|s| s.state)
            .map_err(|e| e.code());
        let _ = dt.send(result);
    });
    rr.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    assert!(
        dr.recv_timeout(std::time::Duration::from_millis(500))
            .is_err()
    );
    assert_eq!(native(&WorkspaceLifecycle::open(&ws), ticket), "ready");
    drop(held);
    assert_eq!(
        dr.recv_timeout(std::time::Duration::from_secs(60))
            .unwrap()
            .unwrap(),
        LifecycleState::Active
    );
}
