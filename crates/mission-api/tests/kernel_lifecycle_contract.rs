//! The domain APIs consume one kernel contract rather than copying its types.
use memory_kernel::lifecycle::{LifecycleState, NativeLifecycle};
use mission_api::{ControlKind, LifecycleStateExt, TargetControl, WaypointState};
use ticket_api::TicketLifecycle;

#[test]
fn domains_share_kernel_state_and_control_types() {
    let state: LifecycleState =
        mission_api::LifecycleState::from_waypoint_state(&WaypointState::InProgress);
    assert_eq!(state, LifecycleState::Active);
    let control: memory_kernel::lifecycle::TargetControl = TargetControl::default();
    assert_eq!(control, memory_kernel::lifecycle::TargetControl::default());
    let kind: memory_kernel::lifecycle::ControlKind = ControlKind::Complete;
    assert_eq!(kind, memory_kernel::lifecycle::ControlKind::Complete);
}

#[test]
fn mission_uses_ticket_owned_native_mapping() {
    for native in [
        "planning",
        "ready",
        "in-implementation",
        "in-review",
        "on-hold",
        "blocked",
        "done",
        "cancelled",
    ] {
        assert_eq!(
            mission_api::LifecycleState::from_ticket_state(native).unwrap(),
            TicketLifecycle.shared_state(native).unwrap()
        );
    }
    let error = mission_api::LifecycleState::from_ticket_state("invented").unwrap_err();
    assert_eq!(error.code(), "ticket_state_unmapped");
}
