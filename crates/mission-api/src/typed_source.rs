use super::*;
use schemars::JsonSchema;

/// Legacy authoring version; v3 requires explicit authored outcome plans.
pub const TYPED_SOURCE_SCHEMA_VERSION: u32 = 2;
pub const PART_SOURCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedDraft {
    #[schemars(with = "String")]
    pub mission_id: Uuid,
    pub schema_version: u32,
    pub revision: u64,
    pub title: String,
    pub objective: String,
    #[serde(default)]
    pub waypoints: Vec<TypedWaypoint>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedWaypoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_plan: Option<WaypointOutcomePlan>,
    pub waypoint_id: String,
    pub title: String,
    pub prompt: String,
    pub part: PartSource,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub validation: Vec<ValidationGate>,
    #[serde(default)]
    pub commit_checkpoint: CommitCheckpoint,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub ownership_refs: OwnershipRefs,
    #[serde(default, skip_serializing_if = "WaypointRelationships::is_empty")]
    pub relationships: WaypointRelationships,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OwnershipRefs {
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub ticket_id: Option<Uuid>,
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub child_mission_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_mission_uri: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PartSource {
    pub waypoint_id: String,
    pub document: Document,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredSource {
    pub(super) mission_id: Uuid,
    pub(super) schema_version: u32,
    pub(super) revision: u64,
    pub(super) title: String,
    pub(super) objective: String,
    pub(super) waypoints: Vec<StoredWaypoint>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoredWaypoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) outcome_plan: Option<WaypointOutcomePlan>,
    pub(super) waypoint_id: String,
    pub(super) title: String,
    pub(super) prompt: String,
    pub(super) part_file: String,
    #[serde(default)]
    pub(super) depends_on: Vec<String>,
    #[serde(default)]
    pub(super) validation: Vec<ValidationGate>,
    #[serde(default)]
    pub(super) commit_checkpoint: CommitCheckpoint,
    #[serde(default)]
    pub(super) non_goals: Vec<String>,
    #[serde(default)]
    pub(super) ownership_refs: OwnershipRefs,
    #[serde(default, skip_serializing_if = "WaypointRelationships::is_empty")]
    pub(super) relationships: WaypointRelationships,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Document {
    #[serde(default)]
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Block {
    Heading {
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    BulletList {
        items: Vec<Vec<Block>>,
    },
    OrderedList {
        items: Vec<Vec<Block>>,
    },
    Blockquote {
        blocks: Vec<Block>,
    },
    CodeFence {
        language: String,
        text: String,
    },
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    ThematicBreak,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Inline {
    Text { text: String },
    Emphasis { inlines: Vec<Inline> },
    Strong { inlines: Vec<Inline> },
    Code { text: String },
    Link { href: String, inlines: Vec<Inline> },
    HardBreak,
}

impl Default for CommitCheckpoint {
    fn default() -> Self {
        Self {
            message: String::new(),
        }
    }
}

impl MissionLifecycle {
    pub fn create_draft(
        &self,
        title: String,
        objective: String,
    ) -> Result<TypedDraft, MissionError> {
        let draft = TypedDraft {
            mission_id: Uuid::new_v4(),
            schema_version: TYPED_SOURCE_SCHEMA_VERSION,
            revision: 1,
            title,
            objective,
            waypoints: vec![],
        };
        self.save_typed(&draft, 0, false)?;
        Ok(draft)
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

    pub(super) fn get_mission_source_unchecked(
        &self,
        mission_id: Uuid,
    ) -> Result<TypedDraft, MissionError> {
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
        let next = accepted_from_typed(draft, accepted.as_ref())?;
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
        self.store
            .write_authority(&next, Some(draft), rendered.as_ref())
    }

    pub(super) fn render_typed(&self, draft: &TypedDraft) -> String {
        let mut output = format!("# {}\n\n{}\n", draft.title, draft.objective);
        for waypoint in &draft.waypoints {
            output.push_str(&format!("\n## {}\n\n", waypoint.title));
            render_blocks(&mut output, &waypoint.part.document.blocks, 0);
            outcome_plan::render_outcome_plan(&mut output, waypoint.outcome_plan.as_ref());
        }
        output
    }
}

pub(super) fn accepted_from_typed(
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

impl MissionStore {
    pub(super) fn prepare_write(&self) -> Result<(), MissionError> {
        fs::create_dir_all(&self.root)?;
        ensure_safe_path(&self.root, &self.root)?;
        Ok(())
    }

    pub(super) fn source_for_legacy(
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
    pub(super) fn write_authority(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
    ) -> Result<(), MissionError> {
        self.write_authority_inner(accepted, source, rendered, false, None)
    }

    pub(super) fn write_authority_with_inputs(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        inputs: Option<&MissionOutputInputs>,
    ) -> Result<(), MissionError> {
        self.write_authority_inner(accepted, source, rendered, false, inputs)
    }

    pub(super) fn write_authority_for_repair(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        replace_source: bool,
    ) -> Result<(), MissionError> {
        self.write_authority_inner(accepted, source, rendered, replace_source, None)
    }

    fn write_authority_inner(
        &self,
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
        replace_source: bool,
        inputs: Option<&MissionOutputInputs>,
    ) -> Result<(), MissionError> {
        let id = accepted.mission_id;
        self.validate_requirement_references(&accepted.bundle)?;
        waypoint_relationships::resolve_bundle(
            self.root.ancestors().nth(3).unwrap_or(&self.root),
            &accepted.bundle,
        )?;
        self.validate_artifact_references(id, &accepted.bundle)?;
        let provenance = MissionArtifactProvenance::for_write(accepted, source, rendered);
        provenance
            .validate()
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let directory = self.root.join(id.to_string());
        if directory.join("history/events").exists() {
            self.validate_preserved_events(id)?;
        }
        let stage = self.root.join(format!(".{id}.typed.tmp"));
        let backup = self.root.join(format!(".{id}.typed.backup"));
        if stage.exists() || backup.exists() {
            return Err(MissionError::PublicationRecovery {
                mission_id: id,
                message: "prior staging or backup exists; inspect it before retrying".into(),
            });
        }
        let result = (|| {
            if directory.exists() {
                copy_directory(&directory, &stage)?;
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
        })();
        if let Err(error) = result {
            if stage.exists() {
                fs::remove_dir_all(&stage)?;
            }
            return Err(error);
        }
        if let Err(error) = sync_tree(&stage) {
            if stage.exists() {
                fs::remove_dir_all(&stage)?;
            }
            return Err(error);
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
        Ok(())
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), MissionError> {
    use std::io::Write;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}

fn sync_tree(path: &Path) -> Result<(), MissionError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    }
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

fn promote_authority(stage: &Path, directory: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if PROMOTION_FAILURE.with(|failure| failure.get()) {
        return Err(std::io::Error::other(
            "injected authority promotion failure",
        ));
    }
    fs::rename(stage, directory)
}

#[cfg(test)]
thread_local! {
    static PROMOTION_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TYPED_BACKUP_CLEANUP_FAILURE: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

fn remove_typed_backup(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if TYPED_BACKUP_CLEANUP_FAILURE.with(std::cell::Cell::get) {
        return Err(std::io::Error::other(
            "injected typed backup cleanup failure",
        ));
    }
    fs::remove_dir_all(path)
}

pub(super) struct TypedLock {
    path: PathBuf,
}

impl TypedLock {
    pub(super) fn acquire(root: &Path, mission_id: Uuid) -> Result<Self, MissionError> {
        let path = root.join(format!(".{mission_id}.typed.lock"));
        loop {
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    std::thread::yield_now();
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

impl Drop for TypedLock {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir(&self.path) {
            eprintln!(
                "mission-api: retained typed write lock {} after completed write: {error}",
                self.path.display()
            );
        }
    }
}

pub(super) fn ensure_safe_path(root: &Path, path: &Path) -> Result<(), MissionError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        typed_error(
            "unsafe-part-path",
            "managed paths must remain below the Mission root.",
        )
    })?;
    let mut current = root.to_path_buf();
    let root_metadata = fs::symlink_metadata(&current).map_err(MissionError::Io)?;
    if root_metadata.file_type().is_symlink() {
        return Err(typed_error(
            "unsafe-part-path",
            "managed source paths and their ancestors must not be symlinks.",
        ));
    }
    for component in relative.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(MissionError::Io)?;
        if metadata.file_type().is_symlink() {
            return Err(typed_error(
                "unsafe-part-path",
                "managed source paths and their ancestors must not be symlinks.",
            ));
        }
    }
    Ok(())
}

pub(super) fn stored_source(draft: &TypedDraft) -> StoredSource {
    StoredSource {
        mission_id: draft.mission_id,
        schema_version: draft.schema_version,
        revision: draft.revision,
        title: draft.title.clone(),
        objective: draft.objective.clone(),
        waypoints: draft
            .waypoints
            .iter()
            .map(|waypoint| StoredWaypoint {
                outcome_plan: waypoint.outcome_plan.clone(),
                waypoint_id: waypoint.waypoint_id.clone(),
                title: waypoint.title.clone(),
                prompt: waypoint.prompt.clone(),
                part_file: format!("parts/{}.json", waypoint.waypoint_id),
                depends_on: waypoint.depends_on.clone(),
                validation: waypoint.validation.clone(),
                commit_checkpoint: waypoint.commit_checkpoint.clone(),
                non_goals: waypoint.non_goals.clone(),
                ownership_refs: waypoint.ownership_refs.clone(),
                relationships: waypoint.relationships.clone(),
            })
            .collect(),
    }
}

fn typed_error(code: &'static str, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "source.json".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.into(),
        }],
    )
}

pub(super) fn validate_typed(draft: &TypedDraft, publishing: bool) -> Result<(), MissionError> {
    if !matches!(
        draft.schema_version,
        TYPED_SOURCE_SCHEMA_VERSION | OUTCOME_SOURCE_SCHEMA_VERSION
    ) {
        return Err(typed_error(
            "invalid-schema-version",
            &format!(
                "source schema_version must be {} (legacy) or {} (planned outcomes).",
                TYPED_SOURCE_SCHEMA_VERSION, OUTCOME_SOURCE_SCHEMA_VERSION
            ),
        ));
    }
    let mut ids = BTreeSet::new();
    if publishing && draft.waypoints.is_empty() {
        return Err(typed_error(
            "draft-not-ready",
            "publishing requires an ordered waypoint set.",
        ));
    }
    for waypoint in &draft.waypoints {
        outcome_plan::validate_outcome_plan(
            waypoint.outcome_plan.as_ref(),
            draft.schema_version == OUTCOME_SOURCE_SCHEMA_VERSION,
            &waypoint.waypoint_id,
        )
        .map_err(one_diagnostic)?;
        waypoint.relationships.validate().map_err(one_diagnostic)?;
        let refs = &waypoint.ownership_refs;
        if let (Some(id), Some(uri)) = (refs.ticket_id, &refs.ticket_uri)
            && uri
                .rsplit('/')
                .next()
                .and_then(|value| Uuid::parse_str(value).ok())
                != Some(id)
        {
            return Err(typed_error(
                "mission-waypoint-link-identity-mismatch",
                "ownership_refs.ticket_id and ticket_uri must identify the same canonical Ticket.",
            ));
        }
        if waypoint.waypoint_id.is_empty()
            || waypoint.waypoint_id.contains('/')
            || waypoint.waypoint_id.contains('\\')
            || waypoint.waypoint_id == "."
            || waypoint.waypoint_id == ".."
        {
            return Err(typed_error(
                "invalid-waypoint-id",
                "waypoint IDs must be safe path components.",
            ));
        }
        if !ids.insert(&waypoint.waypoint_id) || waypoint.part.waypoint_id != waypoint.waypoint_id {
            return Err(typed_error(
                "invalid-part-reference",
                "each waypoint needs one matching unique managed Part.",
            ));
        }
        if waypoint
            .non_goals
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(typed_error(
                "invalid-non-goal",
                "non_goals entries must be nonblank.",
            ));
        }
        validate_blocks(&waypoint.part.document.blocks)?;
        if publishing
            && (waypoint.validation.is_empty()
                || waypoint
                    .validation
                    .iter()
                    .any(|gate| gate.command.trim().is_empty())
                || waypoint.commit_checkpoint.message.trim().is_empty())
        {
            return Err(typed_error(
                "draft-not-ready",
                "publishing requires a nonblank validation command and checkpoint per waypoint.",
            ));
        }
        if waypoint
            .validation
            .iter()
            .any(|gate| gate.cwd.as_ref().is_some_and(|cwd| cwd.trim().is_empty()))
        {
            return Err(typed_error(
                "invalid-validation-cwd",
                "a supplied execution cwd must be nonblank.",
            ));
        }
    }
    for waypoint in &draft.waypoints {
        let mut dependencies = BTreeSet::new();
        for dependency in &waypoint.depends_on {
            if dependency == &waypoint.waypoint_id
                || !ids.contains(dependency)
                || !dependencies.insert(dependency)
            {
                return Err(typed_error(
                    "invalid-dependency-reference",
                    "dependencies must be unique references to other waypoints.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_blocks(blocks: &[Block]) -> Result<(), MissionError> {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } if *level == 0 || *level > 6 => {
                return Err(typed_error(
                    "invalid-heading-level",
                    "heading levels must be 1 through 6.",
                ));
            }
            Block::Heading { inlines, .. } | Block::Paragraph { inlines } => {
                validate_inlines(inlines)?
            }
            Block::BulletList { items } | Block::OrderedList { items } => {
                for item in items {
                    validate_blocks(item)?;
                }
            }
            Block::Blockquote { blocks } => validate_blocks(blocks)?,
            Block::Table { header, rows } => {
                for row in header {
                    validate_inlines(row)?;
                }
                for row in rows {
                    for cell in row {
                        validate_inlines(cell)?;
                    }
                }
            }
            Block::CodeFence { .. } | Block::ThematicBreak => {}
        }
    }
    Ok(())
}
fn validate_inlines(inlines: &[Inline]) -> Result<(), MissionError> {
    for inline in inlines {
        match inline {
            Inline::Emphasis { inlines }
            | Inline::Strong { inlines }
            | Inline::Link { inlines, .. } => validate_inlines(inlines)?,
            _ => {}
        }
    }
    Ok(())
}
fn render_blocks(out: &mut String, blocks: &[Block], depth: usize) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                out.push_str(&"#".repeat(*level as usize));
                out.push(' ');
                render_inlines(out, inlines);
                out.push_str("\n\n");
            }
            Block::Paragraph { inlines } => {
                render_inlines(out, inlines);
                out.push_str("\n\n");
            }
            Block::BulletList { items } | Block::OrderedList { items } => {
                for (i, item) in items.iter().enumerate() {
                    out.push_str(&"  ".repeat(depth));
                    if matches!(block, Block::OrderedList { .. }) {
                        out.push_str(&format!("{}. ", i + 1));
                    } else {
                        out.push_str("- ");
                    }
                    render_blocks(out, item, depth + 1);
                }
            }
            Block::Blockquote { blocks } => {
                let mut nested = String::new();
                render_blocks(&mut nested, blocks, depth);
                for line in nested.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            Block::CodeFence { language, text } => {
                out.push_str("```");
                out.push_str(language);
                out.push('\n');
                out.push_str(text);
                out.push_str("\n```\n\n");
            }
            Block::Table { header, rows } => {
                for row in std::iter::once(header).chain(rows.iter()) {
                    out.push('|');
                    for cell in row {
                        render_inlines(out, cell);
                        out.push('|');
                    }
                    out.push('\n');
                }
            }
            Block::ThematicBreak => out.push_str("---\n\n"),
        }
    }
}
fn render_inlines(out: &mut String, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text { text } => out.push_str(text),
            Inline::Code { text } => {
                out.push('`');
                out.push_str(text);
                out.push('`');
            }
            Inline::HardBreak => out.push_str("  \n"),
            Inline::Emphasis { inlines } => {
                out.push('*');
                render_inlines(out, inlines);
                out.push('*');
            }
            Inline::Strong { inlines } => {
                out.push_str("**");
                render_inlines(out, inlines);
                out.push_str("**");
            }
            Inline::Link { href, inlines } => {
                out.push('[');
                render_inlines(out, inlines);
                out.push_str("](");
                out.push_str(href);
                out.push(')');
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn accepted_write_survives_recoverable_backup_cleanup_failure() {
        let directory = tempdir().unwrap();
        let lifecycle = MissionLifecycle::open(directory.path());
        let created = lifecycle
            .create_draft("initial".into(), "objective".into())
            .unwrap();
        let mut replacement = created.clone();
        replacement.title = "accepted replacement".into();

        TYPED_BACKUP_CLEANUP_FAILURE.with(|failure| failure.set(true));
        let accepted = lifecycle
            .save_draft(created.mission_id, replacement, 1)
            .unwrap();
        TYPED_BACKUP_CLEANUP_FAILURE.with(|failure| failure.set(false));

        assert_eq!(accepted.revision, 2);
        assert_eq!(
            lifecycle.get_mission_source(created.mission_id).unwrap(),
            accepted
        );
        assert!(
            directory
                .path()
                .join(".workflow-tools/mission/missions")
                .join(format!(".{}.typed.backup", created.mission_id))
                .is_dir(),
            "the old revision remains recoverable after cleanup warning"
        );
    }
}
