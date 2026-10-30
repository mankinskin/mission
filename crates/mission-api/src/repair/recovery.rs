use super::*;

impl MissionLifecycle {
    pub fn repair_journal(
        &self,
        source_root: &Path,
        journal_id: Uuid,
    ) -> Result<RepairJournal, MissionError> {
        self.repair_root_matches(source_root, source_root)
            .then_some(())
            .ok_or_else(|| {
                repair_error(
                    "repair-root-mismatch",
                    "journal reads require the same explicit source workspace root.",
                )
            })?;
        let path = self.repair_journal_path(journal_id);
        reject_symlink_ancestors(&path)?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(repair_error(
                "repair-journal-type-invalid",
                "journal path must be a regular non-symlink file.",
            ));
        }
        let journal: RepairJournal = serde_json::from_slice(&fs::read(path)?)?;
        if journal.journal_id != journal_id
            || !self.repair_root_matches(source_root, &journal.source_root)
        {
            return Err(repair_error(
                "repair-journal-identity-mismatch",
                "journal identity or source root does not match the requested operation.",
            ));
        }
        let mut seen = BTreeSet::new();
        for record in &journal.records {
            if !seen.insert(record.mission_id) {
                return Err(repair_error(
                    "repair-journal-duplicate-record",
                    "journal contains duplicate Mission identities.",
                ));
            }
            if let Some(candidate) = &record.candidate {
                if candidate.mission_id != record.mission_id
                    || !self.repair_root_matches(source_root, &candidate.source_root)
                {
                    return Err(repair_error(
                        "repair-journal-candidate-identity-mismatch",
                        "candidate identity or source root differs from the journal.",
                    ));
                }
            }
            if let Some(snapshot) = &record.snapshot {
                if snapshot != &self.repair_snapshot_path(journal_id, record.mission_id) {
                    return Err(repair_error(
                        "repair-journal-snapshot-path-invalid",
                        "snapshot path differs from the journal-owned location.",
                    ));
                }
            }
        }
        Ok(journal)
    }

    pub fn repair_resume(
        &self,
        source_root: &Path,
        journal_id: Uuid,
    ) -> Result<RepairResponse, MissionError> {
        let _journal_lock = self.acquire_repair_journal_lock(journal_id)?;
        let mut journal = self.repair_journal(source_root, journal_id)?;
        let mut response = response_from_journal(&journal);
        if journal.state == RepairJournalState::Applied {
            for (index, record) in journal.records.iter().enumerate() {
                if record.state == RepairRecordState::Applied {
                    let expected =
                        record.after_payload_fingerprint.as_deref().ok_or_else(|| {
                            repair_error(
                                "repair-journal-after-fingerprint-missing",
                                "applied record has no durable after-state fingerprint.",
                            )
                        })?;
                    self.verify_owned_fingerprint(
                        &journal,
                        index,
                        expected,
                        "repair-resume-drift",
                        "applied Mission differs from this journal's durable after-state.",
                    )?;
                }
            }
            return Ok(response);
        }
        if journal.state == RepairJournalState::RolledBack {
            for (index, record) in journal.records.iter().enumerate() {
                if record.state == RepairRecordState::RolledBack {
                    let expected =
                        record
                            .rollback_payload_fingerprint
                            .as_deref()
                            .ok_or_else(|| {
                                repair_error(
                                    "repair-journal-rollback-fingerprint-missing",
                                    "rolled-back record has no durable compensation fingerprint.",
                                )
                            })?;
                    self.verify_owned_fingerprint(
                        &journal,
                        index,
                        expected,
                        "repair-resume-drift",
                        "rolled-back Mission differs from this journal's durable compensation state.",
                    )?;
                }
            }
            return Ok(response);
        }
        for index in 0..journal.records.len() {
            if journal.records[index].state == RepairRecordState::Applied {
                let expected = journal.records[index]
                    .after_payload_fingerprint
                    .as_deref()
                    .ok_or_else(|| {
                        repair_error(
                            "repair-journal-after-fingerprint-missing",
                            "applied record has no durable after-state fingerprint.",
                        )
                    })?;
                self.verify_owned_fingerprint(
                    &journal,
                    index,
                    expected,
                    "repair-resume-drift",
                    "applied Mission differs from this journal's durable after-state.",
                )?;
                continue;
            }
            if journal.records[index].state == RepairRecordState::AlreadyConformant {
                continue;
            }
            match self.apply_repair_record(&mut journal, index) {
                Ok(()) => {}
                Err(error) => {
                    let diagnostic = repair_diagnostic(&error);
                    journal.records[index].state = RepairRecordState::Blocked;
                    journal.records[index].diagnostic = Some(diagnostic.clone());
                    journal.state = RepairJournalState::Partial;
                    self.persist_repair_journal(&journal, false)?;
                    response.status = "blocked".into();
                    response.diagnostics.push(diagnostic);
                    response.actual_journal_id = Some(journal_id);
                    response.journal = Some(journal);
                    return Ok(response);
                }
            }
        }
        journal.state = RepairJournalState::Applied;
        self.persist_repair_journal(&journal, false)?;
        response.status = "applied".into();
        response.actual_journal_id = Some(journal_id);
        response.journal = Some(journal);
        Ok(response)
    }

    pub fn repair_rollback(
        &self,
        source_root: &Path,
        journal_id: Uuid,
    ) -> Result<RepairResponse, MissionError> {
        let _journal_lock = self.acquire_repair_journal_lock(journal_id)?;
        let mut journal = self.repair_journal(source_root, journal_id)?;
        let mut response = response_from_journal(&journal);
        if journal.state == RepairJournalState::RolledBack {
            for (index, record) in journal.records.iter().enumerate() {
                if record.state == RepairRecordState::RolledBack {
                    let expected =
                        record
                            .rollback_payload_fingerprint
                            .as_deref()
                            .ok_or_else(|| {
                                repair_error(
                                    "repair-journal-rollback-fingerprint-missing",
                                    "rolled-back record has no durable compensation fingerprint.",
                                )
                            })?;
                    self.verify_owned_fingerprint(
                        &journal,
                        index,
                        expected,
                        "repair-rollback-drift",
                        "rolled-back Mission no longer matches the journaled compensation state.",
                    )?;
                }
            }
            return Ok(response);
        }
        for index in (0..journal.records.len()).rev() {
            let record = &journal.records[index];
            if record.state == RepairRecordState::RolledBack {
                let expected = record
                    .rollback_payload_fingerprint
                    .as_deref()
                    .ok_or_else(|| {
                        repair_error(
                            "repair-journal-rollback-fingerprint-missing",
                            "rolled-back record has no durable compensation fingerprint.",
                        )
                    })?;
                self.verify_owned_fingerprint(
                    &journal,
                    index,
                    expected,
                    "repair-rollback-drift",
                    "rolled-back Mission no longer matches the journaled compensation state.",
                )?;
                continue;
            }
            let may_have_changed = matches!(
                record.state,
                RepairRecordState::Applied | RepairRecordState::RollingBack
            ) || (matches!(
                record.state,
                RepairRecordState::Failed | RepairRecordState::Blocked
            ) && record.snapshot.is_some());
            if !may_have_changed {
                continue;
            }
            match self.rollback_repair_record(&mut journal, index) {
                Ok(()) => {}
                Err(error) => {
                    let diagnostic = repair_diagnostic(&error);
                    journal.records[index].diagnostic = Some(diagnostic.clone());
                    journal.state = RepairJournalState::Partial;
                    self.persist_repair_journal(&journal, false)?;
                    response.status = "blocked".into();
                    response.diagnostics.push(diagnostic);
                    response.actual_journal_id = Some(journal_id);
                    response.journal = Some(journal);
                    return Ok(response);
                }
            }
        }
        journal.state = RepairJournalState::RolledBack;
        self.persist_repair_journal(&journal, false)?;
        response.status = "rolled-back".into();
        response.actual_journal_id = Some(journal_id);
        response.journal = Some(journal);
        Ok(response)
    }

    pub(super) fn verify_owned_fingerprint(
        &self,
        journal: &RepairJournal,
        index: usize,
        expected: &str,
        code: &'static str,
        message: &str,
    ) -> Result<(), MissionError> {
        let mission_id = journal.records[index].mission_id;
        let _lock = typed_source::TypedLock::acquire(&self.store.root, mission_id)?;
        let mission_dir = self.store.root.join(mission_id.to_string());
        let actual = payload_fingerprint(&fingerprint_files(&mission_dir)?);
        if actual != expected {
            return Err(repair_error(code, message));
        }
        Ok(())
    }

    pub(super) fn repair_root_matches(&self, explicit_root: &Path, requested_root: &Path) -> bool {
        let Some(expected_root) = self.store.root.ancestors().nth(3) else {
            return false;
        };
        if !explicit_root.is_absolute()
            || !requested_root.is_absolute()
            || contains_symlink(explicit_root)
            || contains_symlink(requested_root)
        {
            return false;
        }
        let Ok(explicit) = explicit_root.canonicalize() else {
            return false;
        };
        let Ok(requested) = requested_root.canonicalize() else {
            return false;
        };
        let Ok(expected) = expected_root.canonicalize() else {
            return false;
        };
        explicit == requested && requested == expected
    }

    pub(super) fn repair_journal_path(&self, journal_id: Uuid) -> PathBuf {
        self.store
            .root
            .parent()
            .unwrap_or(&self.store.root)
            .join(".repair-journals")
            .join(format!("{journal_id}.json"))
    }

    pub(super) fn acquire_repair_journal_lock(
        &self,
        journal_id: Uuid,
    ) -> Result<File, MissionError> {
        let path = self.repair_journal_path(journal_id);
        reject_symlink_ancestors(&path)?;
        let parent = path.parent().ok_or_else(|| {
            repair_error("repair-journal-path-invalid", "journal path has no parent.")
        })?;
        fs::create_dir_all(parent)?;
        let lock_path = parent.join(format!(".{journal_id}.lock"));
        reject_symlink_ancestors(&lock_path)?;
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock_file.lock_exclusive()?;
        Ok(lock_file)
    }

    pub(super) fn repair_snapshot_path(&self, journal_id: Uuid, mission_id: Uuid) -> PathBuf {
        self.store
            .root
            .parent()
            .unwrap_or(&self.store.root)
            .join(".repair-snapshots")
            .join(journal_id.to_string())
            .join(mission_id.to_string())
    }

    pub(super) fn persist_repair_journal(
        &self,
        journal: &RepairJournal,
        create: bool,
    ) -> Result<(), MissionError> {
        let path = self.repair_journal_path(journal.journal_id);
        reject_symlink_ancestors(&path)?;
        fs::create_dir_all(path.parent().ok_or_else(|| {
            repair_error("repair-journal-path-invalid", "journal path has no parent.")
        })?)?;
        let bytes = serde_json::to_vec_pretty(journal)?;
        let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let write_result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            if create {
                fs::hard_link(&temporary, &path)?;
                fs::remove_file(&temporary)?;
            } else {
                fs::rename(&temporary, &path)?;
            }
            Ok::<_, std::io::Error>(())
        })();
        if let Err(error) = write_result {
            if temporary.exists() {
                fs::remove_file(&temporary)?;
            }
            return Err(error.into());
        }
        if let Some(parent) = path.parent() {
            sync_directory_chain(parent)?;
        }
        Ok(())
    }
}
