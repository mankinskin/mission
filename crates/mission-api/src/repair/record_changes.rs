use super::*;

impl MissionLifecycle {
    pub(super) fn apply_repair_record(
        &self,
        journal: &mut RepairJournal,
        index: usize,
    ) -> Result<(), MissionError> {
        let (mission_id, candidate, before_fingerprint, after_fingerprint, has_snapshot) = {
            let record = &journal.records[index];
            (
                record.mission_id,
                record.candidate.clone().ok_or_else(|| {
                    repair_error(
                        "repair-candidate-missing",
                        "journal record has no candidate overlay.",
                    )
                })?,
                record.before_payload_fingerprint.clone(),
                record.after_payload_fingerprint.clone(),
                record.snapshot.is_some(),
            )
        };
        let _lock = typed_source::TypedLock::acquire(&self.store.root, mission_id)?;
        let mission_dir = self.store.root.join(mission_id.to_string());
        let current_files = fingerprint_files(&mission_dir)?;
        let current_fingerprint = payload_fingerprint(&current_files);
        let expected_after = after_fingerprint
            .as_deref()
            .unwrap_or(&candidate.predicted_payload_fingerprint);
        if current_fingerprint == expected_after {
            if !has_snapshot {
                return Err(repair_error(
                    "repair-after-state-unowned",
                    "matching after-state has no operation-owned recovery snapshot.",
                ));
            }
            journal.records[index].after_payload_fingerprint =
                Some(candidate.predicted_payload_fingerprint.clone());
            journal.records[index].after_revision = Some(candidate.predicted_revision);
            journal.records[index].state = RepairRecordState::Applied;
            return self.persist_repair_journal(journal, false);
        }
        if current_fingerprint != before_fingerprint {
            return Err(repair_error(
                "repair-payload-drift",
                "Mission payload differs from both the journal's before and after states.",
            ));
        }
        if !has_snapshot {
            let snapshot = self.repair_snapshot_path(journal.journal_id, mission_id);
            if snapshot.exists() {
                if payload_fingerprint(&fingerprint_files(&snapshot)?) != before_fingerprint {
                    return Err(repair_error(
                        "repair-snapshot-conflict",
                        "existing recovery snapshot differs from the journaled before-state.",
                    ));
                }
            } else {
                copy_tree(&mission_dir, &snapshot)?;
            }
            journal.records[index].snapshot = Some(snapshot);
            journal.records[index].state = RepairRecordState::SnapshotCreated;
            self.persist_repair_journal(journal, false)?;
        }
        self.store.write_authority(
            &candidate.accepted,
            Some(&candidate.typed_source),
            Some(&candidate.rendered),
        )?;
        let after = payload_fingerprint(&fingerprint_files(&mission_dir)?);
        if after != candidate.predicted_payload_fingerprint {
            return Err(repair_error(
                "repair-postwrite-fingerprint-mismatch",
                "persisted Mission differs from the journaled candidate projection.",
            ));
        }
        let record = &mut journal.records[index];
        record.after_payload_fingerprint = Some(after);
        record.after_revision = Some(candidate.predicted_revision);
        record.state = RepairRecordState::Applied;
        record.diagnostic = None;
        self.persist_repair_journal(journal, false)
    }

    pub(super) fn rollback_repair_record(
        &self,
        journal: &mut RepairJournal,
        index: usize,
    ) -> Result<(), MissionError> {
        let (mission_id, before_fingerprint, after_fingerprint, snapshot, state) = {
            let record = &journal.records[index];
            (
                record.mission_id,
                record.before_payload_fingerprint.clone(),
                record
                    .after_payload_fingerprint
                    .clone()
                    .or_else(|| {
                        record
                            .candidate
                            .as_ref()
                            .map(|candidate| candidate.predicted_payload_fingerprint.clone())
                    })
                    .unwrap_or_default(),
                record.snapshot.clone(),
                record.state.clone(),
            )
        };
        let _lock = typed_source::TypedLock::acquire(&self.store.root, mission_id)?;
        let mission_dir = self.store.root.join(mission_id.to_string());
        let current_files = fingerprint_files(&mission_dir)?;
        let current_fingerprint = payload_fingerprint(&current_files);
        if state == RepairRecordState::RolledBack {
            if journal.records[index]
                .rollback_payload_fingerprint
                .as_deref()
                == Some(current_fingerprint.as_str())
            {
                return Ok(());
            }
            return Err(repair_error(
                "repair-rollback-drift",
                "rolled-back Mission no longer matches the journaled compensation state.",
            ));
        }
        if current_fingerprint == before_fingerprint && snapshot.is_some() {
            let record = &mut journal.records[index];
            record.rollback_revision = Some(record.before_revision);
            record.rollback_payload_fingerprint = Some(before_fingerprint);
            record.state = RepairRecordState::RolledBack;
            return self.persist_repair_journal(journal, false);
        }
        if journal.records[index]
            .rollback_payload_fingerprint
            .as_deref()
            == Some(current_fingerprint.as_str())
        {
            journal.records[index].state = RepairRecordState::RolledBack;
            return self.persist_repair_journal(journal, false);
        }
        if current_fingerprint != after_fingerprint {
            return Err(repair_error(
                "repair-rollback-drift",
                "current Mission no longer matches this operation's durable after-state.",
            ));
        }
        let snapshot = snapshot.as_ref().ok_or_else(|| {
            repair_error(
                "repair-snapshot-missing",
                "journaled recovery snapshot is missing.",
            )
        })?;
        if snapshot != &self.repair_snapshot_path(journal.journal_id, mission_id)
            || payload_fingerprint(&fingerprint_files(snapshot)?) != before_fingerprint
        {
            return Err(repair_error(
                "repair-snapshot-conflict",
                "recovery snapshot path or content differs from the journaled before-state.",
            ));
        }
        let snapshot_accepted: AcceptedMission =
            serde_json::from_slice(&fs::read(snapshot.join("accepted.json"))?)?;
        let current = self.store.get(mission_id)?;
        let next_revision = current
            .bundle
            .manifest
            .revision
            .checked_add(1)
            .ok_or_else(|| repair_error("revision-exhausted", "revision cannot be incremented."))?;
        let history = mission_dir.join("history");
        if history
            .join(format!("revision-{next_revision}.json"))
            .exists()
        {
            return Err(repair_error(
                "repair-rollback-history-conflict",
                "compensation revision history already exists.",
            ));
        }
        let mut restored = snapshot_accepted;
        restored.bundle.manifest.revision = next_revision;
        restored.label_mapping.revision = next_revision;
        let source_path = snapshot.join("source.json");
        let source = if source_path.is_file() {
            let mut source = load_source_from_directory(snapshot, mission_id)?;
            source.revision = next_revision;
            Some(source)
        } else {
            None
        };
        if source.is_some()
            && history
                .join(format!("source-revision-{next_revision}.json"))
                .exists()
        {
            return Err(repair_error(
                "repair-rollback-history-conflict",
                "compensation source history already exists.",
            ));
        }
        let rendered = self
            .store
            .render_projection(&restored.bundle, mission_id, None)?;
        let projected = projected_rollback_fingerprint(
            &current_files,
            &restored,
            source.as_ref(),
            &rendered,
            &MissionLifecycle {
                store: MissionStore::open(&journal.source_root),
            },
        )?;
        journal.records[index].rollback_revision = Some(next_revision);
        journal.records[index].rollback_payload_fingerprint = Some(payload_fingerprint(&projected));
        journal.records[index].state = RepairRecordState::RollingBack;
        self.persist_repair_journal(journal, false)?;
        self.store
            .write_authority_for_repair(&restored, source.as_ref(), Some(&rendered), true)?;
        let actual = payload_fingerprint(&fingerprint_files(&mission_dir)?);
        if Some(actual.as_str())
            != journal.records[index]
                .rollback_payload_fingerprint
                .as_deref()
        {
            return Err(repair_error(
                "repair-rollback-postwrite-mismatch",
                "persisted compensation differs from the journaled rollback projection.",
            ));
        }
        journal.records[index].state = RepairRecordState::RolledBack;
        journal.records[index].diagnostic = None;
        self.persist_repair_journal(journal, false)
    }
}
