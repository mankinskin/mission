use super::controls::{api_error, required_reason};
use super::*;
use mission_api::{MissionLifecycle, PendingHandoff, control_error};

impl WorkspaceLifecycle {
    pub fn handoff_session(
        &self,
        id: Uuid,
        source: &str,
        destination: &str,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        required_reason(request)?;
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(api_error)?;
        self.check_revision(&tx)?;
        let mut state = tx.control_state()?;
        if let Some(pending) = &state.pending_handoff {
            if pending.operation_id != request.op_id
                || pending.source != source
                || pending.destination != destination
                || pending.reason != required_reason(request)?
                || pending.expected_control_sequence != request.expected_control_sequence
            {
                return Err(control_error(
                    "cross_store_pending",
                    "another handoff is pending",
                ));
            }
        } else {
            self.check_cas(&tx, request)?;
            if state.active_session.as_deref().is_some_and(|s| s != source) {
                return Err(control_error(
                    "session_binding_conflict",
                    "source does not own active binding",
                ));
            }
            let session = self.session_store();
            session
                .check_mission_pin_transfer(
                    source,
                    destination,
                    &format!("ce://default/mission/{id}"),
                    request.op_id,
                )
                .map_err(api_error)?;
            if session
                .requester_session_finished(source)
                .map_err(api_error)?
                || session
                    .requester_session_finished(destination)
                    .map_err(api_error)?
            {
                return Err(control_error(
                    "session_inactive",
                    "both Sessions must be active",
                ));
            }
            let expected = state.sequence;
            state.pending_handoff = Some(PendingHandoff {
                operation_id: request.op_id,
                source: source.into(),
                destination: destination.into(),
                reason: required_reason(request)?.into(),
                expected_control_sequence: request.expected_control_sequence,
            });
            tx.write_control_state(&mut state, expected)?;
        }
        self.session_store()
            .transfer_mission_pin(
                source,
                destination,
                &format!("ce://default/mission/{id}"),
                request.op_id,
            )
            .map_err(api_error)?;
        let expected = state.sequence;
        state.active_session = Some(destination.into());
        state.pending_handoff = None;
        state.operations.insert(request.op_id);
        tx.write_control_state(&mut state, expected)?;
        drop(tx);
        self.read_lifecycle(&WaypointIdentity::Mission { mission_id: id })
    }
}

impl HandoffCapability for WorkspaceLifecycle {
    fn hand_off(
        &self,
        identity: &WaypointIdentity,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            source_session: String,
            destination_session: String,
            reason: String,
        }
        let input: Input = serde_json::from_str(required_reason(request)?)
            .map_err(|e| control_error("handoff_input_invalid", e.to_string()))?;
        self.handoff_session(
            controls::mission_id(identity)?,
            &input.source_session,
            &input.destination_session,
            &ControlRequest {
                evidence: Some(input.reason),
                ..request.clone()
            },
        )
    }
}
