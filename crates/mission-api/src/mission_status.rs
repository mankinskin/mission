use super::*;

impl MissionStore {
    pub fn derived_state(&self, mission_id: Uuid) -> Result<WaypointState, MissionError> {
        self.derived_state_inner(mission_id, &mut BTreeSet::new())
    }

    /// Materialize externally owned ticket state and recursive child state into
    /// an ephemeral rendering bundle; accepted records are never changed.
    pub fn render_with_status_snapshot(
        &self,
        mission_id: Uuid,
        ticket_states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<RenderedRoadmap, MissionError> {
        let accepted = self.get(mission_id)?;
        self.render_bundle_with_status_snapshot(
            mission_id,
            &accepted.bundle,
            ticket_states,
            snapshot_id,
        )
    }

    pub fn derived_state_with_status_snapshot(
        &self,
        mission_id: Uuid,
        ticket_states: &BTreeMap<String, WaypointState>,
    ) -> Result<WaypointState, MissionError> {
        let accepted = self.get(mission_id)?;
        let mut bundle = accepted.bundle;
        self.materialize_accepted_bundle(mission_id, &mut bundle, ticket_states)?;
        Ok(first_incomplete_state(&bundle))
    }

    pub(super) fn render_bundle_with_status_snapshot(
        &self,
        mission_id: Uuid,
        source_bundle: &MissionBundle,
        ticket_states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<RenderedRoadmap, MissionError> {
        validate_bundle(source_bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let mut bundle = source_bundle.clone();
        self.materialize_accepted_bundle(mission_id, &mut bundle, ticket_states)?;
        self.render_projection(&bundle, mission_id, Some(snapshot_id))
    }

    fn materialize_accepted_bundle(
        &self,
        mission_id: Uuid,
        bundle: &mut MissionBundle,
        ticket_states: &BTreeMap<String, WaypointState>,
    ) -> Result<(), MissionError> {
        self.validate_child_missions(mission_id, bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let mut visiting = BTreeSet::from([mission_id]);
        self.materialize_states(bundle, ticket_states, &mut visiting)
    }

    fn materialize_states(
        &self,
        bundle: &mut MissionBundle,
        ticket_states: &BTreeMap<String, WaypointState>,
        visiting: &mut BTreeSet<Uuid>,
    ) -> Result<(), MissionError> {
        for waypoint in &mut bundle.waypoints {
            if let Some(reference) = &waypoint.child_mission_ref {
                let child_id = mission_reference_id(reference).expect("validated child reference");
                if !visiting.insert(child_id) {
                    return Err(MissionError::Validation(
                        1,
                        vec![diag(
                            "child-mission-cycle",
                            Path::new("accepted.json"),
                            "",
                            Some(&waypoint.waypoint_id),
                            "Child mission references must not form a cycle.",
                        )],
                    ));
                }
                let mut child = self.get(child_id)?.bundle;
                self.materialize_states(&mut child, ticket_states, visiting)?;
                waypoint.state = Some(first_incomplete_state(&child));
                visiting.remove(&child_id);
            } else if let Some(reference) = waypoint.ticket_ref.as_ref().or(match &waypoint.scope {
                Scope::TicketBacked { ticket_ref } => Some(ticket_ref),
                Scope::SingleSession => None,
            }) {
                let Some(state) = ticket_states.get(reference).cloned() else {
                    return Err(MissionError::Validation(
                        1,
                        vec![diag(
                            "external-status-unresolved",
                            Path::new("accepted.json"),
                            "/ticket_ref",
                            Some(&waypoint.waypoint_id),
                            &format!(
                                "Ticket status snapshot does not contain canonical reference `{reference}`."
                            ),
                        )],
                    ));
                };
                waypoint.state = Some(state);
            }
        }
        Ok(())
    }

    fn derived_state_inner(
        &self,
        mission_id: Uuid,
        visiting: &mut BTreeSet<Uuid>,
    ) -> Result<WaypointState, MissionError> {
        if !visiting.insert(mission_id) {
            return Err(MissionError::Validation(
                1,
                vec![diag(
                    "child-mission-cycle",
                    Path::new("accepted.json"),
                    "",
                    None,
                    "Child mission references must not form a cycle.",
                )],
            ));
        }
        let accepted = self.get(mission_id)?;
        for waypoint_id in &accepted.bundle.manifest.execution_order {
            let waypoint = accepted
                .bundle
                .waypoints
                .iter()
                .find(|waypoint| waypoint.waypoint_id == *waypoint_id)
                .expect("accepted order is valid");
            let state = match &waypoint.child_mission_ref {
                Some(reference) => {
                    let child =
                        mission_reference_id(reference).expect("accepted child reference is valid");
                    self.derived_state_inner(child, visiting)?
                }
                None => waypoint.state.clone().unwrap_or(WaypointState::Pending),
            };
            if state != WaypointState::Completed {
                visiting.remove(&mission_id);
                return Ok(state);
            }
        }
        visiting.remove(&mission_id);
        Ok(WaypointState::Completed)
    }

    pub(super) fn validate_child_missions(
        &self,
        mission_id: Uuid,
        bundle: &MissionBundle,
    ) -> Result<(), Vec<Diagnostic>> {
        let workspace = self
            .root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let mut diagnostics = Vec::new();
        self.validate_child_missions_inner(
            mission_id,
            bundle,
            workspace,
            &mut BTreeSet::new(),
            &mut diagnostics,
        );
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }

    fn validate_child_missions_inner(
        &self,
        mission_id: Uuid,
        bundle: &MissionBundle,
        workspace: &str,
        visiting: &mut BTreeSet<Uuid>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !visiting.insert(mission_id) {
            diagnostics.push(diag(
                "child-mission-cycle",
                Path::new("manifest.json"),
                "",
                None,
                "Child mission references must not form a cycle.",
            ));
            return;
        }
        for waypoint in &bundle.waypoints {
            let Some(reference) = &waypoint.child_mission_ref else {
                continue;
            };
            let Some((reference_workspace, child_id)) = mission_reference(reference) else {
                continue;
            };
            if reference_workspace != workspace && reference_workspace != "default" {
                diagnostics.push(diag(
                    "cross-workspace-child-mission",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child missions must resolve from the selected workspace mission store.",
                ));
                continue;
            }
            if visiting.contains(&child_id) {
                diagnostics.push(diag(
                    "child-mission-cycle",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child mission references must not form a cycle.",
                ));
                continue;
            }
            match self.get(child_id) {
                Ok(child) if child.bundle.waypoints.is_empty() => diagnostics.push(diag(
                    "empty-child-mission",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child missions must contain at least one waypoint.",
                )),
                Ok(child) => self.validate_child_missions_inner(
                    child_id,
                    &child.bundle,
                    workspace,
                    visiting,
                    diagnostics,
                ),
                Err(MissionError::NotFound(_)) => diagnostics.push(diag(
                    "unaccepted-child-mission",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child mission must be accepted in the selected workspace.",
                )),
                Err(error) => diagnostics.push(diag(
                    "child-mission-resolution-failed",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    &error.to_string(),
                )),
            }
        }
        visiting.remove(&mission_id);
    }
}

pub(super) fn copy_directory(source: &Path, destination: &Path) -> Result<(), MissionError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_symlink() {
            return Err(MissionError::Io(std::io::Error::other(format!(
                "refusing symlink in Mission payload: {}",
                entry.path().display()
            ))));
        } else if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
