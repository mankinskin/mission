use super::*;

impl MissionLifecycle {
    pub fn repair_preview(
        &self,
        source_root: &Path,
        request: RepairRequest,
    ) -> Result<RepairResponse, MissionError> {
        let root_ok = self.repair_root_matches(source_root, &request.source_root);
        if !root_ok {
            return Err(repair_error(
                "repair-root-mismatch",
                "request source_root must match the explicit Mission workspace root.",
            ));
        }
        if request.policy != REPAIR_POLICY {
            return Err(repair_error(
                "repair-policy-unsupported",
                "policy must be forward-authoritative-repair.",
            ));
        }
        if request.records.is_empty() {
            return Err(repair_error(
                "repair-selection-empty",
                "repair requires a finite, nonempty record selection.",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut records = Vec::with_capacity(request.records.len());
        let mut candidate_overlay = BTreeMap::new();
        for selection in &request.records {
            if !seen.insert(selection.mission_id) {
                return Err(repair_error(
                    "repair-selection-duplicate",
                    "each Mission ID may appear only once in a repair request.",
                ));
            }
            let (record, candidate) = self.plan_repair_record(&request.source_root, selection);
            if let Some(candidate) = candidate {
                candidate_overlay.insert(selection.mission_id.to_string(), candidate);
            }
            records.push(record);
        }
        let proposal_bytes = serde_json::to_vec(&(&request, &records, &candidate_overlay))?;
        let proposal_id = format!("sha256:{}", sha256(&proposal_bytes));
        let all_noop = records
            .iter()
            .all(|record| record.disposition.as_deref() == Some("already_conformant"));
        let has_blockers = records.iter().any(|record| record.readiness != "ready");
        Ok(RepairResponse {
            status: if has_blockers {
                "blocked".into()
            } else if all_noop {
                "already_conformant".into()
            } else {
                "ready".into()
            },
            records,
            candidate_overlay,
            planned_journal_identity: Some(proposal_id),
            actual_journal_id: None,
            journal: None,
            diagnostics: Vec::new(),
        })
    }

    pub fn repair_apply(
        &self,
        source_root: &Path,
        request: RepairRequest,
        proposal_id: &str,
    ) -> Result<RepairResponse, MissionError> {
        self.repair_apply_with_journal_id(source_root, request, proposal_id, Uuid::new_v4())
    }

    pub(super) fn repair_apply_with_journal_id(
        &self,
        source_root: &Path,
        request: RepairRequest,
        proposal_id: &str,
        journal_id: Uuid,
    ) -> Result<RepairResponse, MissionError> {
        let mut preview = self.repair_preview(source_root, request.clone())?;
        if preview.planned_journal_identity.as_deref() != Some(proposal_id) {
            preview.status = "blocked".into();
            preview.diagnostics.push(
                "proposal-id-mismatch: current source does not match the supplied preview.".into(),
            );
            return Ok(preview);
        }
        if preview
            .records
            .iter()
            .any(|record| record.readiness != "ready")
        {
            preview.status = "blocked".into();
            preview
                .diagnostics
                .push("repair-preview-blocked: no record was changed.".into());
            return Ok(preview);
        }
        if preview.status == "already_conformant" {
            return Ok(preview);
        }

        let _journal_lock = self.acquire_repair_journal_lock(journal_id)?;
        let mut journal = RepairJournal {
            journal_id,
            source_root: request.source_root.clone(),
            proposal_id: proposal_id.to_owned(),
            state: RepairJournalState::Prepared,
            records: preview
                .records
                .iter()
                .map(|record| {
                    let candidate = preview
                        .candidate_overlay
                        .get(&record.mission_id.to_string())
                        .cloned();
                    RepairJournalRecord {
                        mission_id: record.mission_id,
                        before_revision: record.observed_revision.unwrap_or_default(),
                        after_revision: candidate
                            .as_ref()
                            .map(|candidate| candidate.predicted_revision),
                        before_payload_fingerprint: record
                            .payload_fingerprint
                            .clone()
                            .unwrap_or_default(),
                        after_payload_fingerprint: candidate
                            .as_ref()
                            .map(|candidate| candidate.predicted_payload_fingerprint.clone()),
                        rollback_revision: None,
                        rollback_payload_fingerprint: None,
                        snapshot: None,
                        state: if candidate.is_some() {
                            RepairRecordState::Pending
                        } else {
                            RepairRecordState::AlreadyConformant
                        },
                        diagnostic: None,
                        candidate,
                    }
                })
                .collect(),
        };
        self.persist_repair_journal(&journal, true)?;
        journal.state = RepairJournalState::Applying;
        self.persist_repair_journal(&journal, false)?;

        let mut response = preview;
        response.actual_journal_id = Some(journal_id);
        for index in 0..journal.records.len() {
            if journal.records[index].state == RepairRecordState::AlreadyConformant {
                continue;
            }
            match self.apply_repair_record(&mut journal, index) {
                Ok(()) => {}
                Err(error) => {
                    let diagnostic = repair_diagnostic(&error);
                    journal.records[index].state = RepairRecordState::Failed;
                    journal.records[index].diagnostic = Some(diagnostic.clone());
                    journal.state = RepairJournalState::Partial;
                    self.persist_repair_journal(&journal, false)?;
                    response.status = "partial".into();
                    response.diagnostics.push(diagnostic);
                    response.journal = Some(journal);
                    return Ok(response);
                }
            }
        }
        journal.state = RepairJournalState::Applied;
        self.persist_repair_journal(&journal, false)?;
        response.status = "applied".into();
        response.journal = Some(journal);
        Ok(response)
    }
}
