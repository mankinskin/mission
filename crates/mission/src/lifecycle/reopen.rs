use super::controls::{api_error, required_reason};
use super::*;
use mission_api::{MissionLifecycle, PendingReopen, control_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReopenStep {
    BeforeTicketTransition,
    AfterTicketTransition,
    BeforeMissionFinalization,
}

impl WorkspaceLifecycle {
    /// Compare both the route revision and immutable completion identity.
    /// Interrupted operations must be retried with the identical request.
    pub fn reopen_most_recent_completed(
        &self,
        id: Uuid,
        expected_revision: u64,
        expected_completion: u64,
        request: &ControlRequest,
        fault: Option<ReopenStep>,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let reason = required_reason(request)?;
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(api_error)?;
        let mut state = tx.control_state()?;
        let accepted = MissionStore::open(&self.workspace_root)
            .get(id)
            .map_err(api_error)?;
        let plan = tx.review_plan().map_err(api_error)?;
        if plan.revision != expected_revision {
            return Err(control_error("stale_plan_revision", "route changed"));
        }
        let pending = if let Some(pending) = &state.pending_reopen {
            if pending.operation_id != request.op_id
                || pending.completion_sequence != expected_completion
                || pending.reason != reason
                || pending.expected_plan_revision != expected_revision
                || pending.expected_control_sequence != request.expected_control_sequence
            {
                return Err(control_error(
                    "cross_store_pending",
                    "another reopen is pending",
                ));
            }
            pending.clone()
        } else {
            self.check_cas(&tx, request)?;
            let controls = LifecycleJournal::read(&self.workspace_root, id)?.projection()?;
            if controls.target(&ControlTarget::Mission).terminal == Some(LifecycleState::Cancelled)
            {
                return Err(control_error(
                    "illegal_transition",
                    "cancelled Mission cannot reopen",
                ));
            }
            state.invalidate_other_plans(&plan.plan_digest);
            let completion = state.latest_completion()?;
            if completion.sequence != expected_completion
                || completion.plan_digest != plan.plan_digest
            {
                return Err(control_error(
                    "stale_completion",
                    "latest current completion changed",
                ));
            }
            let waypoint = Self::find_waypoint(&accepted, id, &completion.waypoint_id)?;
            let (ticket_id, ticket_revision) = if let Some(reference) = Self::ticket_of(waypoint) {
                let ticket_id = self.ticket_id(reference)?;
                let ticket = self.read_ticket(ticket_id)?;
                if ticket.native_state != "done" {
                    return Err(control_error(
                        "ticket_transition_rejected",
                        "linked Ticket is not done",
                    ));
                }
                (Some(ticket_id), Some(ticket.revision))
            } else if waypoint.child_mission_ref.is_some() {
                return Err(control_error(
                    "ticket_transition_rejected",
                    "child Mission requires its owning lifecycle",
                ));
            } else {
                (None, None)
            };
            let mut stale_waypoints = BTreeSet::from([completion.waypoint_id.clone()]);
            loop {
                let prior = stale_waypoints.len();
                for waypoint in &plan.waypoints {
                    if waypoint
                        .depends_on
                        .iter()
                        .any(|dep| stale_waypoints.contains(dep))
                    {
                        stale_waypoints.insert(waypoint.waypoint_id.clone());
                    }
                }
                if stale_waypoints.len() == prior {
                    break;
                }
            }
            let pending = PendingReopen {
                operation_id: request.op_id,
                completion_sequence: completion.sequence,
                waypoint_id: completion.waypoint_id.clone(),
                reason: reason.into(),
                ticket_id,
                ticket_revision,
                stale_waypoints,
                expected_plan_revision: expected_revision,
                expected_control_sequence: request.expected_control_sequence,
            };
            let expected = state.sequence;
            state.pending_reopen = Some(pending.clone());
            tx.write_control_state(&mut state, expected)?;
            pending
        };
        let interrupt = |step| {
            if fault == Some(step) {
                Err(control_error(
                    "cross_store_pending",
                    format!("injected interruption at {step:?}"),
                ))
            } else {
                Ok(())
            }
        };
        interrupt(ReopenStep::BeforeTicketTransition)?;
        if let Some(ticket_id) = pending.ticket_id {
            let revision = pending
                .ticket_revision
                .ok_or_else(|| LifecycleError::JournalCorrupt("missing Ticket revision".into()))?;
            self.ticket_store()?
                .reopen_done(
                    &ticket_id,
                    revision,
                    &pending.reason,
                    pending.operation_id,
                    None,
                )
                .map_err(|e| control_error("ticket_transition_rejected", e.to_string()))?;
            interrupt(ReopenStep::AfterTicketTransition)?;
            let read = self.read_ticket(ticket_id)?;
            if read.native_state != "ready" || read.revision != revision + 1 {
                return Err(control_error(
                    "ticket_transition_rejected",
                    "Ticket read-back disagrees",
                ));
            }
        } else {
            interrupt(ReopenStep::AfterTicketTransition)?;
        }
        interrupt(ReopenStep::BeforeMissionFinalization)?;
        let session = self.session_store();
        crate::approval::MissionApproval::open(&self.workspace_root, &session)
            .invalidate_locked(&tx, pending.operation_id, &pending.reason)
            .map_err(Self::gate_error)?;
        for completion in &mut state.completions {
            if pending.stale_waypoints.contains(&completion.waypoint_id) {
                completion.current = false;
            }
        }
        state.reopened.insert(pending.waypoint_id.clone());
        state.stale.extend(pending.stale_waypoints);
        state.phase = LifecycleState::Planned;
        state.phase_reason = Some(pending.reason.clone());
        state.result_review = None;
        state.operations.insert(pending.operation_id);
        state.pending_reopen = None;
        let expected = state.sequence;
        tx.write_control_state(&mut state, expected)?;
        drop(tx);
        self.read_lifecycle(&WaypointIdentity::Waypoint {
            mission_id: id,
            waypoint_id: pending.waypoint_id,
        })
    }
}

impl ReopenCapability for WorkspaceLifecycle {
    fn reopen(
        &self,
        identity: &WaypointIdentity,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let WaypointIdentity::Mission { mission_id } = identity else {
            return Err(LifecycleError::unsupported(Capability::Reopen, PROVIDER));
        };
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            expected_revision: u64,
            completion_sequence: u64,
            reason: String,
        }
        let input: Input = serde_json::from_str(required_reason(request)?)
            .map_err(|e| control_error("reopen_input_invalid", e.to_string()))?;
        self.reopen_most_recent_completed(
            *mission_id,
            input.expected_revision,
            input.completion_sequence,
            &ControlRequest {
                evidence: Some(input.reason),
                ..request.clone()
            },
            None,
        )
    }
}
