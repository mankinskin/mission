//! W1 facade adapter tests: Ticket authority, Mission projections and
//! unsupported capabilities across the two stores.

use std::{collections::BTreeMap, fs, path::Path};

use mission::{
    CommitCheckpoint, ControlEvent, ControlKind, ControlRequest, ControlTarget, DirectWaypoint,
    EligibilityDenial, ExecutionEligibility, LifecycleJournal, LifecycleRead, LifecycleState,
    LifecycleTransition, MissionLifecycle, NativeDomain, ReopenCapability, TransitionRequest,
    ValidationGate, WaypointIdentity, WorkspaceLifecycle,
};
use ticket_api::storage::store::TicketStore;
use uuid::Uuid;

fn ticket(root: &Path) -> Uuid {
    let id = Uuid::new_v4();
    TicketStore::init(root)
        .unwrap()
        .create(
            Some(id),
            "tracker-improvement",
            Some("Linked ticket"),
            Some("planning"),
            BTreeMap::new(),
            None,
            Some("Linked ticket body."),
        )
        .unwrap();
    id
}

fn direct(id: &str) -> DirectWaypoint {
    DirectWaypoint {
        waypoint_id: id.into(),
        title: format!("{id} title"),
        depends_on: vec![],
        prompt: format!("{id} prompt"),
        validation: vec![ValidationGate {
            command: "true".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: format!("{id} checkpoint"),
        },
        non_goals: vec![],
    }
}

fn transition(to: &str) -> TransitionRequest {
    TransitionRequest {
        op_id: Uuid::new_v4(),
        to_native_state: to.into(),
        author: Some("w1-test".into()),
    }
}

fn control(
    seq: u64,
    kind: ControlKind,
    target: ControlTarget,
    prior: LifecycleState,
) -> ControlEvent {
    ControlEvent {
        seq,
        op_id: Uuid::new_v4(),
        target,
        prior_seq: seq - 1,
        event: kind,
        prior_state: prior,
        evidence: None,
    }
}

fn write_journal(root: &Path, mission_id: Uuid, events: Vec<ControlEvent>) {
    let journal = LifecycleJournal { version: 1, events };
    fs::write(
        LifecycleJournal::path(root, mission_id),
        journal.to_json().unwrap(),
    )
    .unwrap();
}

/// Mission with one waypoint rewritten to be backed by `ticket_id`.
fn ticket_backed_mission(root: &Path, ticket_id: Uuid) -> Uuid {
    let accepted = MissionLifecycle::open(root)
        .create("Linked".into(), "objective".into(), vec![direct("first")])
        .unwrap();
    let path = root
        .join(".workflow-tools/mission/missions")
        .join(accepted.mission_id.to_string())
        .join("accepted.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let waypoint = &mut value["bundle"]["waypoints"][0];
    waypoint["scope"] = serde_json::json!({
        "kind": "ticket-backed",
        "ticket_ref": format!("ce://default/ticket/{ticket_id}"),
    });
    waypoint.as_object_mut().unwrap().remove("state");
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    accepted.mission_id
}

#[test]
fn ticket_snapshot_retains_native_state_revision_and_root() {
    let root = tempfile::tempdir().unwrap();
    let id = ticket(root.path());
    let adapter = WorkspaceLifecycle::open(root.path());
    let identity = WaypointIdentity::Ticket { ticket_id: id };
    let snapshot = adapter.read_lifecycle(&identity).unwrap();
    assert_eq!(snapshot.state, LifecycleState::Planned);
    assert_eq!(snapshot.native_domain, NativeDomain::Ticket);
    assert_eq!(snapshot.native_state, "planning");
    assert_eq!(snapshot.owning_root, root.path());
    assert_eq!(snapshot.control_sequence, 0);
    assert!(snapshot.eligibility.is_eligible());
    assert!(!snapshot.result_review_needed);
}

#[test]
fn ticket_transitions_use_ticket_domain_and_distinguish_cancelled_from_blocked() {
    let root = tempfile::tempdir().unwrap();
    let id = ticket(root.path());
    let adapter = WorkspaceLifecycle::open(root.path());
    let identity = WaypointIdentity::Ticket { ticket_id: id };
    let before = adapter.read_lifecycle(&identity).unwrap();

    let held = adapter
        .transition(&identity, &transition("on-hold"))
        .unwrap();
    assert_eq!(
        (held.state, held.native_state.as_str()),
        (LifecycleState::Paused, "on-hold")
    );
    assert!(held.native_revision > before.native_revision);
    // The Ticket store is the one source of truth.
    let stored = TicketStore::open(root.path()).unwrap().get(&id).unwrap();
    assert_eq!(stored.extra["state"], "on-hold");

    let cancelled = adapter
        .transition(&identity, &transition("cancelled"))
        .unwrap();
    assert_eq!(cancelled.state, LifecycleState::Cancelled);
    assert_ne!(cancelled.state, LifecycleState::Blocked);
}

#[test]
fn illegal_ticket_transition_is_rejected_by_ticket_domain_and_changes_nothing() {
    let root = tempfile::tempdir().unwrap();
    let id = ticket(root.path());
    let adapter = WorkspaceLifecycle::open(root.path());
    let identity = WaypointIdentity::Ticket { ticket_id: id };
    let before = adapter.read_lifecycle(&identity).unwrap();
    let error = adapter
        .transition(&identity, &transition("not-a-state"))
        .unwrap_err();
    assert_eq!(error.code(), "foreign_transition_rejected");
    assert_eq!(adapter.read_lifecycle(&identity).unwrap(), before);
}

#[test]
fn absent_foreign_authority_is_explicit() {
    let root = tempfile::tempdir().unwrap();
    // No Ticket store at all.
    let adapter = WorkspaceLifecycle::open(root.path());
    let missing = WaypointIdentity::Ticket {
        ticket_id: Uuid::new_v4(),
    };
    assert_eq!(
        adapter.read_lifecycle(&missing).unwrap_err().code(),
        "foreign_authority_absent"
    );
    // Store exists, Ticket does not.
    ticket(root.path());
    assert_eq!(
        adapter.read_lifecycle(&missing).unwrap_err().code(),
        "foreign_authority_absent"
    );
    // Unknown Mission is a Mission-owned error.
    let mission = WaypointIdentity::Mission {
        mission_id: Uuid::new_v4(),
    };
    assert_eq!(
        adapter.read_lifecycle(&mission).unwrap_err().code(),
        "mission_unavailable"
    );
}

#[test]
fn paused_mission_denies_eligibility_but_keeps_ticket_state() {
    let root = tempfile::tempdir().unwrap();
    let ticket_id = ticket(root.path());
    let mission_id = ticket_backed_mission(root.path(), ticket_id);
    let adapter = WorkspaceLifecycle::open(root.path());
    let waypoint = WaypointIdentity::Waypoint {
        mission_id,
        waypoint_id: "first".into(),
    };
    let mission = WaypointIdentity::Mission { mission_id };

    // No journal (legacy): no controls, ticket planning -> planned mission.
    let legacy = adapter.read_lifecycle(&mission).unwrap();
    assert_eq!(
        (legacy.state, legacy.control_sequence),
        (LifecycleState::Planned, 0)
    );
    assert!(legacy.eligibility.is_eligible());

    adapter
        .transition(&waypoint, &transition("cancelled"))
        .unwrap();
    write_journal(
        root.path(),
        mission_id,
        vec![control(
            1,
            ControlKind::Pause,
            ControlTarget::Mission,
            LifecycleState::Planned,
        )],
    );
    let ticket_wp = adapter.read_lifecycle(&waypoint).unwrap();
    assert_eq!(
        ticket_wp.state,
        LifecycleState::Cancelled,
        "Ticket state unchanged by Mission pause"
    );
    assert_eq!(ticket_wp.native_domain, NativeDomain::Ticket);
    assert_eq!(ticket_wp.control_sequence, 1);
    assert_eq!(
        ticket_wp.eligibility,
        ExecutionEligibility::Denied {
            denial: EligibilityDenial::MissionPaused
        }
    );
    let paused = adapter.read_lifecycle(&mission).unwrap();
    assert_eq!(paused.state, LifecycleState::Paused);
    assert_eq!(paused.native_domain, NativeDomain::Mission);
    assert_eq!(
        TicketStore::open(root.path())
            .unwrap()
            .get(&ticket_id)
            .unwrap()
            .extra["state"],
        "cancelled"
    );
}

#[test]
fn single_session_projection_controls_and_legacy_completion() {
    let root = tempfile::tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(root.path());
    let accepted = lifecycle
        .create(
            "Local".into(),
            "o".into(),
            vec![direct("a"), {
                let mut b = direct("b");
                b.depends_on = vec!["a".into()];
                b
            }],
        )
        .unwrap();
    let id = accepted.mission_id;
    let adapter = WorkspaceLifecycle::open(root.path());
    let mission = WaypointIdentity::Mission { mission_id: id };
    let a = WaypointIdentity::Waypoint {
        mission_id: id,
        waypoint_id: "a".into(),
    };

    let snapshot = adapter.read_lifecycle(&mission).unwrap();
    assert_eq!(snapshot.state, LifecycleState::Planned);
    assert_eq!(snapshot.native_revision, 1);
    assert_eq!(snapshot.native_state, "pending");

    // Pause of a single-session waypoint comes from the journal only.
    write_journal(
        root.path(),
        id,
        vec![control(
            1,
            ControlKind::Pause,
            ControlTarget::Waypoint {
                waypoint_id: "a".into(),
            },
            LifecycleState::Planned,
        )],
    );
    let paused = adapter.read_lifecycle(&a).unwrap();
    assert_eq!(paused.state, LifecycleState::Paused);
    assert_eq!(paused.native_state, "pending", "native value retained");
    assert_eq!(lifecycle.get(id).unwrap().bundle.manifest.revision, 1);
    // A paused waypoint still counts as started work for the Mission.
    assert_eq!(
        adapter.read_lifecycle(&mission).unwrap().state,
        LifecycleState::Active
    );

    // Corrupt journal is explicit, never reset.
    fs::write(LifecycleJournal::path(root.path(), id), "{").unwrap();
    assert_eq!(
        adapter.read_lifecycle(&mission).unwrap_err().code(),
        "lifecycle_journal_corrupt"
    );
    fs::remove_file(LifecycleJournal::path(root.path(), id)).unwrap();

    // Legacy all-completed Mission without completion event: active + review.
    let mut legacy = lifecycle.get(id).unwrap().bundle;
    legacy.manifest.revision = 2;
    for waypoint in &mut legacy.waypoints {
        waypoint.state = Some(mission::WaypointState::Completed);
    }
    let rev = mission::MissionStore::open(root.path())
        .import(id, legacy, 1)
        .unwrap();
    let review = adapter.read_lifecycle(&mission).unwrap();
    assert_eq!(review.state, LifecycleState::Active);
    assert!(review.result_review_needed);
    assert_eq!(review.native_revision, rev.bundle.manifest.revision);

    // Explicit completion event completes it.
    let mut done = control(
        1,
        ControlKind::Complete,
        ControlTarget::Mission,
        LifecycleState::Active,
    );
    done.evidence = Some("results reviewed".into());
    write_journal(root.path(), id, vec![done]);
    let completed = adapter.read_lifecycle(&mission).unwrap();
    assert_eq!(completed.state, LifecycleState::Completed);
    assert!(!completed.result_review_needed);
    assert!(!completed.eligibility.is_eligible());
}

#[test]
fn unsupported_capabilities_do_not_change_either_store() {
    let root = tempfile::tempdir().unwrap();
    let id = ticket(root.path());
    let adapter = WorkspaceLifecycle::open(root.path());
    let identity = WaypointIdentity::Ticket { ticket_id: id };
    let before = adapter.read_lifecycle(&identity).unwrap();
    let request = ControlRequest {
        op_id: Uuid::new_v4(),
        expected_control_sequence: 0,
        evidence: None,
    };
    let error = adapter.reopen(&identity, &request).unwrap_err();
    assert_eq!(error.code(), "unsupported_capability");
    assert!(error.to_string().contains("reopen"));
    assert_eq!(adapter.read_lifecycle(&identity).unwrap(), before);
    // Mission-level native transition is not a W1 capability.
    let mission = WaypointIdentity::Mission {
        mission_id: Uuid::new_v4(),
    };
    assert_eq!(
        adapter
            .transition(&mission, &transition("done"))
            .unwrap_err()
            .code(),
        "unsupported_capability"
    );
}

#[test]
fn public_lifecycle_module_paths_resolve_for_facade_and_api() {
    let _: mission::lifecycle::LifecycleState = mission_api::lifecycle::LifecycleState::Active;
    let root = tempfile::tempdir().unwrap();
    let _ = mission::lifecycle::WorkspaceLifecycle::open(root.path());
    let _ = mission::WorkspaceLifecycle::open(root.path());
}

#[test]
fn cancelled_required_ticket_blocks_mission_and_denies_eligibility() {
    let root = tempfile::tempdir().unwrap();
    let ticket_id = ticket(root.path());
    let mission_id = ticket_backed_mission(root.path(), ticket_id);
    let adapter = WorkspaceLifecycle::open(root.path());
    let waypoint = WaypointIdentity::Waypoint {
        mission_id,
        waypoint_id: "first".into(),
    };
    adapter
        .transition(&waypoint, &transition("cancelled"))
        .unwrap();
    let ticket_state = adapter.read_lifecycle(&waypoint).unwrap();
    assert_eq!(
        ticket_state.state,
        LifecycleState::Cancelled,
        "Ticket stays cancelled"
    );
    let mission = adapter
        .read_lifecycle(&WaypointIdentity::Mission { mission_id })
        .unwrap();
    assert_eq!(mission.state, LifecycleState::Blocked);
    assert!(!mission.eligibility.is_eligible());
}

#[test]
fn ghost_journal_target_is_rejected_at_facade_reads() {
    let root = tempfile::tempdir().unwrap();
    let accepted = MissionLifecycle::open(root.path())
        .create("Local".into(), "o".into(), vec![direct("a")])
        .unwrap();
    let id = accepted.mission_id;
    let adapter = WorkspaceLifecycle::open(root.path());
    write_journal(
        root.path(),
        id,
        vec![control(
            1,
            ControlKind::Pause,
            ControlTarget::Waypoint {
                waypoint_id: "ghost".into(),
            },
            LifecycleState::Planned,
        )],
    );
    for identity in [
        WaypointIdentity::Mission { mission_id: id },
        WaypointIdentity::Waypoint {
            mission_id: id,
            waypoint_id: "a".into(),
        },
    ] {
        let error = adapter.read_lifecycle(&identity).unwrap_err();
        assert_eq!(error.code(), "control_history_invalid");
        assert!(error.to_string().contains("ghost"));
    }
    // Unsupported version / invalid history stay explicit at the same read.
    let path = LifecycleJournal::path(root.path(), id);
    fs::write(&path, "{\"version\":2,\"events\":[]}").unwrap();
    let mission = WaypointIdentity::Mission { mission_id: id };
    assert_eq!(
        adapter.read_lifecycle(&mission).unwrap_err().code(),
        "lifecycle_journal_unsupported_version"
    );
}

#[test]
fn ticket_backed_waypoint_cannot_be_a_control_target() {
    let root = tempfile::tempdir().unwrap();
    let ticket_id = ticket(root.path());
    let mission_id = ticket_backed_mission(root.path(), ticket_id);
    write_journal(
        root.path(),
        mission_id,
        vec![control(
            1,
            ControlKind::Pause,
            ControlTarget::Waypoint {
                waypoint_id: "first".into(),
            },
            LifecycleState::Planned,
        )],
    );
    let adapter = WorkspaceLifecycle::open(root.path());
    let error = adapter
        .read_lifecycle(&WaypointIdentity::Mission { mission_id })
        .unwrap_err();
    assert_eq!(error.code(), "control_history_invalid");
}

#[test]
fn completion_event_with_pending_required_waypoint_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(root.path());
    let id = lifecycle
        .create("Local".into(), "o".into(), vec![direct("a")])
        .unwrap()
        .mission_id;
    let mut done = control(
        1,
        ControlKind::Complete,
        ControlTarget::Mission,
        LifecycleState::Active,
    );
    done.evidence = Some("claimed".into());
    write_journal(root.path(), id, vec![done]);
    let adapter = WorkspaceLifecycle::open(root.path());
    let mission = WaypointIdentity::Mission { mission_id: id };
    assert_eq!(
        adapter.read_lifecycle(&mission).unwrap_err().code(),
        "control_history_invalid"
    );
    // Valid once the required outcome is completed.
    let mut legacy = lifecycle.get(id).unwrap().bundle;
    legacy.manifest.revision = 2;
    legacy.waypoints[0].state = Some(mission::WaypointState::Completed);
    mission::MissionStore::open(root.path())
        .import(id, legacy, 1)
        .unwrap();
    let snapshot = adapter.read_lifecycle(&mission).unwrap();
    assert_eq!(snapshot.state, LifecycleState::Completed);
}
