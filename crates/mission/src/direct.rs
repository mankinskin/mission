use super::*;

pub use mission_api::DirectWaypoint;

pub(super) fn create(
    workspace: &Path,
    title: String,
    objective: String,
    waypoints: Vec<DirectWaypoint>,
) -> StatusSnapshot {
    let create =
        MissionLifecycle::open(workspace).create_with_identity(title, objective, waypoints);
    finish(Some(create.mission_id), create.result)
}

pub(super) fn edit_objective(
    workspace: &Path,
    id: Uuid,
    objective: String,
    expected: u64,
) -> StatusSnapshot {
    finish(
        Some(id),
        MissionLifecycle::open(workspace).edit_objective(id, objective, expected),
    )
}

pub(super) fn add_waypoint(
    workspace: &Path,
    id: Uuid,
    waypoint: DirectWaypoint,
    expected: u64,
) -> StatusSnapshot {
    finish(
        Some(id),
        MissionLifecycle::open(workspace).add_waypoint(id, waypoint, expected),
    )
}

pub(super) fn edit_waypoint(
    workspace: &Path,
    id: Uuid,
    waypoint: DirectWaypoint,
    expected: u64,
) -> StatusSnapshot {
    finish(
        Some(id),
        MissionLifecycle::open(workspace).edit_waypoint(id, waypoint, expected),
    )
}

pub(super) fn order_waypoints(
    workspace: &Path,
    id: Uuid,
    execution_order: Vec<String>,
    dependencies: BTreeMap<String, Vec<String>>,
    expected: u64,
) -> StatusSnapshot {
    finish(
        Some(id),
        MissionLifecycle::open(workspace).order_waypoints(
            id,
            execution_order,
            dependencies,
            expected,
        ),
    )
}

pub(super) fn record_progress(
    workspace: &Path,
    id: Uuid,
    waypoint_id: String,
    state: WaypointState,
    expected: u64,
) -> StatusSnapshot {
    finish(
        Some(id),
        MissionLifecycle::open(workspace).record_progress(id, &waypoint_id, state, expected),
    )
}

pub(super) fn record_result(
    workspace: &Path,
    id: Uuid,
    result: OperatorResult,
    expected: u64,
) -> StatusSnapshot {
    finish(
        Some(id),
        MissionLifecycle::open(workspace).record_result(id, result, expected),
    )
}

pub(super) fn validate(workspace: &Path, id: Uuid) -> StatusSnapshot {
    finish(Some(id), MissionLifecycle::open(workspace).validate(id))
}

pub(super) fn publish(workspace: &Path, id: Uuid, expected: u64) -> StatusSnapshot {
    let lifecycle = MissionLifecycle::open(workspace);
    let result = lifecycle.get(id).and_then(|accepted| {
        let store = MissionStore::open(workspace);
        let snapshot = ticket_snapshot(workspace, &store, id, &accepted.bundle)?;
        let states = snapshot_states(&snapshot)?;
        lifecycle.publish(id, expected, &states, &snapshot_id(&snapshot)?)
    });
    finish(Some(id), result)
}

pub(super) fn get(workspace: &Path, id: Uuid) -> StatusSnapshot {
    finish(Some(id), MissionLifecycle::open(workspace).get(id))
}
