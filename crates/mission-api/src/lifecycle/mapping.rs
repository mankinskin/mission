//! Native mappings remain outside the neutral kernel.
use super::{LifecycleError, LifecycleState};
use crate::WaypointState;
use memory_kernel::lifecycle::NativeLifecycle;
use ticket_api::TicketLifecycle;

pub trait LifecycleStateExt {
    fn from_ticket_state(state: &str) -> Result<LifecycleState, LifecycleError>;
    fn from_waypoint_state(state: &WaypointState) -> LifecycleState;
}

impl LifecycleStateExt for LifecycleState {
    /// Maps a Ticket-native state. Unknown states are explicit errors.
    fn from_ticket_state(state: &str) -> Result<Self, LifecycleError> {
        TicketLifecycle.shared_state(state).map_err(Into::into)
    }

    /// Maps the legacy single-session native state; paused/cancelled never
    /// come from this enum.
    fn from_waypoint_state(state: &WaypointState) -> Self {
        match state {
            WaypointState::Pending => Self::Planned,
            WaypointState::InProgress => Self::Active,
            WaypointState::Completed => Self::Completed,
            WaypointState::Blocked => Self::Blocked,
        }
    }
}
