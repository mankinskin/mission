//! Append-only approval records under the Mission's publication lock.
use super::*;
use std::io::Write;

/// Trusted facade transaction; never exposed through agent tools.
pub struct ApprovalTransaction<'a> {
    lifecycle: &'a MissionLifecycle,
    mission_id: Uuid,
    root: PathBuf,
    _lock: typed_source::TypedLock,
}

fn record_error(code: &'static str, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "approvals".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.into(),
        }],
    )
}

pub(super) struct EntryAuthority(());

fn sync_dir(path: &Path) -> Result<(), MissionError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

impl MissionLifecycle {
    /// Hold through Ticket entry; acquire Mission typed locks before this lock.
    pub fn lock_ticket_links(&self, ticket_id: Uuid) -> Result<TicketLinkLock, MissionError> {
        self.store.prepare_write()?;
        typed_source::TicketLinkLock::acquire(&self.store.root, ticket_id)
    }

    /// Serialize approval checks and records with semantic publication.
    pub fn begin_approval_transaction(
        &self,
        mission_id: Uuid,
    ) -> Result<ApprovalTransaction<'_>, MissionError> {
        self.store.get(mission_id)?;
        self.store.prepare_write()?;
        let lock = typed_source::TypedLock::acquire(&self.store.root, mission_id)?;
        Ok(ApprovalTransaction {
            lifecycle: self,
            mission_id,
            root: self.store.root.clone(),
            _lock: lock,
        })
    }
}

impl ApprovalTransaction<'_> {
    pub fn mission_id(&self) -> Uuid {
        self.mission_id
    }

    pub fn review_plan(&self) -> Result<ReviewPlan, MissionError> {
        self.lifecycle.review_plan_unlocked(self.mission_id)
    }

    /// Evidence-only appends preserve this revision; edit/revert advances it.
    pub fn semantic_revision(&self) -> Result<u64, MissionError> {
        self.lifecycle.semantic_revision_unlocked(self.mission_id)
    }

    /// Trusted facade entry writes retain this transaction's publication lock.
    pub fn record_progress(
        &self,
        waypoint_id: &str,
        state: WaypointState,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.lifecycle.record_progress_locked(
            self.mission_id,
            waypoint_id,
            state,
            expected,
            &EntryAuthority(()),
        )
    }

    fn mission_dir(&self) -> PathBuf {
        self.lifecycle.mission_directory(self.mission_id)
    }

    fn base(&self) -> PathBuf {
        self.mission_dir().join("approvals")
    }

    fn resolve(&self, name: &str) -> Result<PathBuf, MissionError> {
        if name.is_empty() || name.len() > 240 {
            return Err(record_error(
                "approval-record-name",
                "record name length is invalid.",
            ));
        }
        let mut path = self.base();
        for part in name.split('/') {
            if part.is_empty()
                || part.starts_with('.')
                || !part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                return Err(record_error(
                    "approval-record-name",
                    "record names require safe components without a leading dot.",
                ));
            }
            path.push(part);
        }
        Ok(path)
    }

    fn exists_safe(&self, path: &Path) -> Result<bool, MissionError> {
        let unsafe_path = || {
            record_error(
                "unsafe-approval-path",
                "approval records and their ancestors must not be symlinks.",
            )
        };
        let relative = path.strip_prefix(&self.root).map_err(|_| unsafe_path())?;
        if fs::symlink_metadata(&self.root)?.file_type().is_symlink() {
            return Err(unsafe_path());
        }
        let mut current = self.root.clone();
        for component in relative.components() {
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.file_type().is_symlink() => return Err(unsafe_path()),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(true)
    }

    pub fn read_record(&self, name: &str) -> Result<Option<Vec<u8>>, MissionError> {
        let path = self.resolve(name)?;
        if !self.exists_safe(&path)? {
            return Ok(None);
        }
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(record_error(
                "unsafe-approval-path",
                "record is not a regular file.",
            ));
        }
        Ok(Some(fs::read(path)?))
    }

    /// List direct children, excluding crash-orphaned temporary records.
    pub fn list_records(&self, dir: &str) -> Result<Vec<String>, MissionError> {
        let path = if dir.is_empty() {
            self.base()
        } else {
            self.resolve(dir)?
        };
        if !self.exists_safe(&path)? {
            return Ok(Vec::new());
        }
        if !fs::symlink_metadata(&path)?.file_type().is_dir() {
            return Err(record_error(
                "unsafe-approval-path",
                "record directory is not a directory.",
            ));
        }
        let mut names = Vec::new();
        for entry in fs::read_dir(&path)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') {
                names.push(name);
            }
        }
        names.sort();
        Ok(names)
    }

    /// Atomically create a record without replacing an existing entry.
    pub fn write_record_new(&self, name: &str, bytes: &[u8]) -> Result<(), MissionError> {
        let path = self.resolve(name)?;
        let parent = path
            .parent()
            .ok_or_else(|| record_error("approval-record-name", "record has no parent directory."))?
            .to_path_buf();
        self.exists_safe(&parent)?;
        fs::create_dir_all(&parent)?;
        if !self.exists_safe(&parent)? || !fs::symlink_metadata(&parent)?.file_type().is_dir() {
            return Err(record_error(
                "unsafe-approval-path",
                "record parent is not a directory.",
            ));
        }
        if self.exists_safe(&path)? {
            return Err(record_error(
                "approval-record-exists",
                "record already exists.",
            ));
        }
        let tmp = parent.join(format!(".tmp-{}", Uuid::new_v4()));
        let written = (|| -> std::io::Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            file.write_all(bytes)?;
            file.sync_all()
        })();
        let linked = written.and_then(|_| fs::hard_link(&tmp, &path));
        let cleanup = match fs::remove_file(&tmp) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
        match (linked, cleanup) {
            (Ok(()), Ok(())) => {}
            (Ok(()), Err(cleanup)) => {
                return Err(MissionError::PublicationRecovery {
                    mission_id: self.mission_id,
                    message: format!(
                        "approval record {name} published but removing {} failed: {cleanup}",
                        tmp.display()
                    ),
                });
            }
            (Err(error), cleanup) => {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    return Err(record_error(
                        "approval-record-exists",
                        "record already exists.",
                    ));
                }
                let context = cleanup
                    .err()
                    .map(|c| format!("; cleanup of {} also failed: {c}", tmp.display()))
                    .unwrap_or_default();
                return Err(MissionError::Io(std::io::Error::new(
                    error.kind(),
                    format!("writing approval record {name}: {error}{context}"),
                )));
            }
        }
        sync_dir(&parent)?;
        let base = self.base();
        if parent != base {
            sync_dir(&base)?;
        }
        sync_dir(&self.mission_dir())?;
        Ok(())
    }
}
