use super::*;

impl MissionLifecycle {
    pub fn open(workspace_root: &Path) -> Self {
        Self {
            store: MissionStore::open(workspace_root),
        }
    }

    pub fn create(
        &self,
        title: String,
        objective: String,
        waypoints: Vec<DirectWaypoint>,
    ) -> Result<AcceptedMission, MissionError> {
        self.create_with_identity(title, objective, waypoints)
            .result
    }

    /// Creates a mission while retaining the API-allocated identity on failure.
    pub fn create_with_identity(
        &self,
        title: String,
        objective: String,
        waypoints: Vec<DirectWaypoint>,
    ) -> CreateMissionResult {
        let mission_id = Uuid::new_v4();
        let bundle = MissionBundle {
            manifest: BundleManifest {
                schema_version: SCHEMA_VERSION,
                revision: 1,
                title,
                objective,
                waypoint_files: vec![],
                execution_order: waypoints
                    .iter()
                    .map(|waypoint| waypoint.waypoint_id.clone())
                    .collect(),
                artifacts: vec![],
                requirements: vec![],
                validation_gates: vec![],
                notes: vec![],
                operator_results: vec![],
            },
            waypoints: waypoints.into_iter().map(Self::fragment).collect(),
        };
        CreateMissionResult {
            mission_id,
            result: self.store.import(mission_id, bundle, 0),
        }
    }

    pub fn edit_objective(
        &self,
        mission_id: Uuid,
        objective: String,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            bundle.manifest.objective = objective;
            Ok(())
        })
    }

    pub fn add_waypoint(
        &self,
        mission_id: Uuid,
        waypoint: DirectWaypoint,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            bundle
                .manifest
                .execution_order
                .push(waypoint.waypoint_id.clone());
            bundle.waypoints.push(Self::fragment(waypoint));
            Ok(())
        })
    }

    pub fn edit_waypoint(
        &self,
        mission_id: Uuid,
        waypoint: DirectWaypoint,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            let item = Self::local_waypoint(bundle, &waypoint.waypoint_id)?;
            item.title = waypoint.title;
            item.depends_on = waypoint.depends_on;
            item.prompt = waypoint.prompt;
            item.validation = waypoint.validation;
            item.commit_checkpoint = waypoint.commit_checkpoint;
            item.non_goals = waypoint.non_goals;
            Ok(())
        })
    }

    pub fn order_waypoints(
        &self,
        mission_id: Uuid,
        execution_order: Vec<String>,
        dependencies: BTreeMap<String, Vec<String>>,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            for (waypoint_id, depends_on) in dependencies {
                Self::local_waypoint(bundle, &waypoint_id)?.depends_on = depends_on;
            }
            bundle.manifest.execution_order = execution_order;
            Ok(())
        })
    }

    pub fn record_progress(
        &self,
        mission_id: Uuid,
        waypoint_id: &str,
        state: WaypointState,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        let bundle = self.progress_bundle(mission_id, waypoint_id, state, expected)?;
        self.store.import_with(
            mission_id,
            bundle,
            expected,
            &typed_source::ImportOptions {
                authority: None,
                classify_nonsemantic: true,
            },
        )
    }

    pub(crate) fn record_progress_locked(
        &self,
        mission_id: Uuid,
        waypoint_id: &str,
        state: WaypointState,
        expected: u64,
        authority: &approval_transaction::EntryAuthority,
    ) -> Result<AcceptedMission, MissionError> {
        let bundle = self.progress_bundle(mission_id, waypoint_id, state, expected)?;
        self.store.import_locked(
            mission_id,
            bundle,
            expected,
            &typed_source::ImportOptions {
                authority: Some(authority),
                classify_nonsemantic: true,
            },
        )
    }

    fn progress_bundle(
        &self,
        mission_id: Uuid,
        waypoint_id: &str,
        state: WaypointState,
        expected: u64,
    ) -> Result<MissionBundle, MissionError> {
        let mut bundle = self.next_bundle(mission_id, expected)?;
        let waypoint = Self::local_waypoint(&mut bundle, waypoint_id)?;
        if state == WaypointState::Completed {
            return Err(MissionError::Validation(
                1,
                vec![Diagnostic {
                    code: "completion_evidence_required",
                    source: "lifecycle".into(),
                    pointer: String::new(),
                    waypoint_id: Some(waypoint_id.into()),
                    message: "Use the validated lifecycle completion operation, not raw progress."
                        .into(),
                }],
            ));
        }
        waypoint.state = Some(state);
        Ok(bundle)
    }

    pub fn record_result(
        &self,
        mission_id: Uuid,
        result: OperatorResult,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        let mut bundle = self.next_bundle(mission_id, expected)?;
        bundle.manifest.operator_results.push(result);
        self.store.import_with(
            mission_id,
            bundle,
            expected,
            &typed_source::ImportOptions {
                authority: None,
                classify_nonsemantic: true,
            },
        )
    }

    pub fn validate(&self, mission_id: Uuid) -> Result<AcceptedMission, MissionError> {
        let accepted = self.store.get(mission_id)?;
        validate_bundle(&accepted.bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        waypoint_relationships::resolve_bundle(
            self.store
                .root
                .ancestors()
                .nth(3)
                .unwrap_or(&self.store.root),
            &accepted.bundle,
        )?;
        Ok(accepted)
    }

    pub fn get(&self, mission_id: Uuid) -> Result<AcceptedMission, MissionError> {
        self.store.get(mission_id)
    }

    pub fn publish(
        &self,
        mission_id: Uuid,
        expected: u64,
        states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<AcceptedMission, MissionError> {
        let bundle = self.next_bundle(mission_id, expected)?;
        self.store
            .publish_with_status_snapshot(mission_id, bundle, expected, states, snapshot_id)
    }

    pub(super) fn fragment(input: DirectWaypoint) -> WaypointFragment {
        WaypointFragment {
            outcome_plan: None,
            session_package: input.waypoint_id.clone(),
            waypoint_id: input.waypoint_id,
            title: input.title,
            depends_on: input.depends_on,
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            relationships: WaypointRelationships::default(),
            child_mission_ref: None,
            part_markdown: input.prompt.clone(),
            prompt: input.prompt,
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: input.non_goals,
            validation: input.validation,
            commit_checkpoint: input.commit_checkpoint,
        }
    }

    fn update(
        &self,
        mission_id: Uuid,
        expected: u64,
        update: impl FnOnce(&mut MissionBundle) -> Result<(), MissionError>,
    ) -> Result<AcceptedMission, MissionError> {
        let mut bundle = self.next_bundle(mission_id, expected)?;
        update(&mut bundle)?;
        self.store.import(mission_id, bundle, expected)
    }

    fn next_bundle(&self, mission_id: Uuid, expected: u64) -> Result<MissionBundle, MissionError> {
        let mut bundle = self.store.get(mission_id)?.bundle;
        if bundle.manifest.revision != expected {
            return Err(MissionError::StaleRevision {
                expected,
                actual: bundle.manifest.revision,
            });
        }
        bundle.manifest.revision = expected.checked_add(1).ok_or_else(|| {
            MissionError::Io(std::io::Error::other(
                "Mission revision exhausted; cannot allocate another revision.",
            ))
        })?;
        Ok(bundle)
    }

    fn local_waypoint<'a>(
        bundle: &'a mut MissionBundle,
        waypoint_id: &str,
    ) -> Result<&'a mut WaypointFragment, MissionError> {
        let waypoint = bundle
            .waypoints
            .iter_mut()
            .find(|item| item.waypoint_id == waypoint_id)
            .ok_or_else(|| {
                MissionError::Validation(
                    1,
                    vec![Diagnostic {
                        code: "unknown-waypoint",
                        source: "accepted.json".into(),
                        pointer: "/waypoints".into(),
                        waypoint_id: Some(waypoint_id.into()),
                        message: "Use a waypoint ID returned by get.".into(),
                    }],
                )
            })?;
        if waypoint.ticket_ref.is_some()
            || waypoint.child_mission_ref.is_some()
            || matches!(waypoint.scope, Scope::TicketBacked { .. })
        {
            return Err(MissionError::Validation(1, vec![Diagnostic {
                code: "externally-owned-waypoint",
                source: "accepted.json".into(),
                pointer: "/waypoints".into(),
                waypoint_id: Some(waypoint_id.into()),
                message: "Change the authoritative Ticket or child Mission, not local waypoint content or state.".into(),
            }]));
        }
        Ok(waypoint)
    }
}
