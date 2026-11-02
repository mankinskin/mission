use super::*;

impl MissionStore {
    pub(super) fn render_projection(
        &self,
        bundle: &MissionBundle,
        mission_id: Uuid,
        snapshot_id: Option<&str>,
    ) -> Result<RenderedRoadmap, MissionError> {
        let generated_directory = Path::new(".workflow-tools")
            .join("mission")
            .join("missions")
            .join(mission_id.to_string())
            .join("generated");
        let mut rendered = render_with_consumer(
            bundle,
            mission_id,
            snapshot_id,
            self.root.ancestors().nth(3),
            Some(&generated_directory),
        )
        .map_err(|errors| MissionError::Validation(errors.len(), errors))?;
        rendered.parts.insert(
            "HISTORY.md".into(),
            output_history::history(
                Some(&self.root.join(mission_id.to_string())),
                bundle.manifest.revision,
            )?,
        );
        Ok(rendered)
    }

    fn at_workspace(workspace_root: &Path) -> Self {
        let root = workspace_root
            .join(".workflow-tools")
            .join("mission")
            .join("missions");
        Self {
            root,
            #[cfg(test)]
            test_event_metadata: None,
        }
    }

    #[cfg(test)]
    // The test-only integration harness uses these helpers; the unit-test
    // target compiles this module without that integration target.
    #[allow(dead_code)]
    pub(crate) fn with_test_event_metadata(
        mut self,
        event_id: Uuid,
        recorded_at_unix_ms: u64,
    ) -> Self {
        self.test_event_metadata = Some((event_id, recorded_at_unix_ms));
        self
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn check_generated_output(&self, mission_id: Uuid) -> Result<(), MissionError> {
        let accepted = self.get(mission_id)?;
        let rendered = self.render_projection(&accepted.bundle, mission_id, None)?;
        let generated = self.root.join(mission_id.to_string()).join("generated");
        let diagnostics = rendered.check_generated(&generated)?;
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(MissionError::Validation(diagnostics.len(), diagnostics))
        }
    }

    pub fn init(workspace_root: &Path) -> Result<Self, MissionError> {
        let store = Self::at_workspace(workspace_root);
        fs::create_dir_all(&store.root)?;
        Ok(store)
    }

    /// Open a store without creating directories or changing filesystem state.
    pub fn open(workspace_root: &Path) -> Self {
        Self::at_workspace(workspace_root)
    }

    pub fn import(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.import_with(
            mission_id,
            bundle,
            expected_current_revision,
            &typed_source::ImportOptions::default(),
        )
    }

    pub(crate) fn import_with(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        options: &typed_source::ImportOptions<'_>,
    ) -> Result<AcceptedMission, MissionError> {
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.validate_child_missions(mission_id, &bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.root, mission_id)?;
        self.import_locked(mission_id, bundle, expected_current_revision, options)
    }

    pub(crate) fn import_locked(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        options: &typed_source::ImportOptions<'_>,
    ) -> Result<AcceptedMission, MissionError> {
        crate::lifecycle::ensure_control_settled(&self.root.join(mission_id.to_string()))?;
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.validate_child_missions(mission_id, &bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let current = match self.get(mission_id) {
            Ok(current) => Some(current),
            Err(MissionError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let actual = current
            .as_ref()
            .map_or(0, |mission| mission.bundle.manifest.revision);
        if expected_current_revision != actual {
            return Err(MissionError::StaleRevision {
                expected: expected_current_revision,
                actual,
            });
        }
        if let Some(existing) = &current
            && bundle.manifest.revision <= existing.bundle.manifest.revision
        {
            return Err(MissionError::StaleRevision {
                expected: existing.bundle.manifest.revision + 1,
                actual: bundle.manifest.revision,
            });
        }
        let labels = bundle
            .manifest
            .execution_order
            .iter()
            .enumerate()
            .map(|(index, id)| (format!("W{}", index + 1), id.clone()))
            .collect();
        let accepted = AcceptedMission {
            mission_id,
            label_mapping: LabelMapping {
                revision: bundle.manifest.revision,
                labels,
            },
            bundle,
        };
        let source = self.source_for_legacy(&accepted)?;
        let extras = match (
            options.classify_nonsemantic,
            source.as_ref(),
            current.as_ref(),
        ) {
            (true, Some(next_source), Some(old)) => {
                self.nonsemantic_extras(old, &accepted, next_source)?
            }
            _ => Vec::new(),
        };
        match options.authority {
            Some(authority) => {
                self.write_authority_entering(&accepted, source.as_ref(), None, &extras, authority)?
            }
            None => self.write_authority_with_extras(&accepted, source.as_ref(), None, &extras)?,
        }
        self.get(mission_id)
    }

    pub fn verify_store_chain(&self) -> Result<(), MissionError> {
        typed_source::ensure_store_chain(&self.root)
    }

    pub fn get(&self, mission_id: Uuid) -> Result<AcceptedMission, MissionError> {
        typed_source::ensure_store_chain(&self.root)?;
        let path = self.root.join(mission_id.to_string()).join("accepted.json");
        if !path.is_file() {
            return Err(MissionError::NotFound(mission_id));
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn revision_mapping(
        &self,
        mission_id: Uuid,
        revision: u64,
    ) -> Result<LabelMapping, MissionError> {
        let path = self
            .root
            .join(mission_id.to_string())
            .join("history")
            .join(format!("revision-{revision}.json"));
        if !path.is_file() {
            return Err(MissionError::NotFound(mission_id));
        }
        Ok(serde_json::from_slice::<AcceptedMission>(&fs::read(path)?)?.label_mapping)
    }

    /// Atomically replaces the accepted record and its generated projection.
    ///
    /// The entire mission directory is staged before its current directory is
    /// moved aside.  A failed replacement restores that directory; failure to
    /// restore is reported explicitly rather than silently selecting either
    /// version.
    pub fn publish(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.publish_inner(mission_id, bundle, expected_current_revision, None, None)
    }

    /// Publish request material and generated output in the same whole-state transaction.
    pub fn publish_with_inputs(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        inputs: &MissionOutputInputs,
        status_snapshot: Option<(&BTreeMap<String, WaypointState>, &str)>,
    ) -> Result<AcceptedMission, MissionError> {
        self.publish_inner(
            mission_id,
            bundle,
            expected_current_revision,
            status_snapshot,
            Some(inputs),
        )
    }

    pub fn publish_with_status_snapshot(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        ticket_states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<AcceptedMission, MissionError> {
        self.publish_inner(
            mission_id,
            bundle,
            expected_current_revision,
            Some((ticket_states, snapshot_id)),
            None,
        )
    }

    fn publish_inner(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        status_snapshot: Option<(&BTreeMap<String, WaypointState>, &str)>,
        inputs: Option<&MissionOutputInputs>,
    ) -> Result<AcceptedMission, MissionError> {
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.validate_child_missions(mission_id, &bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.root, mission_id)?;
        crate::lifecycle::ensure_control_settled(&self.root.join(mission_id.to_string()))?;
        let current = match self.get(mission_id) {
            Ok(current) => Some(current),
            Err(MissionError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let actual = current
            .as_ref()
            .map_or(0, |mission| mission.bundle.manifest.revision);
        if expected_current_revision != actual {
            return Err(MissionError::StaleRevision {
                expected: expected_current_revision,
                actual,
            });
        }
        if let Some(existing) = &current
            && bundle.manifest.revision <= existing.bundle.manifest.revision
        {
            return Err(MissionError::StaleRevision {
                expected: existing.bundle.manifest.revision + 1,
                actual: bundle.manifest.revision,
            });
        }
        let rendered = if let Some((ticket_states, snapshot_id)) = status_snapshot {
            self.render_bundle_with_status_snapshot(
                mission_id,
                &bundle,
                ticket_states,
                snapshot_id,
            )?
        } else {
            self.render_projection(&bundle, mission_id, None)?
        };
        let accepted = AcceptedMission {
            mission_id,
            label_mapping: LabelMapping {
                revision: bundle.manifest.revision,
                labels: bundle
                    .manifest
                    .execution_order
                    .iter()
                    .enumerate()
                    .map(|(index, id)| (format!("W{}", index + 1), id.clone()))
                    .collect(),
            },
            bundle,
        };
        let source = self.source_for_legacy(&accepted)?;
        self.write_authority_with_inputs(&accepted, source.as_ref(), Some(&rendered), inputs)?;
        self.get(mission_id)
    }
}
