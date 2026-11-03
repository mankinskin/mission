use super::*;
use mission_api::{
    ApprovalTransaction, CompletionRecord, MissionControlState, MissionLifecycle, control_error,
};

pub(super) fn mission_id(identity: &WaypointIdentity) -> Result<Uuid, LifecycleError> {
    match identity {
        WaypointIdentity::Mission { mission_id } => Ok(*mission_id),
        _ => Err(control_error(
            "illegal_transition",
            "operation requires a Mission identity",
        )),
    }
}
pub(super) fn required_reason(request: &ControlRequest) -> Result<&str, LifecycleError> {
    request
        .evidence
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| control_error("reason_required", "nonempty evidence/reason is required"))
}
pub(super) fn api_error(e: impl std::fmt::Display) -> LifecycleError {
    LifecycleError::JournalIo(e.to_string())
}

impl WorkspaceLifecycle {
    pub(super) fn session_store(&self) -> SessionStoreConfig {
        SessionStoreConfig::new(self.workspace_root.join(".workflow-tools/session"))
    }

    pub(super) fn check_cas(
        &self,
        tx: &ApprovalTransaction<'_>,
        request: &ControlRequest,
    ) -> Result<MissionControlState, LifecycleError> {
        self.check_revision(tx)?;
        let state = tx.control_state()?;
        let journal = LifecycleJournal::read(&self.workspace_root, tx.mission_id())?;
        let sequence = state.sequence + journal.projection()?.last_seq;
        if sequence != request.expected_control_sequence {
            return Err(control_error(
                "stale_control_sequence",
                format!(
                    "expected {}, current {sequence}",
                    request.expected_control_sequence
                ),
            ));
        }
        state.ensure_settled()?;
        if request.op_id.is_nil()
            || state.operations.contains(&request.op_id)
            || journal.events.iter().any(|e| e.op_id == request.op_id)
        {
            return Err(control_error(
                "operation_conflict",
                "operation identity already used or nil",
            ));
        }
        Ok(state)
    }

    pub(super) fn approve_locked(
        &self,
        tx: &ApprovalTransaction<'_>,
    ) -> Result<(), LifecycleError> {
        let session = self.session_store();
        crate::approval::MissionApproval::open(&self.workspace_root, &session)
            .check_locked(tx)
            .map_err(Self::gate_error)?;
        Ok(())
    }

    pub fn activate(
        &self,
        id: Uuid,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(controls::api_error)?;
        let mut state = self.check_cas(&tx, request)?;
        let controls = LifecycleJournal::read(&self.workspace_root, id)?
            .projection()?
            .target(&ControlTarget::Mission);
        if state.phase != LifecycleState::Planned
            || controls.terminal.is_some()
            || controls.paused_from.is_some()
        {
            return Err(control_error(
                "illegal_transition",
                "activation requires planned Mission",
            ));
        }
        self.approve_locked(&tx)?;
        let expected = state.sequence;
        state.phase = LifecycleState::Active;
        state.phase_reason = request.evidence.clone();
        state.operations.insert(request.op_id);
        tx.write_control_state(&mut state, expected)?;
        drop(tx);
        self.read_lifecycle(&WaypointIdentity::Mission { mission_id: id })
    }

    pub fn block(
        &self,
        id: Uuid,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        required_reason(request)?;
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(api_error)?;
        let mut state = self.check_cas(&tx, request)?;
        if state.phase != LifecycleState::Active {
            return Err(control_error(
                "illegal_transition",
                "blocking requires active Mission",
            ));
        }
        let control = LifecycleJournal::read(&self.workspace_root, id)?
            .projection()?
            .target(&ControlTarget::Mission);
        if control.paused_from.is_some() || control.terminal.is_some() {
            return Err(control_error(
                "illegal_transition",
                "paused/terminal Mission cannot be blocked",
            ));
        }
        let expected = state.sequence;
        state.phase = LifecycleState::Blocked;
        state.phase_reason = request.evidence.clone();
        state.operations.insert(request.op_id);
        tx.write_control_state(&mut state, expected)?;
        drop(tx);
        self.read_lifecycle(&WaypointIdentity::Mission { mission_id: id })
    }

    pub fn complete_waypoint(
        &self,
        id: Uuid,
        waypoint_id: &str,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let result_ref = required_reason(request)?;
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(api_error)?;
        let mut state = self.check_cas(&tx, request)?;
        self.approve_locked(&tx)?;
        let plan = tx.review_plan().map_err(api_error)?;
        state.invalidate_other_plans(&plan.plan_digest);
        let waypoint = plan
            .waypoints
            .iter()
            .find(|w| w.waypoint_id == waypoint_id)
            .ok_or_else(|| control_error("waypoint_absent", waypoint_id))?;
        let controls = LifecycleJournal::read(&self.workspace_root, id)?
            .projection()?
            .target(&ControlTarget::Mission);
        if controls.terminal.is_some()
            || controls.paused_from.is_some()
            || matches!(
                state.phase,
                LifecycleState::Cancelled | LifecycleState::Completed | LifecycleState::Blocked
            )
        {
            return Err(control_error(
                "execution_gated",
                "Mission controls deny completion",
            ));
        }
        if state
            .completions
            .iter()
            .any(|c| c.current && c.waypoint_id == waypoint_id)
        {
            return Err(control_error(
                "illegal_transition",
                "waypoint already completed",
            ));
        }
        Self::check_prerequisites(&state, &plan, waypoint_id)?;
        if !waypoint
            .evidence
            .iter()
            .any(|e| e.current && e.evidence_pointer == result_ref)
        {
            return Err(control_error(
                "completion_evidence_required",
                "result must be current W3 evidence",
            ));
        }
        let accepted = MissionStore::open(&self.workspace_root)
            .get(id)
            .map_err(api_error)?;
        let native = Self::find_waypoint(&accepted, id, waypoint_id)?;
        if let Some(reference) = Self::ticket_of(native) {
            if self.read_ticket(self.ticket_id(reference)?)?.state != LifecycleState::Completed {
                return Err(control_error(
                    "ticket_transition_rejected",
                    "linked Ticket must be done",
                ));
            }
        } else if let Some(reference) = &native.child_mission_ref {
            let child = super::super::parse_reference(&self.workspace_root, reference, "mission")
                .map_err(api_error)?;
            if self.read_mission(child, &mut BTreeSet::from([id]))?.state
                != LifecycleState::Completed
            {
                return Err(control_error(
                    "completion_evidence_required",
                    "child Mission must be completed",
                ));
            }
        }
        let expected = state.sequence;
        let sequence = state.completions.last().map_or(1, |c| c.sequence + 1);
        state.completions.push(CompletionRecord {
            sequence,
            waypoint_id: waypoint_id.into(),
            plan_digest: plan.plan_digest,
            plan_revision: plan.revision,
            result_ref: result_ref.into(),
            current: true,
        });
        state.stale.remove(waypoint_id);
        state.reopened.remove(waypoint_id);
        state.result_review = None;
        state.operations.insert(request.op_id);
        tx.write_control_state(&mut state, expected)?;
        drop(tx);
        self.read_lifecycle(&WaypointIdentity::Waypoint {
            mission_id: id,
            waypoint_id: waypoint_id.into(),
        })
    }

    pub(crate) fn check_prerequisites(
        state: &MissionControlState,
        plan: &mission_api::ReviewPlan,
        waypoint_id: &str,
    ) -> Result<(), LifecycleError> {
        let waypoint = plan
            .waypoints
            .iter()
            .find(|w| w.waypoint_id == waypoint_id)
            .ok_or_else(|| control_error("waypoint_absent", waypoint_id))?;
        for dependency in &waypoint.depends_on {
            if state.stale.contains(dependency)
                || !state.completions.iter().any(|c| {
                    c.current && &c.waypoint_id == dependency && c.plan_digest == plan.plan_digest
                })
            {
                return Err(control_error("prerequisite_evidence_required", dependency));
            }
        }
        Ok(())
    }

    pub(super) fn guard_ticket_prerequisites(
        &self,
        guard: &crate::approval::EntryGuard<'_>,
        id: Uuid,
        ticket: Uuid,
    ) -> Result<(), LifecycleError> {
        let accepted = MissionStore::open(&self.workspace_root)
            .get(id)
            .map_err(api_error)?;
        for waypoint in &accepted.bundle.waypoints {
            if let Some(reference) = Self::ticket_of(waypoint)
                && self.ticket_id(reference)? == ticket
            {
                guard
                    .check_waypoint_entry(&waypoint.waypoint_id)
                    .map_err(Self::gate_error)?;
            }
        }
        Ok(())
    }

    fn control(
        &self,
        identity: &WaypointIdentity,
        request: &ControlRequest,
        kind: ControlKind,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let id = mission_id(identity)?;
        required_reason(request)?;
        // Read before taking the publication lock; CAS below validates the read.
        let current = self.read_lifecycle(identity)?;
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(api_error)?;
        let mut state = self.check_cas(&tx, request)?;
        if kind == ControlKind::Resume {
            self.approve_locked(&tx)?;
            let control = LifecycleJournal::read(&self.workspace_root, id)?
                .projection()?
                .target(&ControlTarget::Mission);
            if state.phase == LifecycleState::Blocked && control.paused_from.is_none() {
                let expected = state.sequence;
                state.phase = LifecycleState::Active;
                state.phase_reason = request.evidence.clone();
                state.operations.insert(request.op_id);
                tx.write_control_state(&mut state, expected)?;
                drop(tx);
                return self.read_lifecycle(identity);
            }
        }
        if kind == ControlKind::Complete {
            self.approve_locked(&tx)?;
            if !matches!(
                current.state,
                LifecycleState::Active | LifecycleState::Planned
            ) {
                return Err(control_error(
                    "illegal_transition",
                    "completion requires eligible Mission",
                ));
            }
            let plan = tx.review_plan().map_err(api_error)?;
            if state.result_review.is_none()
                || plan.waypoints.iter().any(|w| {
                    !state.completions.iter().any(|c| {
                        c.current
                            && c.waypoint_id == w.waypoint_id
                            && c.plan_digest == plan.plan_digest
                    })
                })
            {
                return Err(control_error(
                    "result_review_required",
                    "all current outcomes and result review required",
                ));
            }
            let expected = state.sequence;
            state.phase = LifecycleState::Completed;
            state.phase_reason = request.evidence.clone();
            state.operations.insert(request.op_id);
            tx.write_control_state(&mut state, expected)?;
            drop(tx);
            return self.read_lifecycle(identity);
        }
        let journal = LifecycleJournal::read(&self.workspace_root, id)?;
        let event = journal.next_event(
            ControlTarget::Mission,
            kind,
            current.state,
            request.op_id,
            request.evidence.clone(),
        )?;
        tx.append_control(&self.workspace_root, event)?;
        drop(tx);
        self.read_lifecycle(identity)
    }
}

impl PauseResumeCapability for WorkspaceLifecycle {
    fn pause(
        &self,
        id: &WaypointIdentity,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        self.control(id, request, ControlKind::Pause)
    }
    fn resume(
        &self,
        id: &WaypointIdentity,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        self.control(id, request, ControlKind::Resume)
    }
}
impl CancellationCapability for WorkspaceLifecycle {
    fn cancel(
        &self,
        id: &WaypointIdentity,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        self.control(id, request, ControlKind::Cancel)
    }
}
impl CompletionCapability for WorkspaceLifecycle {
    fn complete(
        &self,
        identity: &WaypointIdentity,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        if let WaypointIdentity::Waypoint {
            mission_id,
            waypoint_id,
        } = identity
        {
            return self.complete_waypoint(*mission_id, waypoint_id, request);
        }
        self.control(identity, request, ControlKind::Complete)
    }
    fn review_result(
        &self,
        identity: &WaypointIdentity,
        request: &ControlRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let id = mission_id(identity)?;
        let review = required_reason(request)?;
        let lifecycle = MissionLifecycle::open(&self.workspace_root);
        let tx = lifecycle
            .begin_approval_transaction(id)
            .map_err(api_error)?;
        let mut state = self.check_cas(&tx, request)?;
        let plan = tx.review_plan().map_err(api_error)?;
        if plan.waypoints.iter().any(|w| {
            !state.completions.iter().any(|c| {
                c.current && c.waypoint_id == w.waypoint_id && c.plan_digest == plan.plan_digest
            })
        }) {
            return Err(control_error(
                "completion_evidence_required",
                "review requires all current outcomes",
            ));
        }
        let expected = state.sequence;
        state.result_review = Some(review.into());
        state.operations.insert(request.op_id);
        tx.write_control_state(&mut state, expected)?;
        drop(tx);
        self.read_lifecycle(identity)
    }
}
