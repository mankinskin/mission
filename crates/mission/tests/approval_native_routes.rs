//! Native-route approval regressions; fixture hook input is not human proof.
#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/host_hook.rs"]
mod host_hook;
use approval_fixture::{Fx, SESSION, linked};
use mission::approval::{EntryPath, ReviewInput, ReviewKind, ReviewVerdict};
use mission::{
    LifecycleTransition, MissionLifecycle, MissionStore, Operation, OperatorResult, PlanOperation,
    PlanOperationRequest, StatusSnapshot, TransitionRequest, WaypointIdentity, WaypointState,
    WorkspaceLifecycle, execute,
};
use std::{fs, path::Path, process::Command};
use uuid::Uuid;
fn progress(f: &Fx, state: WaypointState) -> StatusSnapshot {
    execute(
        &f.ws,
        Operation::RecordProgress {
            mission_id: f.id,
            waypoint_id: "b".into(),
            state,
            expected_current_revision: f.plan().revision,
        },
    )
}
fn code(snapshot: &StatusSnapshot) -> &str {
    &snapshot.diagnostics[0].code
}

fn states(f: &Fx) -> Vec<(String, Option<WaypointState>)> {
    MissionStore::open(&f.ws).get(f.id).unwrap().bundle.waypoints.iter()
        .map(|w| (w.waypoint_id.clone(), w.state.clone())).collect()
}
#[test]
fn typed_publish_and_operations_cannot_introduce_in_progress() {
    let f = linked();
    let lifecycle = MissionLifecycle::open(&f.ws);
    let mut draft = lifecycle.get_mission_source(f.id).unwrap();
    draft.waypoints[1].prompt = "b published".into();
    let mut created = draft.waypoints[1].clone();
    created.waypoint_id = "c".into();
    created.part.waypoint_id = "c".into();
    created.prompt = "c prompt".into();
    draft.waypoints.push(created.clone());
    lifecycle.publish_draft(f.id, draft, f.plan().revision).unwrap();
    for (id, state) in states(&f) { assert_ne!(state, Some(WaypointState::InProgress), "{id}"); }
    let mut d = created;
    d.waypoint_id = "d".into();
    d.part.waypoint_id = "d".into();
    lifecycle.apply_plan_operation(f.id, PlanOperationRequest {
        request_id: "create-d".into(), expected_revision: f.plan().revision,
        operation: PlanOperation::CreateWaypoint { waypoint: d, after: Some("c".into()) },
    }).unwrap();
    let mut b = lifecycle.get_mission_source(f.id).unwrap().waypoints[1].clone();
    b.prompt = "b updated".into();
    lifecycle.apply_plan_operation(f.id, PlanOperationRequest {
        request_id: "update-b".into(), expected_revision: f.plan().revision,
        operation: PlanOperation::UpdateWaypoint { waypoint: b },
    }).unwrap();
    assert_eq!(states(&f).len(), 4);
    for (id, state) in states(&f) { assert_ne!(state, Some(WaypointState::InProgress), "{id}"); }
    assert_eq!(code(&progress(&f, WaypointState::InProgress)), "approval_required");
}
#[test]
fn typed_edits_preserve_entered_state_but_invalidate_grant() {
    let f = linked();
    f.approve("c1", "evt-1");
    assert_eq!(progress(&f, WaypointState::InProgress).status, "ok");
    let lifecycle = MissionLifecycle::open(&f.ws);
    let mut draft = lifecycle.get_mission_source(f.id).unwrap();
    draft.waypoints[1].prompt = "b typed edit".into();
    lifecycle.publish_draft(f.id, draft, f.plan().revision).unwrap();
    assert_eq!(b_state(&f), Some(WaypointState::InProgress));
    assert_eq!(f.code(EntryPath::Resume), "approval_stale");
    f.edit_b("typed-op", "b operation edit");
    assert_eq!(b_state(&f), Some(WaypointState::InProgress));
    assert_eq!(f.code(EntryPath::Dispatch), "approval_stale");
}
#[test]
fn invalid_nonsemantic_classification_fails_closed() {
    let f = linked();
    f.approve("c1", "evt-1");
    assert_eq!(progress(&f, WaypointState::InProgress).status, "ok");
    let revision = f.plan().revision;
    let lifecycle = MissionLifecycle::open(&f.ws);
    let path = f.ws.join(".workflow-tools/mission/missions").join(f.id.to_string()).join("evidence-bindings.json");
    let original = fs::read(&path).unwrap();
    let cases = [
        (format!("{{\"entries\":[],\"nonsemantic_revisions\":[{revision},{revision}]}}"), "invalid_evidence_ledger"),
        ("{\"entries\":[],\"nonsemantic_revisions\":[0]}".into(), "invalid_evidence_ledger"),
        (format!("{{\"entries\":[],\"nonsemantic_revisions\":[{}]}}", revision + 5), "projection_diverged"),
        ("{\"entries\":[],\"nonsemantic_revisions\":[1],\"extra\":1}".into(), "invalid_evidence_ledger"),
        (format!("{{\"entries\":[{{\"result_sha256\":\"{}\",\"plan_digest\":\"{}\",\"revision\":{revision}}}],\"nonsemantic_revisions\":[{revision}]}}",
            "a".repeat(64), "b".repeat(64)), "invalid_evidence_ledger"),
    ];
    for (body, expected) in cases {
        fs::write(&path, body).unwrap();
        assert_eq!(lifecycle.review_plan(f.id).unwrap_err().diagnostics().unwrap()[0].code, expected);
        assert_eq!(f.code(EntryPath::Resume), "approval_authority_unavailable");
        let failed = execute(&f.ws, Operation::RecordProgress {
            mission_id: f.id, waypoint_id: "b".into(), state: WaypointState::Blocked,
            expected_current_revision: revision,
        });
        assert_eq!(code(&failed), expected);
    }
    fs::write(&path, original).unwrap();
    assert_eq!(progress(&f, WaypointState::Blocked).status, "ok");
}
fn b_state(f: &Fx) -> Option<WaypointState> {
    MissionStore::open(&f.ws)
        .get(f.id)
        .unwrap()
        .bundle
        .waypoints
        .iter()
        .find(|w| w.waypoint_id == "b")
        .unwrap()
        .state
        .clone()
}
fn tree_files(path: &Path) -> usize {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .map(|e| {
            let p = e.unwrap().path();
            if p.is_dir() { tree_files(&p) } else { 1 }
        })
        .sum()
}
#[test]
fn unapproved_in_progress_is_refused_on_exposed_native_routes() {
    let f = linked();
    let before = f.plan().revision;
    let refused = progress(&f, WaypointState::InProgress);
    assert_eq!(refused.status, "error");
    assert_eq!(code(&refused), "approval_required");
    let output = Command::new(env!("CARGO_BIN_EXE_mission"))
        .args([
            "--workspace",
            f.ws.to_str().unwrap(),
            "record-progress",
            &f.id.to_string(),
            "b",
            "in-progress",
            &before.to_string(),
        ])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.status.success() && text.contains("approval_required"),
        "{text}"
    );
    let error = MissionLifecycle::open(&f.ws)
        .record_progress(f.id, "b", WaypointState::InProgress, before)
        .unwrap_err();
    assert_eq!(error.diagnostics().unwrap()[0].code, "approval_required");
    let store = MissionStore::open(&f.ws);
    let mut bundle = store.get(f.id).unwrap().bundle;
    bundle.manifest.revision = before + 1;
    bundle
        .waypoints
        .iter_mut()
        .find(|w| w.waypoint_id == "b")
        .unwrap()
        .state = Some(WaypointState::InProgress);
    assert_eq!(
        store
            .import(f.id, bundle, before)
            .unwrap_err()
            .diagnostics()
            .unwrap()[0]
            .code,
        "approval_required"
    );
    assert_eq!(f.plan().revision, before);
    assert_ne!(b_state(&f), Some(WaypointState::InProgress));
}
#[test]
fn approved_native_progress_and_results_keep_grant() {
    let f = linked();
    f.approve("c1", "evt-1");
    let digest = f.plan().plan_digest;
    let entered = progress(&f, WaypointState::InProgress);
    assert_eq!(entered.status, "ok", "{:?}", entered.diagnostics);
    assert_eq!(b_state(&f), Some(WaypointState::InProgress));
    let result = execute(
        &f.ws,
        Operation::RecordValidationResult {
            mission_id: f.id,
            result: OperatorResult {
                waypoint_id: "b".into(),
                outcome: "passed".into(),
                operator_command: "cargo test".into(),
                evidence_pointer: "ce://default/test/receipt-a".into(),
            },
            expected_current_revision: f.plan().revision,
        },
    );
    assert_eq!(result.status, "ok", "{:?}", result.diagnostics);
    assert_eq!(progress(&f, WaypointState::Blocked).status, "ok");
    assert_eq!(f.plan().plan_digest, digest);
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        f.approval().check_entry(f.id, path).unwrap();
    }
    assert_eq!(progress(&f, WaypointState::InProgress).status, "ok");
    f.edit_b("semantic", "b changed");
    assert_eq!(progress(&f, WaypointState::Blocked).status, "ok");
    assert_eq!(
        code(&progress(&f, WaypointState::InProgress)),
        "approval_stale"
    );
    f.edit_b("revert", "b prompt");
    assert_eq!(f.code(EntryPath::Resume), "approval_stale");
}
#[cfg(unix)]
#[test]
fn symlinked_store_ancestor_fails_closed_for_consumers() {
    let f = linked();
    f.reviewed();
    let mission_dir = f.ws.join(".workflow-tools/mission");
    let outside = f._dir.path().join("outside-mission");
    fs::rename(&mission_dir, &outside).unwrap();
    std::os::unix::fs::symlink(&outside, &mission_dir).unwrap();
    let approvals = outside
        .join("missions")
        .join(f.id.to_string())
        .join("approvals");
    let before = tree_files(&approvals);
    assert_eq!(
        f.code(EntryPath::Activation),
        "approval_authority_unavailable"
    );
    assert_eq!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "c9")
            .unwrap_err()
            .code(),
        "approval_authority_unavailable"
    );
    assert!(
        f.approval()
            .record_review(
                f.id,
                ReviewInput {
                    kind: ReviewKind::ColdWalkthrough,
                    verdict: ReviewVerdict::Pass,
                    reviewer: "x".into(),
                    revision: 1,
                    plan_digest: "0".repeat(64),
                    evidence_identity: "0".repeat(64),
                    cold_review_id: None,
                }
            )
            .is_err()
    );
    let lifecycle = MissionLifecycle::open(&f.ws);
    assert!(lifecycle.begin_approval_transaction(f.id).is_err());
    assert!(lifecycle.lock_ticket_links(f.ticket).is_err());
    assert!(lifecycle.review_plan(f.id).is_err());
    assert!(
        lifecycle
            .apply_plan_operation(
                f.id,
                PlanOperationRequest {
                    request_id: "x".into(),
                    expected_revision: 1,
                    operation: PlanOperation::SetGoal {
                        title: "t".into(),
                        objective: "o".into()
                    },
                }
            )
            .is_err()
    );
    assert_eq!(
        execute(
            &f.ws,
            Operation::RecordProgress {
                mission_id: f.id,
                waypoint_id: "b".into(),
                state: WaypointState::Blocked,
                expected_current_revision: 1,
            }
        )
        .status,
        "error"
    );
    assert_eq!(
        WorkspaceLifecycle::open(&f.ws)
            .transition(
                &WaypointIdentity::Ticket {
                    ticket_id: f.ticket
                },
                &TransitionRequest {
                    op_id: Uuid::new_v4(),
                    to_native_state: "in-implementation".into(),
                    author: Some("w4-test".into())
                },
            )
            .unwrap_err()
            .code(),
        "approval_authority_unavailable"
    );
    assert_eq!(tree_files(&approvals), before);
}

#[cfg(unix)]
#[test]
fn symlinked_ancestor_blocks_draft_only_source_reads() {
    let f = linked();
    let managed = f.ws.join(".workflow-tools/mission");
    let accepted = managed
        .join("missions")
        .join(f.id.to_string())
        .join("accepted.json");
    fs::remove_file(accepted).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let moved = outside.path().join("mission");
    fs::rename(&managed, &moved).unwrap();
    std::os::unix::fs::symlink(&moved, &managed).unwrap();
    let before = tree_files(&moved);
    let lifecycle = MissionLifecycle::open(&f.ws);
    assert!(lifecycle.get_mission_source(f.id).is_err());
    let result = execute(&f.ws, Operation::GetMissionSource { mission_id: f.id });
    assert_eq!(result.status, "error");
    assert_eq!(tree_files(&moved), before);
}
