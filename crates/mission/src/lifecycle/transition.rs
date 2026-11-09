use super::*;

impl LifecycleTransition for WorkspaceLifecycle {
    /// Ticket owns its legal transitions and history; Mission only gates entry.
    fn transition(
        &self,
        identity: &WaypointIdentity,
        request: &TransitionRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let Some(ticket_id) = self.resolve_ticket(identity)? else {
            return Err(LifecycleError::unsupported(
                Capability::Transition,
                PROVIDER,
            ));
        };
        let session = SessionStoreConfig::new(self.workspace_root.join(".workflow-tools/session"));
        let approval = crate::approval::MissionApproval::open(&self.workspace_root, &session)
            .with_evidence_admission(self.evidence_admission.clone());
        let mut guards = Vec::new();
        let mut link_lock = None;
        if matches!(
            LifecycleState::from_ticket_state(&request.to_native_state),
            Ok(LifecycleState::Active)
        ) {
            let lifecycle = mission_api::MissionLifecycle::open(&self.workspace_root);
            let discover = || -> Result<BTreeSet<Uuid>, LifecycleError> {
                let mut missions = self.missions_linking(ticket_id)?;
                if let WaypointIdentity::Waypoint { mission_id, .. } = identity {
                    missions.insert(*mission_id);
                }
                Ok(missions)
            };
            let mut settled = false;
            for _ in 0..3 {
                drop(link_lock.take());
                guards.clear();
                let missions = discover()?;
                for mission_id in &missions {
                    let guard = approval
                        .guard_entry(*mission_id, crate::approval::EntryPath::Activation)
                        .map_err(Self::gate_error)?;
                    if let WaypointIdentity::Waypoint {
                        mission_id: selected,
                        ..
                    } = identity
                        && selected == mission_id
                        && let Some(expected) = self.expected_revision
                    {
                        guard.check_revision(expected).map_err(Self::gate_error)?;
                    }
                    self.guard_ticket_prerequisites(&guard, *mission_id, ticket_id)?;
                    guards.push(guard);
                }
                link_lock = Some(lifecycle.lock_ticket_links(ticket_id).map_err(|error| {
                    LifecycleError::ApprovalGate {
                        code: "approval_authority_unavailable",
                        message: error.to_string(),
                    }
                })?);
                if discover()? == missions {
                    settled = true;
                    break;
                }
            }
            if !settled {
                return Err(LifecycleError::ApprovalGate {
                    code: "approval_authority_unavailable",
                    message: "Mission links kept changing during entry".into(),
                });
            }
        }
        self.ticket_store()?
            .update_with_options(
                &ticket_id,
                Default::default(),
                None,
                Some(&request.to_native_state),
                None,
                None,
                request.author.as_deref(),
                true,
            )
            .map_err(|error| LifecycleError::TransitionRejected(error.to_string()))?;
        drop(link_lock);
        drop(guards);
        self.read_lifecycle(identity)
    }
}
