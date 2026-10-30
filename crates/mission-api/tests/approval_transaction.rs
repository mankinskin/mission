use mission_api::*;
use std::{
    fs,
    sync::{Arc, mpsc},
    time::Duration,
};
use tempfile::{TempDir, tempdir};
use uuid::Uuid;

fn seeded() -> (TempDir, MissionLifecycle, Uuid) {
    let dir = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(dir.path());
    let mut draft = lifecycle
        .create_draft("Plan".into(), "Goal".into())
        .unwrap();
    let id = draft.mission_id;
    draft.waypoints = vec![TypedWaypoint {
        waypoint_id: "a".into(),
        title: "a title".into(),
        prompt: "a prompt".into(),
        part: PartSource {
            waypoint_id: "a".into(),
            document: Document {
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text {
                        text: "a body".into(),
                    }],
                }],
            },
        },
        validation: vec![ValidationGate {
            command: "cargo test".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: "a checkpoint".into(),
        },
        non_goals: vec!["a non-goal".into()],
        ..Default::default()
    }];
    lifecycle.publish_draft(id, draft, 1).unwrap();
    (dir, lifecycle, id)
}

fn set_goal(revision: u64, request: &str, title: &str, objective: &str) -> PlanOperationRequest {
    PlanOperationRequest {
        request_id: request.into(),
        expected_revision: revision,
        operation: PlanOperation::SetGoal {
            title: title.into(),
            objective: objective.into(),
        },
    }
}

fn evidence(revision: u64, plan_digest: &str) -> PlanOperationRequest {
    PlanOperationRequest {
        request_id: "ev1".into(),
        expected_revision: revision,
        operation: PlanOperation::RecordEvidence {
            plan_digest: plan_digest.into(),
            result: OperatorResult {
                waypoint_id: "a".into(),
                outcome: "passed".into(),
                operator_command: "cargo test".into(),
                evidence_pointer: "ce://default/test/receipt-a".into(),
            },
        },
    }
}

#[test]
fn transaction_serializes_writers_and_shared_readers_until_released() {
    let (_dir, lifecycle, id) = seeded();
    let lifecycle = Arc::new(lifecycle);
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    let held = tx.review_plan().unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    for kind in ["writer", "reader"] {
        let (lc, ready, done, revision) = (
            lifecycle.clone(),
            ready_tx.clone(),
            done_tx.clone(),
            held.revision,
        );
        std::thread::spawn(move || {
            ready.send(kind).unwrap();
            let outcome = if kind == "writer" {
                lc.apply_plan_operation(id, set_goal(revision, "w1", "second", "Goal"))
                    .map(|_| ())
            } else {
                lc.review_plan(id).map(|_| ())
            };
            done.send((kind, outcome.is_ok())).unwrap();
        });
    }
    for _ in 0..2 {
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker started");
    }
    assert!(done_rx.recv_timeout(Duration::from_millis(400)).is_err());
    assert_eq!(tx.review_plan().unwrap().revision, held.revision);
    drop(tx);
    for _ in 0..2 {
        let (_, ok) = done_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("worker completes");
        assert!(ok);
    }
    let again = lifecycle.begin_approval_transaction(id).unwrap();
    assert!(again.review_plan().unwrap().revision > held.revision);
}

#[test]
fn plan_read_inside_the_transaction_does_not_reacquire_the_lock() {
    let (_dir, lifecycle, id) = seeded();
    let lifecycle = Arc::new(lifecycle);
    let (sender, receiver) = mpsc::channel();
    let lc = lifecycle.clone();
    std::thread::spawn(move || {
        let tx = lc.begin_approval_transaction(id).unwrap();
        sender
            .send(tx.review_plan().map(|plan| plan.plan_digest))
            .unwrap();
    });
    let digest = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("no deadlock")
        .unwrap();
    assert!(!digest.is_empty());
    assert!(lifecycle.review_plan(id).is_ok());
}

#[test]
fn absent_mission_is_not_found() {
    let dir = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(dir.path());
    let error = lifecycle
        .begin_approval_transaction(Uuid::new_v4())
        .err()
        .unwrap();
    assert!(matches!(error, MissionError::NotFound(_)), "{error:?}");
}

#[test]
fn records_are_create_new_atomic_and_leave_no_temp_files() {
    let (_dir, lifecycle, id) = seeded();
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    assert_eq!(tx.read_record("g/one.json").unwrap(), None);
    tx.write_record_new("g/one.json", b"{\"a\":1}").unwrap();
    let duplicate = tx.write_record_new("g/one.json", b"{\"a\":2}").unwrap_err();
    assert_eq!(
        duplicate.diagnostics().unwrap()[0].code,
        "approval-record-exists"
    );
    assert_eq!(tx.read_record("g/one.json").unwrap().unwrap(), b"{\"a\":1}");
    assert_eq!(tx.list_records("g").unwrap(), vec!["one.json".to_string()]);
    assert_eq!(
        fs::read_dir(lifecycle.mission_directory(id).join("approvals/g"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn hostile_record_names_are_rejected() {
    let (_dir, lifecycle, id) = seeded();
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    for name in [
        "", "../x", "a/../x", "/abs", ".hidden", "a//b", "a b", "a/.tmp-x",
    ] {
        let error = tx.write_record_new(name, b"x").unwrap_err();
        assert_eq!(
            error.diagnostics().unwrap()[0].code,
            "approval-record-name",
            "{name}"
        );
        assert!(tx.read_record(name).is_err(), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn symlinked_record_directory_is_refused_not_followed() {
    use std::os::unix::fs::symlink;
    let (dir, lifecycle, id) = seeded();
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    tx.write_record_new("g/one.json", b"1").unwrap();
    let base = lifecycle.mission_directory(id).join("approvals");
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("one.json"), b"foreign").unwrap();
    fs::remove_dir_all(base.join("g")).unwrap();
    symlink(&elsewhere, base.join("g")).unwrap();
    assert!(tx.read_record("g/one.json").is_err());
    assert!(tx.write_record_new("g/two.json", b"2").is_err());
    assert!(!elsewhere.join("two.json").exists());
}

#[test]
fn records_survive_publication_and_semantic_revision_never_reverts() {
    let (_dir, lifecycle, id) = seeded();
    {
        let tx = lifecycle.begin_approval_transaction(id).unwrap();
        tx.write_record_new("journal/00000001.json", b"{}").unwrap();
    }
    let plan = lifecycle.review_plan(id).unwrap();
    let base = lifecycle
        .begin_approval_transaction(id)
        .unwrap()
        .semantic_revision()
        .unwrap();
    assert_eq!(base, plan.revision);
    lifecycle
        .apply_plan_operation(id, evidence(plan.revision, &plan.plan_digest))
        .unwrap();
    let after_evidence = lifecycle.review_plan(id).unwrap();
    assert!(after_evidence.revision > plan.revision);
    assert_eq!(after_evidence.plan_digest, plan.plan_digest);
    {
        let tx = lifecycle.begin_approval_transaction(id).unwrap();
        assert_eq!(tx.semantic_revision().unwrap(), base);
        assert_eq!(
            tx.read_record("journal/00000001.json").unwrap().unwrap(),
            b"{}"
        );
    }
    lifecycle
        .apply_plan_operation(id, set_goal(after_evidence.revision, "e1", "Other", "Goal"))
        .unwrap();
    let edited = lifecycle.review_plan(id).unwrap();
    assert_ne!(edited.plan_digest, plan.plan_digest);
    lifecycle
        .apply_plan_operation(id, set_goal(edited.revision, "e2", "Plan", "Goal"))
        .unwrap();
    let reverted = lifecycle.review_plan(id).unwrap();
    assert_eq!(reverted.plan_digest, plan.plan_digest);
    let tx = lifecycle.begin_approval_transaction(id).unwrap();
    assert!(tx.semantic_revision().unwrap() > base);
    assert_eq!(
        tx.read_record("journal/00000001.json").unwrap().unwrap(),
        b"{}"
    );
}
