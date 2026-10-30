use super::*;

impl MissionStore {
    pub(crate) fn prepare_write(&self) -> Result<(), MissionError> {
        ensure_store_chain(&self.root)?;
        fs::create_dir_all(&self.root)?;
        ensure_store_chain(&self.root)?;
        ensure_safe_path(&self.root, &self.root)?;
        Ok(())
    }

    pub(crate) fn source_for_legacy(
        &self,
        next: &AcceptedMission,
    ) -> Result<Option<TypedDraft>, MissionError> {
        let directory = self.root.join(next.mission_id.to_string());
        if !directory.join("source.json").exists() {
            return Ok(None);
        }
        let lifecycle = MissionLifecycle {
            store: Self {
                root: self.root.clone(),
                #[cfg(test)]
                test_event_metadata: None,
            },
        };
        let previous = lifecycle.get_mission_source(next.mission_id)?;
        let old = self.get(next.mission_id)?;
        let mut source =
            typed_migration::convert(self.root.ancestors().nth(3).unwrap_or(&self.root), next)?;
        for waypoint in &mut source.waypoints {
            if old.bundle.waypoints.iter().any(|w| {
                w.waypoint_id == waypoint.waypoint_id
                    && next.bundle.waypoints.iter().any(|n| {
                        n.waypoint_id == w.waypoint_id && n.part_markdown == w.part_markdown
                    })
            }) && let Some(saved) = previous
                .waypoints
                .iter()
                .find(|w| w.waypoint_id == waypoint.waypoint_id)
            {
                waypoint.part = saved.part.clone();
            }
        }
        validate_typed(&source, false)?;
        Ok(Some(source))
    }

    /// Stage all representations under the same lock and replace one directory.
    pub(crate) fn nonsemantic_extras(
        &self,
        old: &AcceptedMission,
        next: &AcceptedMission,
        next_source: &TypedDraft,
    ) -> Result<Vec<(String, Vec<u8>)>, MissionError> {
        let lifecycle = MissionLifecycle {
            store: Self {
                root: self.root.clone(),
                #[cfg(test)]
                test_event_metadata: None,
            },
        };
        let previous = lifecycle.get_mission_source(old.mission_id)?;
        plan_operations::nonsemantic_ledger(
            &self.root.join(old.mission_id.to_string()),
            old,
            &previous,
            next,
            next_source,
        )
    }

    pub(crate) fn write_authority(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
    ) -> Result<(), MissionError> {
        self.write_authority_inner(accepted, source, rendered, false, None, &[], None)
    }

    pub(crate) fn write_authority_with_extras(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        extras: &[(String, Vec<u8>)],
    ) -> Result<(), MissionError> {
        self.write_authority_inner(accepted, source, rendered, false, None, extras, None)
    }

    pub(crate) fn write_authority_entering(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        extras: &[(String, Vec<u8>)],
        authority: &approval_transaction::EntryAuthority,
    ) -> Result<(), MissionError> {
        self.write_authority_inner(
            accepted,
            source,
            rendered,
            false,
            None,
            extras,
            Some(authority),
        )
    }

    pub(crate) fn write_authority_with_inputs(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        inputs: Option<&MissionOutputInputs>,
    ) -> Result<(), MissionError> {
        self.write_authority_inner(accepted, source, rendered, false, inputs, &[], None)
    }

    pub(crate) fn write_authority_for_repair(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        replace_source: bool,
    ) -> Result<(), MissionError> {
        self.write_authority_inner(accepted, source, rendered, replace_source, None, &[], None)
    }

    fn write_authority_inner(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        replace_source: bool,
        inputs: Option<&MissionOutputInputs>,
        extras: &[(String, Vec<u8>)],
        authority: Option<&approval_transaction::EntryAuthority>,
    ) -> Result<(), MissionError> {
        let id = accepted.mission_id;
        self.validate_requirement_references(&accepted.bundle)?;
        waypoint_relationships::resolve_bundle(
            self.root.ancestors().nth(3).unwrap_or(&self.root),
            &accepted.bundle,
        )?;
        let _ticket_links = TicketLinkLock::acquire_all(
            &self.root,
            &waypoint_relationships::linked_ticket_ids(
                self.root.ancestors().nth(3).unwrap_or(&self.root),
                &accepted.bundle,
            )?,
        )?;
        self.validate_artifact_references(id, &accepted.bundle)?;
        let provenance = MissionArtifactProvenance::for_write(accepted, source, rendered);
        provenance
            .validate()
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let directory = self.root.join(id.to_string());
        if authority.is_none() {
            let existing = if present(&directory.join("accepted.json"))? {
                Some(self.get(id)?)
            } else {
                None
            };
            if let Some(waypoint_id) = entering_in_progress(existing.as_ref(), &accepted.bundle) {
                return Err(typed_error(
                    "approval_required",
                    &format!(
                        "waypoint `{waypoint_id}` requires the requester-approval entry gate."
                    ),
                ));
            }
        }
        if directory.join("history/events").exists() {
            self.validate_preserved_events(id)?;
        }
        let stage = self.root.join(format!(".{id}.typed.tmp"));
        let backup = self.root.join(format!(".{id}.typed.backup"));
        let recovery = |message: &str| MissionError::PublicationRecovery {
            mission_id: id,
            message: message.into(),
        };
        if present(&backup)? {
            return Err(recovery(
                "prior staging or backup exists; inspect it before retrying",
            ));
        }
        let creating = source.is_some() && !directory.exists();
        if present(&stage)? && !creating {
            return Err(recovery(
                "prior staging or backup exists; inspect it before retrying",
            ));
        }
        let build = |stage: &Path| -> Result<(), MissionError> {
            if directory.exists() {
                super::super::mission_status::copy_directory(&directory, &stage)?;
            } else {
                fs::create_dir(&stage)?;
            }
            fs::create_dir_all(stage.join("history"))?;
            let bytes = serde_json::to_vec_pretty(accepted)?;
            fs::write(stage.join("accepted.json"), &bytes)?;
            write_new(
                &stage.join("history").join(format!(
                    "revision-{}.json",
                    accepted.bundle.manifest.revision
                )),
                &bytes,
            )?;
            if let Some(source) = source {
                if stage.join("parts").exists() {
                    fs::remove_dir_all(stage.join("parts"))?;
                }
                fs::create_dir_all(stage.join("parts"))?;
                let bytes = serde_json::to_vec_pretty(&stored_source(source))?;
                fs::write(stage.join("source.json"), &bytes)?;
                write_new(
                    &stage
                        .join("history")
                        .join(format!("source-revision-{}.json", source.revision)),
                    &bytes,
                )?;
                for waypoint in &source.waypoints {
                    let path = stage
                        .join("parts")
                        .join(format!("{}.json", waypoint.waypoint_id));
                    fs::write(path, serde_json::to_vec_pretty(&waypoint.part)?)?;
                }
            } else if replace_source {
                if stage.join("source.json").exists() {
                    fs::remove_file(stage.join("source.json"))?;
                }
                if stage.join("parts").exists() {
                    fs::remove_dir_all(stage.join("parts"))?;
                }
            }
            if stage.join("mission.md").exists() {
                fs::remove_file(stage.join("mission.md"))?;
            }
            if stage.join("generated").exists() {
                fs::remove_dir_all(stage.join("generated"))?;
            }
            if let Some(rendered) = rendered {
                fs::create_dir(stage.join("generated"))?;
                for (name, bytes) in rendered.files() {
                    let path = stage.join("generated").join(name);
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(path, bytes)?;
                }
                if let Some(source) = source {
                    let lifecycle = MissionLifecycle {
                        store: Self {
                            root: self.root.clone(),
                            #[cfg(test)]
                            test_event_metadata: None,
                        },
                    };
                    fs::write(stage.join("mission.md"), lifecycle.render_typed(source))?;
                }
            }
            for (name, bytes) in extras {
                let path = stage.join(name);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(path, bytes)?;
            }
            // Mirror projected_state for every writer, including repair.
            let projection = stage.join(plan_operations::REVIEW_PLAN_FILE);
            match (source, rendered) {
                (Some(source), Some(_)) => {
                    let ledger = plan_operations::read_ledger(&stage)?;
                    let plan = plan_operations::build_review_plan(accepted, source, &ledger)?;
                    fs::write(&projection, serde_json::to_vec_pretty(&plan)?)?;
                }
                _ => {
                    if present(&projection)? {
                        fs::remove_file(&projection)?;
                    }
                }
            }
            write_new(
                &stage.join("history").join(format!(
                    "provenance-revision-{}.json",
                    accepted.bundle.manifest.revision
                )),
                &serde_json::to_vec_pretty(&provenance)?,
            )?;
            super::artifact_contract::validate_file_outputs(&provenance, &stage, false)?;
            fs::write(
                stage.join("artifact-provenance.json"),
                serde_json::to_vec_pretty(&provenance)?,
            )?;
            if let Some(inputs) = inputs {
                super::output_history::write_inputs(&stage, inputs)?;
            }
            super::output_history::capture(&stage, accepted)?;
            Ok::<_, MissionError>(())
        };
        let mut adopt_stage = false;
        let mut create_intent: Option<PathBuf> = None;
        if creating && let Some(source) = source {
            let expected = CreateIntent {
                schema_version: CREATE_INTENT_SCHEMA_VERSION,
                mission_id: id,
                revision: accepted.bundle.manifest.revision,
                request_sha256: create_request_digest(accepted, source)?,
            };
            let intent_path = self.root.join(format!(".{id}.typed.intent"));
            let state = read_create_intent(&intent_path, &expected)?;
            if state == IntentState::Conflicts {
                return Err(recovery(
                    "a durable create intent binds this Mission id to a different or unreadable request; left untouched for inspection",
                ));
            }
            if present(&stage)? {
                let probe = self
                    .root
                    .join(format!(".{id}.typed.verify-{}", Uuid::new_v4()));
                let verdict = build(&probe).and_then(|()| compare_stage(&stage, &probe));
                if probe.exists()
                    && let Err(error) = fs::remove_dir_all(&probe)
                {
                    return Err(recovery(&format!(
                        "could not remove verification scratch {}: {error}",
                        probe.display()
                    )));
                }
                match verdict? {
                    StageMatch::Same => adopt_stage = true,
                    StageMatch::OwnPartial if state == IntentState::Matches => {
                        fs::remove_dir_all(&stage)?;
                    }
                    _ => {
                        return Err(recovery(
                            "existing staging directory is not provably this create request's; left untouched for inspection",
                        ));
                    }
                }
            }
            if !adopt_stage && state == IntentState::Absent {
                write_create_intent(&self.root, &intent_path, &expected)?;
                #[cfg(test)]
                if CRASH_AFTER_INTENT.with(|flag| flag.replace(false)) {
                    return Err(MissionError::Io(std::io::Error::other(
                        "injected crash after create intent",
                    )));
                }
            }
            create_intent = Some(intent_path);
        }
        let result = if adopt_stage { Ok(()) } else { build(&stage) };
        if let Err(error) = result {
            if stage.exists() {
                fs::remove_dir_all(&stage)?;
            }
            forget_create_intent(create_intent.as_deref());
            return Err(error);
        }
        if let Err(error) = sync_tree(&stage) {
            if stage.exists() {
                fs::remove_dir_all(&stage)?;
            }
            forget_create_intent(create_intent.as_deref());
            return Err(error);
        }
        #[cfg(test)]
        if CRASH_BEFORE_PROMOTE.with(|flag| flag.replace(false)) {
            return Err(MissionError::Io(std::io::Error::other(
                "injected crash before promotion",
            )));
        }
        if directory.exists() {
            fs::rename(&directory, &backup)?;
            if let Err(error) = fs::File::open(&self.root).and_then(|root| root.sync_all()) {
                if let Err(restore) = fs::rename(&backup, &directory) {
                    return Err(MissionError::PublicationRecovery {
                        mission_id: id,
                        message: format!(
                            "could not sync backup rename ({error}); restore failed ({restore})"
                        ),
                    });
                }
                return Err(error.into());
            }
        }
        if let Err(error) = promote_authority(&stage, &directory) {
            if backup.exists()
                && let Err(restore) = fs::rename(&backup, &directory)
            {
                return Err(MissionError::PublicationRecovery {
                    mission_id: id,
                    message: format!("replacement failed ({error}); restore failed ({restore})"),
                });
            }
            fs::remove_dir_all(&stage)?;
            forget_create_intent(create_intent.as_deref());
            return Err(error.into());
        }
        if let Err(error) = fs::File::open(&self.root).and_then(|root| root.sync_all()) {
            return Err(MissionError::PublicationRecovery {
                mission_id: id,
                message: format!("authority promoted but directory sync failed: {error}"),
            });
        }
        if backup.exists()
            && let Err(error) = remove_typed_backup(&backup)
        {
            eprintln!(
                "mission-api: accepted revision {} but retained backup {}: {error}",
                accepted.bundle.manifest.revision,
                backup.display()
            );
        }
        forget_create_intent(create_intent.as_deref());
        Ok(())
    }
}
