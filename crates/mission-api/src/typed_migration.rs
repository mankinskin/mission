//! Recoverable conversion of native v1/v2 records into typed v2/v3 sources.
//!
//! The accepted bundle remains an immutable historical representation.  The
//! typed source is added alongside it, and the normal store revision is
//! advanced once so callers have one CAS value for both representations.

use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationRecord {
    pub workspace: PathBuf,
    pub mission_id: Uuid,
    pub source_revision: u64,
    pub payload_digest: String,
    pub snapshot: PathBuf,
    pub state: MigrationState,
    #[serde(default)]
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationState {
    Planned,
    Applied,
    Verified,
    RolledBack,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationJournal {
    pub id: Uuid,
    pub records: Vec<MigrationRecord>,
}

impl MissionLifecycle {
    /// Plans all records in explicitly selected workspaces.  Planning reads no
    /// mutable store files and rejects a record rather than using an opaque
    /// Markdown fallback.
    pub fn plan_migration(roots: &[PathBuf]) -> Result<MigrationJournal, MissionError> {
        let mut records = Vec::new();
        for workspace in roots {
            let store = MissionStore::open(workspace);
            if !store.root.is_dir() {
                continue;
            }
            let mut directories = fs::read_dir(&store.root)?
                .filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .collect::<Vec<_>>();
            directories.sort_by_key(|e| e.file_name());
            for entry in directories {
                let Ok(mission_id) = entry.file_name().to_string_lossy().parse() else {
                    continue;
                };
                let accepted = store.get(mission_id)?;
                let diagnostics =
                    validate_manifest_shape(&accepted.bundle.manifest, Path::new("accepted.json"));
                if !diagnostics.is_empty() {
                    return Err(MissionError::Validation(diagnostics.len(), diagnostics));
                }
                match MissionLifecycle::open(workspace).get_mission_source(mission_id) {
                    Ok(_) => continue,
                    Err(error)
                        if fs::symlink_metadata(entry.path().join("source.json")).is_ok() =>
                    {
                        // Existing but unreadable sources are not legacy records.
                        return Err(error);
                    }
                    Err(MissionError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
                let converted = migration_source(workspace, &accepted)?;
                records.push(MigrationRecord {
                    workspace: workspace.clone(),
                    mission_id,
                    source_revision: accepted.bundle.manifest.revision,
                    payload_digest: digest(&serde_json::to_vec(&converted)?),
                    snapshot: store
                        .root
                        .join(".migration-snapshots")
                        .join(mission_id.to_string()),
                    state: MigrationState::Planned,
                    diagnostic: None,
                });
            }
        }
        Ok(MigrationJournal {
            id: Uuid::new_v4(),
            records,
        })
    }

    /// Applies only planned/failed records with their original CAS.  Each
    /// record directory is copied before mutation, making a batch intentionally
    /// non-atomic but individually recoverable.
    pub fn apply_migration(journal: &mut MigrationJournal) -> Result<(), MissionError> {
        persist(journal)?;
        for record in &mut journal.records {
            if !matches!(
                record.state,
                MigrationState::Planned | MigrationState::Failed
            ) {
                continue;
            }
            let store = MissionStore::open(&record.workspace);
            let current = match store.get(record.mission_id) {
                Ok(value) => value,
                Err(error) => {
                    record.state = MigrationState::Failed;
                    record.diagnostic = Some(describe_migration_error(&error));
                    continue;
                }
            };
            if current.bundle.manifest.revision != record.source_revision {
                record.state = MigrationState::Failed;
                record.diagnostic =
                    Some("source revision changed; original CAS no longer applies".into());
                continue;
            }
            if fs::symlink_metadata(
                store
                    .root
                    .join(record.mission_id.to_string())
                    .join("source.json"),
            )
            .is_ok()
            {
                record.state = MigrationState::Failed;
                record.diagnostic = Some(
                    "typed source appeared after planning; preserve it and replan explicitly"
                        .into(),
                );
                continue;
            }
            let source = match migration_source(&record.workspace, &current) {
                Ok(source) => source,
                Err(error) => {
                    record.state = MigrationState::Failed;
                    record.diagnostic = Some(describe_migration_error(&error));
                    continue;
                }
            };
            if digest(&serde_json::to_vec(&source)?) != record.payload_digest {
                record.state = MigrationState::Failed;
                record.diagnostic = Some("planned typed payload changed".into());
                continue;
            }
            let directory = store.root.join(record.mission_id.to_string());
            let snapshot = record.snapshot.join(journal.id.to_string());
            if let Err(error) = copy_tree(&directory, &snapshot) {
                record.state = MigrationState::Failed;
                record.diagnostic = Some(error.to_string());
                continue;
            }
            let result = MissionLifecycle::open(&record.workspace).save_typed(
                &source,
                record.source_revision,
                false,
            );
            match result {
                Ok(()) => {
                    record.state = MigrationState::Applied;
                }
                Err(error) => {
                    record.state = MigrationState::Failed;
                    record.diagnostic = Some(describe_migration_error(&error));
                }
            }
        }
        persist(journal)?;
        Ok(())
    }

    pub fn read_migration(journal: &MigrationJournal) -> &MigrationJournal {
        journal
    }

    pub fn resume_migration(journal: &mut MigrationJournal) -> Result<(), MissionError> {
        Self::apply_migration(journal)
    }

    /// Compensates only records this journal applied, preserving every revision.
    pub fn rollback_migration(journal: &mut MigrationJournal) -> Result<(), MissionError> {
        for record in &mut journal.records {
            if record.state != MigrationState::Applied {
                continue;
            }
            let store = MissionStore::open(&record.workspace);
            store.prepare_write()?;
            let _lock = typed_source::TypedLock::acquire(&store.root, record.mission_id)?;
            let current = store.get(record.mission_id)?;
            if current.bundle.manifest.revision != record.source_revision + 1 {
                record.diagnostic = Some("migration target drifted; refusing rollback".into());
                continue;
            }
            let snapshot = record.snapshot.join(journal.id.to_string());
            let mut restored: AcceptedMission =
                serde_json::from_slice(&fs::read(snapshot.join("accepted.json"))?)?;
            if restored.mission_id != record.mission_id
                || restored.bundle.manifest.revision != record.source_revision
                || snapshot.join("source.json").exists()
            {
                return Err(migration_error(
                    "snapshot",
                    0,
                    "Original native snapshot identity mismatch.",
                ));
            }
            restored.bundle.manifest.revision = current
                .bundle
                .manifest
                .revision
                .checked_add(1)
                .ok_or_else(|| migration_error("revision", 0, "revision exhausted"))?;
            restored.label_mapping.revision = restored.bundle.manifest.revision;
            let rendered = if snapshot.join("generated").is_dir() {
                Some(store.render_projection(&restored.bundle, record.mission_id, None)?)
            } else {
                None
            };
            store.write_authority_for_repair(&restored, None, rendered.as_ref(), true)?;
            record.state = MigrationState::RolledBack;
        }
        persist(journal)?;
        Ok(())
    }
}

fn migration_source(
    workspace: &Path,
    accepted: &AcceptedMission,
) -> Result<TypedDraft, MissionError> {
    let mut source = convert(workspace, accepted)?;
    source.revision = source
        .revision
        .checked_add(1)
        .ok_or_else(|| migration_error("manifest", 0, "revision exhausted"))?;
    Ok(source)
}

pub(super) fn convert(
    workspace: &Path,
    accepted: &AcceptedMission,
) -> Result<TypedDraft, MissionError> {
    let mut diagnostics =
        validate_manifest_shape(&accepted.bundle.manifest, Path::new("accepted.json"));
    for waypoint in &accepted.bundle.waypoints {
        if let Err(diagnostic) = outcome_plan::validate_outcome_plan(
            waypoint.outcome_plan.as_ref(),
            accepted.bundle.manifest.schema_version == OUTCOME_BUNDLE_SCHEMA_VERSION,
            &waypoint.waypoint_id,
        ) {
            diagnostics.push(diagnostic);
        }
    }
    if !diagnostics.is_empty() {
        return Err(MissionError::Validation(diagnostics.len(), diagnostics));
    }
    Ok(TypedDraft {
        mission_id: accepted.mission_id,
        schema_version: if accepted.bundle.manifest.schema_version == OUTCOME_BUNDLE_SCHEMA_VERSION
        {
            OUTCOME_SOURCE_SCHEMA_VERSION
        } else {
            TYPED_SOURCE_SCHEMA_VERSION
        },
        revision: accepted.bundle.manifest.revision,
        title: accepted.bundle.manifest.title.clone(),
        objective: accepted.bundle.manifest.objective.clone(),
        waypoints: accepted
            .bundle
            .waypoints
            .iter()
            .map(|w| {
                let ticket_uri = w.ticket_ref.as_ref().or(match &w.scope {
                    Scope::TicketBacked { ticket_ref } => Some(ticket_ref),
                    Scope::SingleSession => None,
                });
                let ticket_id = ticket_uri
                    .map(|reference| local_entity_reference_id(workspace, reference, "ticket"))
                    .transpose()?;
                if let Scope::TicketBacked { ticket_ref } = &w.scope
                    && Some(local_entity_reference_id(workspace, ticket_ref, "ticket")?)
                        != ticket_id
                {
                    return Err(migration_error(
                        &w.waypoint_id,
                        0,
                        "Ticket scope and reference identify different Tickets.",
                    ));
                }
                let child_mission_id = w
                    .child_mission_ref
                    .as_deref()
                    .map(|reference| local_entity_reference_id(workspace, reference, "mission"))
                    .transpose()?;
                Ok(TypedWaypoint {
                    outcome_plan: w.outcome_plan.clone(),
                    waypoint_id: w.waypoint_id.clone(),
                    title: w.title.clone(),
                    prompt: w.prompt.clone(),
                    part: PartSource {
                        waypoint_id: w.waypoint_id.clone(),
                        document: parse_markdown(&w.part_markdown, &w.waypoint_id)?,
                    },
                    depends_on: w.depends_on.clone(),
                    validation: w.validation.clone(),
                    commit_checkpoint: w.commit_checkpoint.clone(),
                    non_goals: w.non_goals.clone(),
                    relationships: w.relationships.clone(),
                    ownership_refs: OwnershipRefs {
                        ticket_id,
                        child_mission_id,
                        ticket_uri: ticket_uri.cloned(),
                        child_mission_uri: w.child_mission_ref.clone(),
                    },
                })
            })
            .collect::<Result<Vec<_>, MissionError>>()?,
    })
}

// This deliberately accepts only the W2 block vocabulary.  A non-empty line
// is always represented as text, so no source bytes are silently discarded.
fn digest(bytes: &[u8]) -> String {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h = (h ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}
fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
fn persist(journal: &MigrationJournal) -> Result<(), MissionError> {
    let bytes = serde_json::to_vec_pretty(journal)?;
    let mut roots = BTreeSet::new();
    for record in &journal.records {
        roots.insert(record.workspace.clone());
    }
    for root in roots {
        let path = MissionStore::open(&root).root.join(".migration-journal");
        fs::create_dir_all(&path)?;
        fs::write(path.join(format!("{}.json", journal.id)), &bytes)?;
    }
    Ok(())
}
mod markdown;

pub(super) use markdown::{describe_migration_error, migration_error, parse_markdown};
