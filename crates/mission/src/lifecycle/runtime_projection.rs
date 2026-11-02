use super::*;
use mission_api::{MissionControlState, MissionLifecycle};

impl WorkspaceLifecycle {
    pub(super) fn runtime_state(&self, id: Uuid) -> Result<MissionControlState, LifecycleError> {
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(controls::api_error)?;
        let mut state = tx.control_state()?;
        if !state.completions.is_empty() {
            state.invalidate_other_plans(
                &tx.review_plan().map_err(controls::api_error)?.plan_digest,
            );
        }
        Ok(state)
    }

    pub(super) fn runtime_waypoint(
        runtime: &MissionControlState,
        waypoint_id: &str,
        native: LifecycleState,
    ) -> LifecycleState {
        if runtime.reopened.contains(waypoint_id) {
            return LifecycleState::Planned;
        }
        if runtime
            .completions
            .iter()
            .any(|c| c.current && c.waypoint_id == waypoint_id)
        {
            return LifecycleState::Completed;
        }
        native
    }
}

impl LifecycleRead for WorkspaceLifecycle {
    fn read_lifecycle(
        &self,
        identity: &WaypointIdentity,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let mut snapshot = self.read_native_lifecycle(identity)?;
        let (id, waypoint) = match identity {
            WaypointIdentity::Ticket { .. } => return Ok(snapshot),
            WaypointIdentity::Mission { mission_id } => (*mission_id, None),
            WaypointIdentity::Waypoint {
                mission_id,
                waypoint_id,
            } => (*mission_id, Some(waypoint_id)),
        };
        let runtime = self.runtime_state(id)?;
        snapshot.control_sequence += runtime.sequence;
        if let Some(waypoint_id) = waypoint {
            // A linked Ticket retains its native snapshot. Mission completion
            // evidence affects eligibility, not the Ticket's native state.
            if snapshot.native_domain != NativeDomain::Ticket {
                snapshot.state = Self::runtime_waypoint(&runtime, waypoint_id, snapshot.state);
            }
        }
        if runtime.pending_reopen.is_some() || runtime.pending_handoff.is_some() {
            snapshot.eligibility = ExecutionEligibility::Denied {
                denial: EligibilityDenial::CrossStorePending,
            };
        } else if waypoint
            .is_some_and(|id| runtime.stale.contains(id) && !runtime.reopened.contains(id))
        {
            snapshot.eligibility = ExecutionEligibility::Denied {
                denial: EligibilityDenial::StaleEvidence,
            };
        } else if runtime.phase == LifecycleState::Blocked {
            snapshot.eligibility = ExecutionEligibility::Denied {
                denial: EligibilityDenial::State(LifecycleState::Blocked),
            };
        } else if matches!(snapshot.eligibility, ExecutionEligibility::Eligible) {
            snapshot.eligibility =
                ExecutionEligibility::effective(snapshot.state, &TargetControl::default());
        }
        Ok(snapshot)
    }
}
