use mission_api::{
    CommitCheckpoint, DirectWaypoint, MissionError, MissionLifecycle, MissionStore, OperatorResult,
    Scope, ValidationGate, WaypointState,
};
use std::collections::BTreeMap;
use tempfile::TempDir;

fn waypoint(id: &str, depends_on: Vec<&str>) -> DirectWaypoint {
    DirectWaypoint {
        waypoint_id: id.into(),
        title: format!("{id} title"),
        depends_on: depends_on.into_iter().map(str::to_owned).collect(),
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

#[test]
fn lifecycle_owns_create_and_mutations() {
    let temp = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(temp.path());
    let accepted = lifecycle
        .create(
            "Ownership".into(),
            "objective".into(),
            vec![waypoint("first", vec![])],
        )
        .unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 1);

    let id = accepted.mission_id;
    lifecycle.edit_objective(id, "edited".into(), 1).unwrap();
    lifecycle
        .add_waypoint(id, waypoint("second", vec!["first"]), 2)
        .unwrap();
    lifecycle
        .edit_waypoint(id, waypoint("second", vec!["first"]), 3)
        .unwrap();
    lifecycle
        .order_waypoints(
            id,
            vec!["first".into(), "second".into()],
            [("second".into(), vec!["first".into()])].into(),
            4,
        )
        .unwrap();
    lifecycle
        .record_progress(id, "first", WaypointState::Completed, 5)
        .unwrap();
    lifecycle
        .record_progress(id, "second", WaypointState::Completed, 6)
        .unwrap();
    lifecycle
        .record_result(
            id,
            OperatorResult {
                waypoint_id: "second".into(),
                outcome: "passed".into(),
                operator_command: "cargo test".into(),
                evidence_pointer: "target/evidence".into(),
            },
            7,
        )
        .unwrap();
    let accepted = lifecycle.validate(id).unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 8);
    assert!(matches!(
        lifecycle.edit_objective(id, "stale".into(), 7),
        Err(MissionError::StaleRevision {
            expected: 7,
            actual: 8
        })
    ));
    let accepted = lifecycle.get(id).unwrap();

    assert_eq!(accepted.bundle.manifest.revision, 8);
    assert_eq!(accepted.bundle.manifest.objective, "edited");
    assert_eq!(accepted.bundle.manifest.operator_results.len(), 1);
    let accepted = lifecycle.publish(id, 8, &BTreeMap::new(), "local").unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 9);
}

#[test]
fn lifecycle_rejects_external_waypoint_mutation() {
    let temp = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(temp.path());
    let accepted = lifecycle
        .create(
            "Ownership".into(),
            "objective".into(),
            vec![waypoint("first", vec![])],
        )
        .unwrap();
    let id = accepted.mission_id;
    let mut bundle = accepted.bundle;
    let waypoint = &mut bundle.waypoints[0];
    let ticket_ref = "ce://workspace/ticket/123e4567-e89b-12d3-a456-426614174000".to_owned();
    waypoint.ticket_ref = Some(ticket_ref.clone());
    waypoint.scope = Scope::TicketBacked { ticket_ref };
    waypoint.state = None;
    bundle.manifest.revision = 2;
    MissionStore::open(temp.path())
        .import(id, bundle, 1)
        .unwrap();

    let error = lifecycle
        .record_progress(id, "first", WaypointState::Completed, 2)
        .unwrap_err();
    let MissionError::Validation(_, diagnostics) = error else {
        panic!("expected externally-owned-waypoint validation error");
    };
    assert_eq!(diagnostics[0].code, "externally-owned-waypoint");
}

#[test]
fn rejected_create_retains_api_allocated_identity_without_a_record() {
    let temp = TempDir::new().unwrap();
    let lifecycle = MissionLifecycle::open(temp.path());
    let mut invalid = waypoint("first", vec![]);
    invalid.prompt.clear();

    let create =
        lifecycle.create_with_identity("Ownership".into(), "objective".into(), vec![invalid]);

    assert!(create.result.is_err());
    assert!(
        MissionStore::open(temp.path())
            .get(create.mission_id)
            .is_err()
    );
}
