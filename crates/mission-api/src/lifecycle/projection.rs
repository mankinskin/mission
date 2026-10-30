use super::*;
use crate::WaypointState;

// ---------------------------------------------------------------------------
// Projections
// ---------------------------------------------------------------------------

/// Projects a single-session waypoint from its native state and controls.
/// Precedence: terminal cancel/complete, pause, then the native mapping.
pub fn project_waypoint(
    native: &WaypointState,
    control: &TargetControl,
) -> Result<LifecycleState, LifecycleError> {
    let base = LifecycleState::from_waypoint_state(native);
    let conflict = |reason: &str| LifecycleError::InvalidControlHistory {
        seq: control.last_seq,
        reason: reason.to_string(),
    };
    if let Some(terminal) = control.terminal {
        if base == LifecycleState::Completed && terminal == LifecycleState::Cancelled {
            return Err(conflict(
                "cancelled control conflicts with completed native state",
            ));
        }
        return Ok(terminal);
    }
    if let Some(paused_from) = control.paused_from {
        if base == LifecycleState::Completed {
            return Err(conflict(
                "paused control conflicts with completed native state",
            ));
        }
        if paused_from != base {
            return Err(conflict("native state changed while paused"));
        }
        return Ok(LifecycleState::Paused);
    }
    Ok(base)
}

/// Facts a Mission projection is computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionFacts {
    /// Shared state of each ordered waypoint (Ticket-backed ones as read from
    /// the Ticket store).
    pub waypoints: Vec<LifecycleState>,
    /// A required entry cannot progress.
    pub required_entry_unavailable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissionProjection {
    pub state: LifecycleState,
    pub result_review_needed: bool,
}

/// Mission projection. Precedence: terminal cancel/complete, pause, blocked
/// entry, then planned/active. Completion only comes from an explicit event
/// and is rejected unless every required waypoint outcome is completed; a
/// cancelled or blocked first unsatisfied required entry blocks the Mission.
pub fn project_mission(
    facts: &MissionFacts,
    control: &TargetControl,
) -> Result<MissionProjection, LifecycleError> {
    let done = |state| {
        Ok(MissionProjection {
            state,
            result_review_needed: false,
        })
    };
    let all_completed = !facts.waypoints.is_empty()
        && facts
            .waypoints
            .iter()
            .all(|s| *s == LifecycleState::Completed);
    if let Some(terminal) = control.terminal {
        if terminal == LifecycleState::Completed && !all_completed {
            return Err(LifecycleError::InvalidControlHistory {
                seq: control.last_seq,
                reason: "completion event while required waypoint outcomes are not all completed"
                    .into(),
            });
        }
        return done(terminal);
    }
    if control.paused_from.is_some() {
        return done(LifecycleState::Paused);
    }
    let entry_unsatisfied = facts
        .waypoints
        .iter()
        .find(|s| **s != LifecycleState::Completed)
        .is_some_and(|s| matches!(s, LifecycleState::Blocked | LifecycleState::Cancelled));
    if facts.required_entry_unavailable || entry_unsatisfied {
        return done(LifecycleState::Blocked);
    }
    if all_completed {
        return Ok(MissionProjection {
            state: LifecycleState::Active,
            result_review_needed: true,
        });
    }
    if facts
        .waypoints
        .iter()
        .any(|s| *s != LifecycleState::Planned)
    {
        return done(LifecycleState::Active);
    }
    done(LifecycleState::Planned)
}
