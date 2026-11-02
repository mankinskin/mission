use super::*;

impl MissionLifecycle {
    pub fn create_draft(
        &self,
        title: String,
        objective: String,
    ) -> Result<TypedDraft, MissionError> {
        self.create_draft_with_id(Uuid::new_v4(), title, objective)
    }

    /// Create a planning draft with an identity journaled by the caller.
    pub fn create_draft_with_id(
        &self,
        mission_id: Uuid,
        title: String,
        objective: String,
    ) -> Result<TypedDraft, MissionError> {
        if mission_id.is_nil() {
            return Err(typed_error(
                "mission-id-nil",
                "mission_id must not be the nil UUID.",
            ));
        }
        let draft = TypedDraft {
            mission_id,
            schema_version: TYPED_SOURCE_SCHEMA_VERSION,
            revision: 1,
            title,
            objective,
            waypoints: vec![],
        };
        self.save_typed(&draft, 0, false)?;
        Ok(draft)
    }

    /// Managed directory under the explicit owning workspace's Mission store.
    pub fn mission_directory(&self, mission_id: Uuid) -> PathBuf {
        self.store.root.join(mission_id.to_string())
    }

    pub fn save_draft(
        &self,
        mission_id: Uuid,
        mut draft: TypedDraft,
        expected_revision: u64,
    ) -> Result<TypedDraft, MissionError> {
        if draft.mission_id != mission_id {
            return Err(typed_error(
                "mission-id-mismatch",
                "draft mission_id must match the API target.",
            ));
        }
        draft.revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| typed_error("revision-exhausted", "revision cannot be incremented."))?;
        self.save_typed(&draft, expected_revision, false)?;
        Ok(draft)
    }

    pub fn publish_draft(
        &self,
        mission_id: Uuid,
        mut draft: TypedDraft,
        expected_revision: u64,
    ) -> Result<TypedDraft, MissionError> {
        if draft.mission_id != mission_id {
            return Err(typed_error(
                "mission-id-mismatch",
                "draft mission_id must match the API target.",
            ));
        }
        draft.revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| typed_error("revision-exhausted", "revision cannot be incremented."))?;
        validate_typed(&draft, true)?;
        let waypoint_ids: Vec<_> = draft
            .waypoints
            .iter()
            .map(|waypoint| waypoint.waypoint_id.clone())
            .collect();
        for (index, waypoint) in draft.waypoints.iter_mut().enumerate() {
            waypoint.depends_on = if index == 0 {
                vec![]
            } else {
                vec![waypoint_ids[index - 1].clone()]
            };
        }
        self.save_typed(&draft, expected_revision, true)?;
        Ok(draft)
    }

    pub fn get_mission_source(&self, mission_id: Uuid) -> Result<TypedDraft, MissionError> {
        let draft = self.get_mission_source_unchecked(mission_id)?;
        let mission_dir = self.store.root.join(mission_id.to_string());
        if let Ok(accepted) = self.store.get(mission_id) {
            if accepted.bundle.manifest.revision != draft.revision {
                return Err(typed_error(
                    "authority-diverged",
                    "accepted/source revisions differ; use an approved forward repair.",
                ));
            }
        } else if mission_dir.join("accepted.json").exists() {
            self.store.get(mission_id)?;
        }
        Ok(draft)
    }

    pub(crate) fn get_mission_source_unchecked(
        &self,
        mission_id: Uuid,
    ) -> Result<TypedDraft, MissionError> {
        ensure_store_chain(&self.store.root)?;
        let mission_dir = self.store.root.join(mission_id.to_string());
        let path = mission_dir.join("source.json");
        if !mission_dir.exists() || !path.exists() {
            return Err(MissionError::NotFound(mission_id));
        }
        ensure_safe_path(&self.store.root, &mission_dir)?;
        ensure_safe_path(&self.store.root, &path)?;
        let source = serde_json::from_slice::<StoredSource>(&fs::read(path)?).map_err(|_| {
            typed_error(
                "invalid-source-json",
                "source.json must be strict typed JSON.",
            )
        })?;
        let mut waypoints = Vec::with_capacity(source.waypoints.len());
        for waypoint in source.waypoints {
            if waypoint.part_file != format!("parts/{}.json", waypoint.waypoint_id) {
                return Err(typed_error(
                    "unsafe-part-path",
                    "Part paths are API-managed paths below the Mission root.",
                ));
            }
            let part_path = mission_dir.join(&waypoint.part_file);
            ensure_safe_path(&self.store.root, &mission_dir.join("parts"))?;
            ensure_safe_path(&self.store.root, &part_path)?;
            let part = serde_json::from_slice(&fs::read(part_path)?).map_err(|_| {
                typed_error("invalid-part-json", "Part files must be strict typed JSON.")
            })?;
            waypoints.push(TypedWaypoint {
                outcome_plan: waypoint.outcome_plan,
                waypoint_id: waypoint.waypoint_id,
                title: waypoint.title,
                prompt: waypoint.prompt,
                part,
                depends_on: waypoint.depends_on,
                validation: waypoint.validation,
                commit_checkpoint: waypoint.commit_checkpoint,
                non_goals: waypoint.non_goals,
                ownership_refs: waypoint.ownership_refs,
                relationships: waypoint.relationships,
            });
        }
        let draft = TypedDraft {
            mission_id: source.mission_id,
            schema_version: source.schema_version,
            revision: source.revision,
            title: source.title,
            objective: source.objective,
            waypoints,
        };
        validate_typed(&draft, false)?;
        if draft.mission_id != mission_id {
            return Err(typed_error(
                "mission-id-mismatch",
                "stored source identity differs from its directory.",
            ));
        }
        Ok(draft)
    }

    pub fn render_mission(&self, mission_id: Uuid) -> Result<String, MissionError> {
        let source = self.get_mission_source(mission_id)?;
        let accepted = accepted_from_typed(&source, Some(&self.store.get(mission_id)?))?;
        let relationships = waypoint_relationships::resolve_bundle(
            self.store
                .root
                .ancestors()
                .nth(3)
                .unwrap_or(&self.store.root),
            &accepted.bundle,
        )?;
        let mut markdown = format!("# {}\n\n{}\n", source.title, source.objective);
        for waypoint in source.waypoints {
            markdown.push_str(&format!("\n## {}\n\n", waypoint.title));
            render_blocks(&mut markdown, &waypoint.part.document.blocks, 0);
            outcome_plan::render_outcome_plan(&mut markdown, waypoint.outcome_plan.as_ref());
            if let Some(content) = relationships.get(&waypoint.waypoint_id) {
                markdown.push_str(content);
            }
        }
        Ok(markdown)
    }

    pub(crate) fn save_typed(
        &self,
        draft: &TypedDraft,
        expected: u64,
        publish: bool,
    ) -> Result<(), MissionError> {
        validate_typed(draft, publish)?;
        self.store.prepare_write()?;
        let _lock = TypedLock::acquire(&self.store.root, draft.mission_id)?;
        self.save_typed_locked(
            draft,
            expected,
            publish,
            &plan_operations::PlanWrite::default(),
        )
    }

    /// The caller must hold this Mission's non-reentrant `TypedLock`.
    pub(crate) fn save_typed_locked(
        &self,
        draft: &TypedDraft,
        expected: u64,
        publish: bool,
        plan: &plan_operations::PlanWrite<'_>,
    ) -> Result<(), MissionError> {
        crate::lifecycle::ensure_control_settled(&self.mission_directory(draft.mission_id))?;
        validate_typed(draft, publish)?;
        let current = match self.get_mission_source(draft.mission_id) {
            Ok(source) => Some(source),
            Err(MissionError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let accepted = match self.store.get(draft.mission_id) {
            Ok(accepted) => Some(accepted),
            Err(MissionError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let actual = accepted.as_ref().map_or_else(
            || current.as_ref().map_or(0, |source| source.revision),
            |accepted| accepted.bundle.manifest.revision,
        );
        if actual != expected {
            return Err(MissionError::StaleRevision { expected, actual });
        }
        let mut next = accepted_from_typed(draft, accepted.as_ref())?;
        if let Some(adjust) = plan.adjust {
            adjust(&mut next.bundle)?;
        }
        let rendered = if publish {
            validate_bundle(&next.bundle)
                .map_err(|errors| MissionError::Validation(errors.len(), errors))?;
            self.store
                .validate_child_missions(draft.mission_id, &next.bundle)
                .map_err(|errors| MissionError::Validation(errors.len(), errors))?;
            Some(
                self.store
                    .render_projection(&next.bundle, draft.mission_id, None)?,
            )
        } else {
            None
        };
        let extras = match plan.extras {
            Some(build) => build(&next)?,
            None => Vec::new(),
        };
        self.store
            .write_authority_with_extras(&next, Some(draft), rendered.as_ref(), &extras)
    }

    pub(crate) fn render_typed(&self, draft: &TypedDraft) -> String {
        let mut output = format!("# {}\n\n{}\n", draft.title, draft.objective);
        for waypoint in &draft.waypoints {
            output.push_str(&format!("\n## {}\n\n", waypoint.title));
            render_blocks(&mut output, &waypoint.part.document.blocks, 0);
            outcome_plan::render_outcome_plan(&mut output, waypoint.outcome_plan.as_ref());
        }
        output
    }
}

pub(crate) fn accepted_from_typed(
    draft: &TypedDraft,
    previous: Option<&AcceptedMission>,
) -> Result<AcceptedMission, MissionError> {
    let mut bundle = previous
        .map(|value| value.bundle.clone())
        .unwrap_or_else(|| MissionBundle {
            manifest: BundleManifest {
                schema_version: SCHEMA_VERSION,
                revision: draft.revision,
                title: draft.title.clone(),
                objective: draft.objective.clone(),
                waypoint_files: vec![],
                execution_order: vec![],
                artifacts: vec![],
                requirements: vec![],
                validation_gates: vec![],
                notes: vec![],
                operator_results: vec![],
            },
            waypoints: vec![],
        });
    if bundle.waypoints.iter().any(|old| {
        !draft
            .waypoints
            .iter()
            .any(|new| new.waypoint_id == old.waypoint_id)
            && (old.state == Some(WaypointState::Completed)
                || bundle
                    .manifest
                    .operator_results
                    .iter()
                    .any(|result| result.waypoint_id == old.waypoint_id))
    }) {
        return Err(typed_error(
            "retained-waypoint-required",
            "completed waypoints and result identities must be retained.",
        ));
    }
    let mut waypoints = Vec::new();
    for input in &draft.waypoints {
        let mut waypoint = bundle
            .waypoints
            .iter()
            .find(|old| old.waypoint_id == input.waypoint_id)
            .cloned()
            .unwrap_or_else(|| {
                MissionLifecycle::fragment(DirectWaypoint {
                    waypoint_id: input.waypoint_id.clone(),
                    title: input.title.clone(),
                    prompt: input.prompt.clone(),
                    depends_on: input.depends_on.clone(),
                    validation: input.validation.clone(),
                    commit_checkpoint: input.commit_checkpoint.clone(),
                    non_goals: input.non_goals.clone(),
                })
            });
        waypoint.title = input.title.clone();
        waypoint.prompt = input.prompt.clone();
        waypoint.depends_on = input.depends_on.clone();
        waypoint.validation = input.validation.clone();
        waypoint.commit_checkpoint = input.commit_checkpoint.clone();
        waypoint.non_goals = input.non_goals.clone();
        waypoint.relationships = input.relationships.clone();
        waypoint.outcome_plan = input.outcome_plan.clone();
        waypoint.part_markdown.clear();
        render_blocks(&mut waypoint.part_markdown, &input.part.document.blocks, 0);
        let refs = &input.ownership_refs;
        waypoint.ticket_ref = refs
            .ticket_uri
            .clone()
            .or_else(|| refs.ticket_id.map(|id| format!("ce://default/ticket/{id}")));
        waypoint.child_mission_ref = refs.child_mission_uri.clone().or_else(|| {
            refs.child_mission_id
                .map(|id| format!("ce://default/mission/{id}"))
        });
        if let Some(reference) = &waypoint.ticket_ref {
            waypoint.state = None;
            waypoint.scope = Scope::TicketBacked {
                ticket_ref: reference.clone(),
            };
        } else if matches!(waypoint.scope, Scope::TicketBacked { .. }) {
            waypoint.scope = Scope::SingleSession;
        }
        waypoints.push(waypoint);
    }
    bundle.manifest.revision = draft.revision;
    bundle.manifest.schema_version = if draft.schema_version == OUTCOME_SOURCE_SCHEMA_VERSION {
        OUTCOME_BUNDLE_SCHEMA_VERSION
    } else {
        SCHEMA_VERSION
    };
    bundle.manifest.title = draft.title.clone();
    bundle.manifest.objective = draft.objective.clone();
    bundle.manifest.execution_order = draft
        .waypoints
        .iter()
        .map(|w| w.waypoint_id.clone())
        .collect();
    bundle.waypoints = waypoints;
    Ok(AcceptedMission {
        mission_id: draft.mission_id,
        label_mapping: LabelMapping {
            revision: draft.revision,
            labels: bundle
                .manifest
                .execution_order
                .iter()
                .enumerate()
                .map(|(i, id)| (format!("W{}", i + 1), id.clone()))
                .collect(),
        },
        bundle,
    })
}
