use super::*;

#[test]
fn stale_revision_and_unknown_identity_preserve_authority_and_projection_bytes() {
    let f = seeded(3, &["a", "b", "c"]);
    rejected(
        &f,
        "r1",
        1,
        PlanOperation::SetGoal {
            title: "T".into(),
            objective: "O".into(),
        },
        "revision_conflict",
    );
    rejected(
        &f,
        "r2",
        2,
        PlanOperation::UpdateWaypoint {
            waypoint: waypoint("ghost"),
        },
        "waypoint_not_found",
    );
    rejected(
        &f,
        "r3",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "ghost".into(),
            replacement: None,
        },
        "waypoint_not_found",
    );
    rejected(
        &f,
        "r4",
        2,
        PlanOperation::ReorderWaypoints {
            order: vec!["c".into(), "b".into(), "ghost".into()],
        },
        "waypoint_not_found",
    );
    rejected(
        &f,
        "r5",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: waypoint("d"),
            after: Some("ghost".into()),
        },
        "waypoint_not_found",
    );
}

#[test]
fn request_id_replays_without_a_new_revision_and_rejects_reuse_with_different_content() {
    let f = seeded(3, &["a"]);
    let create = || PlanOperation::CreateWaypoint {
        waypoint: waypoint("b"),
        after: None,
    };
    let first = apply(&f, "req-1", 2, create()).unwrap();
    assert_eq!((first.revision, first.replayed), (3, false));
    let before = snapshot(f.root());
    let replay = apply(&f, "req-1", 2, create()).unwrap();
    assert!(replay.replayed);
    assert_eq!(
        (replay.revision, &replay.plan_digest),
        (3, &first.plan_digest)
    );
    assert_eq!(snapshot(f.root()), before);
    rejected(
        &f,
        "req-1",
        2,
        PlanOperation::SetGoal {
            title: "T".into(),
            objective: "different".into(),
        },
        "request_id_conflict",
    );
    apply(
        &f,
        "req-2",
        3,
        PlanOperation::SetGoal {
            title: "T".into(),
            objective: "later".into(),
        },
    )
    .unwrap();
    let later = snapshot(f.root());
    assert!(apply(&f, "req-1", 2, create()).unwrap().replayed);
    assert_eq!(snapshot(f.root()), later);
}
