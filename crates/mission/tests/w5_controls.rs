use mission::{
    CommitCheckpoint, ControlRequest, DirectWaypoint, LifecycleRead, LifecycleState,
    MissionLifecycle, PauseResumeCapability, WaypointIdentity, WorkspaceLifecycle,
};
use uuid::Uuid;

#[test]
fn pause_is_durable_and_stale_cas_is_rejected() {
    let root = tempfile::tempdir_in(".").unwrap();
    let id = MissionLifecycle::open(root.path())
        .create(
            "Mission".into(),
            "Goal".into(),
            vec![DirectWaypoint {
                waypoint_id: "one".into(),
                title: "One".into(),
                depends_on: vec![],
                prompt: "Do one".into(),
                validation: vec![mission::ValidationGate {
                    command: "true".into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: "one".into(),
                },
                non_goals: vec![],
            }],
        )
        .unwrap()
        .mission_id;
    let identity = WaypointIdentity::Mission { mission_id: id };
    let request = ControlRequest {
        op_id: Uuid::new_v4(),
        expected_control_sequence: 0,
        evidence: Some("Requester paused for inspection".into()),
    };
    let adapter = WorkspaceLifecycle::open(root.path());
    adapter.pause(&identity, &request).unwrap();
    assert_eq!(
        WorkspaceLifecycle::open(root.path())
            .read_lifecycle(&identity)
            .unwrap()
            .state,
        LifecycleState::Paused
    );
    let error = adapter
        .pause(
            &identity,
            &ControlRequest {
                op_id: Uuid::new_v4(),
                ..request
            },
        )
        .unwrap_err();
    assert_eq!(error.code(), "stale_control_sequence");
}
