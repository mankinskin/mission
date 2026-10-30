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

pub(super) struct TypedLock {
    _file: fs::File,
}

impl TypedLock {
    pub(super) fn acquire(root: &Path, mission_id: Uuid) -> Result<Self, MissionError> {
        Self::acquire_for(root, mission_id, std::time::Duration::from_secs(30))
    }

    /// Read coherently without creating a lock or accepting foreign lock paths.
    pub(super) fn acquire_shared_existing(
        root: &Path,
        mission_id: Uuid,
    ) -> Result<Self, MissionError> {
        ensure_store_chain(root)?;
        let flock = root.join(format!(".{mission_id}.typed.flock"));
        match fs::symlink_metadata(&flock) {
            Ok(meta) if meta.file_type().is_file() => {}
            Ok(_) => {
                return Err(MissionError::PublicationRecovery {
                    mission_id,
                    message: format!(
                        "typed lock file {} is a symlink or not a regular file; refusing a coherent read outside the store",
                        flock.display()
                    ),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(MissionError::PublicationRecovery {
                    mission_id,
                    message: format!(
                        "typed lock file {} is absent, so a coherent read cannot be proven; run a typed write or repair to restore it",
                        flock.display()
                    ),
                });
            }
            Err(error) => return Err(error.into()),
        }
        let file = fs::OpenOptions::new().read(true).open(&flock)?;
        let started = std::time::Instant::now();
        loop {
            match file.try_lock_shared() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) => {
                    if started.elapsed() >= std::time::Duration::from_secs(30) {
                        return Err(typed_error(
                            "typed-lock-timeout",
                            &format!("a writer holds the lock for Mission {mission_id}."),
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        let legacy = root.join(format!(".{mission_id}.typed.lock"));
        match fs::symlink_metadata(&legacy) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self { _file: file }),
            Err(error) => Err(error.into()),
            Ok(_) => Err(MissionError::PublicationRecovery {
                mission_id,
                message: format!(
                    "legacy typed write lock {} exists and its owner cannot be proven; inspect it once no older writer is running",
                    legacy.display()
                ),
            }),
        }
    }

    pub(super) fn acquire_for(
        root: &Path,
        mission_id: Uuid,
        timeout: std::time::Duration,
    ) -> Result<Self, MissionError> {
        ensure_store_chain(root)?;
        let started = std::time::Instant::now();
        let flock = root.join(format!(".{mission_id}.typed.flock"));
        match fs::symlink_metadata(&flock) {
            Ok(meta) if !meta.file_type().is_file() => {
                return Err(MissionError::PublicationRecovery {
                    mission_id,
                    message: format!(
                        "typed lock file {} is a symlink or not a regular file; refusing to lock outside the store",
                        flock.display()
                    ),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&flock)?;
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(fs::TryLockError::WouldBlock) => {
                    if started.elapsed() >= timeout {
                        return Err(typed_error(
                            "typed-lock-timeout",
                            &format!("another writer holds the lock for Mission {mission_id}."),
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
        let legacy = root.join(format!(".{mission_id}.typed.lock"));
        let grace = timeout.min(std::time::Duration::from_secs(2));
        let waiting = std::time::Instant::now();
        loop {
            match fs::symlink_metadata(&legacy) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Self { _file: file });
                }
                Err(error) => return Err(error.into()),
                Ok(_) if waiting.elapsed() >= grace => {
                    return Err(MissionError::PublicationRecovery {
                        mission_id,
                        message: format!(
                            "legacy typed write lock {} exists and its owner cannot be proven; inspect it once no older writer is running",
                            legacy.display()
                        ),
                    });
                }
                Ok(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
    }
}

/// Lock order: Mission typed locks, then ascending Ticket link locks.
pub struct TicketLinkLock {
    _file: fs::File,
}

impl TicketLinkLock {
    pub(super) fn acquire(root: &Path, ticket_id: Uuid) -> Result<Self, MissionError> {
        ensure_store_chain(root)?;
        let flock = root.join(format!(".{ticket_id}.ticket-link.flock"));
        match fs::symlink_metadata(&flock) {
            Ok(meta) if !meta.file_type().is_file() => {
                return Err(typed_error(
                    "ticket-link-lock-unsafe",
                    &format!("Ticket link lock {} is not a regular file", flock.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&flock)?;
        let started = std::time::Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(fs::TryLockError::WouldBlock) => {
                    if started.elapsed() >= std::time::Duration::from_secs(30) {
                        return Err(typed_error(
                            "ticket-link-lock-timeout",
                            &format!("another holder keeps the link lock for Ticket {ticket_id}."),
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(fs::TryLockError::Error(error)) => return Err(error.into()),
            }
        }
    }
    pub(super) fn acquire_all(
        root: &Path,
        ids: &std::collections::BTreeSet<Uuid>,
    ) -> Result<Vec<Self>, MissionError> {
        ids.iter().map(|id| Self::acquire(root, *id)).collect()
    }
}

#[derive(Default)]
pub(super) struct ImportOptions<'a> {
    pub(super) authority: Option<&'a approval_transaction::EntryAuthority>,
    pub(super) classify_nonsemantic: bool,
}

mod io;
mod journal;
mod lifecycle_source;
mod store;
mod store_helpers;
mod typed_validation;

use io::*;
pub(super) use journal::*;
pub(super) use lifecycle_source::accepted_from_typed;
pub(super) use store_helpers::*;
pub(super) use typed_validation::{render_blocks, typed_error, validate_typed};

#[cfg(test)]
mod lock_tests;
#[cfg(test)]
mod recovery_tests;

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
