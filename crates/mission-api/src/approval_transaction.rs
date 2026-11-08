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

    /// Check the complete published state while retaining the publication lock.
    pub fn verify_integrity(&self) -> Result<String, MissionError> {
        self.verify_integrity_inner(true)
    }

    /// Entry validates the immutable output, not a fresh projection of mutable
    /// external Ticket status. Legal Ticket progress must not revoke approval.
    pub fn verify_entry_integrity(&self) -> Result<String, MissionError> {
        self.verify_integrity_inner(false)
    }

    fn verify_integrity_inner(&self, check_projection: bool) -> Result<String, MissionError> {
        let store = &self.lifecycle.store;
        self.control_state()
            .and_then(|state| state.ensure_settled())
            .map_err(|error| record_error("mission-control-invalid", &error.to_string()))?;
        store.verify_output_history(self.mission_id)?;
        let accepted = store.get(self.mission_id)?;
        let workspace = store.root.ancestors().nth(3).unwrap_or(&store.root);
        crate::LifecycleJournal::read(workspace, self.mission_id)
            .and_then(|journal| {
                journal.validate_waypoint_targets(
                    &accepted
                        .bundle
                        .waypoints
                        .iter()
                        .map(|waypoint| waypoint.waypoint_id.clone())
                        .collect(),
                )
            })
            .map_err(|error| record_error("mission-control-invalid", &error.to_string()))?;
        validate_bundle(&accepted.bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        store
            .validate_child_missions(self.mission_id, &accepted.bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        store.validate_requirement_references(&accepted.bundle)?;
        store.validate_artifact_references(self.mission_id, &accepted.bundle)?;
        waypoint_relationships::resolve_bundle(workspace, &accepted.bundle)?;
        let source = self.lifecycle.get_mission_source(self.mission_id)?;
        typed_source::validate_typed(&source, true)?;
        if typed_source::accepted_from_typed(&source, Some(&accepted))? != accepted {
            return Err(record_error(
                "authority-diverged",
                "Typed source and accepted Mission content differ.",
            ));
        }
        if (check_projection || self.mission_dir().join("mission.md").exists())
            && fs::read(self.mission_dir().join("mission.md"))?
                != self.lifecycle.render_typed(&source).as_bytes()
        {
            return Err(record_error(
                "generated-changed",
                "mission.md differs from the typed source.",
            ));
        }
        store.get_artifact_provenance(self.mission_id, accepted.bundle.manifest.revision)?;
        if check_projection {
            let rendered = store.render_projection(&accepted.bundle, self.mission_id, None)?;
            let diagnostics = rendered.check_generated(&self.mission_dir().join("generated"))?;
            if !diagnostics.is_empty() {
                return Err(MissionError::Validation(diagnostics.len(), diagnostics));
            }
        }
        let review_plan = self.review_plan()?;
        if (check_projection
            || self
                .mission_dir()
                .join(plan_operations::REVIEW_PLAN_FILE)
                .exists())
            && fs::read(self.mission_dir().join(plan_operations::REVIEW_PLAN_FILE))?
                != serde_json::to_vec_pretty(&review_plan)?
        {
            return Err(record_error(
                "projection-diverged",
                "Persisted review plan differs from current authority.",
            ));
        }
        store.output_contract_binding(self.mission_id)
    }

    pub fn archived_output_contract_binding(&self, revision: u64) -> Result<String, MissionError> {
        self.lifecycle
            .store
            .archived_output_contract_binding(self.mission_id, revision)
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
