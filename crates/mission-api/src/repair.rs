use super::*;
use fs4::fs_std::FileExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
};

pub const REPAIR_POLICY: &str = "forward-authoritative-repair";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepairRequest {
    pub source_root: PathBuf,
    pub records: Vec<RepairRecordRequest>,
    pub policy: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RepairRecordRequest {
    #[schemars(with = "String")]
    pub mission_id: Uuid,
    pub expected_revision: u64,
    pub accepted_sha256: String,
    pub source_sha256: String,
    pub payload_fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairResponse {
    pub status: String,
    pub records: Vec<RepairRecordResponse>,
    pub candidate_overlay: BTreeMap<String, RepairCandidate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_journal_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_journal_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal: Option<RepairJournal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairRecordResponse {
    pub mission_id: Uuid,
    pub readiness: String,
    pub blockers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_accepted_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed_source_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_fingerprint: Option<String>,
    pub root_ok: bool,
    pub reference_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicted_revision: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairCandidate {
    pub source_root: PathBuf,
    pub mission_id: Uuid,
    pub observed_revision: u64,
    pub observed_accepted_sha256: String,
    pub observed_source_sha256: String,
    pub observed_payload_fingerprint: String,
    pub predicted_revision: u64,
    pub predicted_accepted_sha256: String,
    pub predicted_source_sha256: String,
    pub predicted_payload_fingerprint: String,
    pub typed_source: TypedDraft,
    accepted: AcceptedMission,
    rendered: RenderedRoadmap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepairJournalState {
    Prepared,
    Applying,
    Applied,
    Partial,
    RolledBack,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RepairRecordState {
    Pending,
    SnapshotCreated,
    Applied,
    AlreadyConformant,
    RollingBack,
    Failed,
    Blocked,
    RolledBack,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairJournal {
    pub journal_id: Uuid,
    pub source_root: PathBuf,
    pub proposal_id: String,
    pub state: RepairJournalState,
    pub records: Vec<RepairJournalRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairJournalRecord {
    pub mission_id: Uuid,
    pub before_revision: u64,
    pub after_revision: Option<u64>,
    pub before_payload_fingerprint: String,
    pub after_payload_fingerprint: Option<String>,
    #[serde(default)]
    pub rollback_revision: Option<u64>,
    #[serde(default)]
    pub rollback_payload_fingerprint: Option<String>,
    pub snapshot: Option<PathBuf>,
    pub state: RepairRecordState,
    pub diagnostic: Option<String>,
    pub candidate: Option<RepairCandidate>,
}

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

    fn repair_apply_with_journal_id(
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

    fn verify_owned_fingerprint(
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

    fn plan_repair_record(
        &self,
        source_root: &Path,
        request: &RepairRecordRequest,
    ) -> (RepairRecordResponse, Option<RepairCandidate>) {
        let mission_dir = self.store.root.join(request.mission_id.to_string());
        let root_ok = mission_dir.is_dir() && !contains_symlink(&mission_dir);
        let mut response = RepairRecordResponse {
            mission_id: request.mission_id,
            readiness: "blocked".into(),
            blockers: Vec::new(),
            disposition: None,
            observed_revision: None,
            observed_accepted_sha256: None,
            observed_source_sha256: None,
            payload_fingerprint: None,
            root_ok,
            reference_ok: false,
            provenance: None,
            predicted_revision: None,
        };
        if !root_ok {
            response
                .blockers
                .push("mission-root-missing-or-unsafe".into());
            return (response, None);
        }
        let accepted_path = mission_dir.join("accepted.json");
        let accepted_bytes = match fs::read(&accepted_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-read-failed: {error}"));
                return (response, None);
            }
        };
        response.observed_accepted_sha256 = Some(sha256(&accepted_bytes));
        let source_path = mission_dir.join("source.json");
        let source_bytes = match fs::read(&source_path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                response
                    .blockers
                    .push(format!("source-read-failed: {error}"));
                return (response, None);
            }
        };
        response.observed_source_sha256 = Some(
            source_bytes
                .as_deref()
                .map(sha256)
                .unwrap_or_else(|| "missing".into()),
        );
        let files = match fingerprint_files(&mission_dir) {
            Ok(files) => files,
            Err(error) => {
                response
                    .blockers
                    .push(format!("payload-fingerprint-failed: {error}"));
                return (response, None);
            }
        };
        let fingerprint = payload_fingerprint(&files);
        response.payload_fingerprint = Some(fingerprint.clone());
        let accepted: AcceptedMission = match serde_json::from_slice(&accepted_bytes) {
            Ok(accepted) => accepted,
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-json-invalid: {error}"));
                return (response, None);
            }
        };
        response.observed_revision = Some(accepted.bundle.manifest.revision);
        response.provenance = Some(if source_bytes.is_some() {
            "typed-source-present".into()
        } else {
            "converted-from-accepted-history".into()
        });
        if accepted.mission_id != request.mission_id {
            response.blockers.push("accepted-identity-mismatch".into());
        }
        let has_source = source_bytes.is_some();
        let mut source = match source_bytes {
            Some(_) => match self.get_mission_source_unchecked(request.mission_id) {
                Ok(source) => source,
                Err(error) => {
                    response
                        .blockers
                        .push(format!("typed-source-invalid: {error}"));
                    return (response, None);
                }
            },
            None => match typed_migration::convert(&accepted) {
                Ok(source) => source,
                Err(error) => {
                    response
                        .blockers
                        .push(format!("typed-conversion-blocked: {error}"));
                    return (response, None);
                }
            },
        };
        let ownership_source = match typed_migration::convert(&accepted) {
            Ok(source) => source,
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-reference-invalid: {error}"));
                return (response, None);
            }
        };
        response.reference_ok = merge_accepted_ownership(&mut source, &ownership_source);
        if !response.reference_ok {
            response
                .blockers
                .push("typed-waypoint-identity-does-not-match-accepted-record".into());
        }
        if let Err(error) = validate_bundle(&accepted.bundle) {
            response
                .blockers
                .push(format!("accepted-bundle-invalid: {}", error.len()));
        }
        if let Err(error) = validate_typed(&source, false) {
            response
                .blockers
                .push(format!("typed-source-invalid: {error}"));
        }
        let original_source_revision = source.revision;
        let Some(next_revision) = accepted.bundle.manifest.revision.checked_add(1) else {
            response.blockers.push("revision-exhausted".into());
            return (response, None);
        };
        response.predicted_revision = Some(next_revision);
        source.revision = next_revision;
        source.schema_version = TYPED_SOURCE_SCHEMA_VERSION;
        let next = match accepted_from_typed(&source, Some(&accepted)) {
            Ok(next) => next,
            Err(error) => {
                response
                    .blockers
                    .push(format!("typed-merge-failed: {error}"));
                return (response, None);
            }
        };
        if let Err(errors) = validate_bundle(&next.bundle) {
            response
                .blockers
                .push(format!("repaired-bundle-invalid: {}", errors.len()));
        }
        if let Err(errors) = self
            .store
            .validate_child_missions(request.mission_id, &next.bundle)
        {
            response
                .blockers
                .push(format!("child-reference-invalid: {}", errors.len()));
        }
        let history = mission_dir.join("history");
        if !history.is_dir() {
            response.blockers.push("history-directory-missing".into());
        }
        for name in [
            format!("revision-{next_revision}.json"),
            format!("source-revision-{next_revision}.json"),
        ] {
            if history.join(name).exists() {
                response
                    .blockers
                    .push("next-history-revision-already-exists".into());
            }
        }
        if request.expected_revision != accepted.bundle.manifest.revision {
            response.blockers.push("expected-revision-mismatch".into());
        }
        if !matches_hash(&request.accepted_sha256, &response.observed_accepted_sha256) {
            response.blockers.push("accepted-sha256-mismatch".into());
        }
        if !matches_hash(&request.source_sha256, &response.observed_source_sha256) {
            response.blockers.push("source-sha256-mismatch".into());
        }
        if request.payload_fingerprint.is_empty() {
            response
                .blockers
                .push("payload-fingerprint-required".into());
        } else if request.payload_fingerprint != fingerprint {
            response
                .blockers
                .push("payload-fingerprint-mismatch".into());
        }
        if !response.blockers.is_empty() {
            return (response, None);
        }
        let current_is_conformant = has_source
            && original_source_revision == accepted.bundle.manifest.revision
            && accepted_from_typed(
                &TypedDraft {
                    revision: accepted.bundle.manifest.revision,
                    ..source.clone()
                },
                Some(&accepted),
            )
            .is_ok_and(|candidate| candidate == accepted);
        if current_is_conformant {
            response.readiness = "ready".into();
            response.disposition = Some("already_conformant".into());
            return (response, None);
        }
        let rendered = match self.store.render_projection(&next.bundle, request.mission_id, None) {
            Ok(rendered) => rendered,
            Err(errors) => {
                response
                    .blockers
                    .push(format!("render-failed: {errors}"));
                return (response, None);
            }
        };
        let projected = match projected_fingerprint(
            &files,
            &next,
            &source,
            &rendered,
            &MissionLifecycle {
                store: MissionStore::open(source_root),
            },
        ) {
            Ok(projected) => projected,
            Err(error) => {
                response
                    .blockers
                    .push(format!("projection-failed: {error}"));
                return (response, None);
            }
        };
        let predicted_accepted_sha256 = match serde_json::to_vec_pretty(&next) {
            Ok(bytes) => sha256(&bytes),
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-serialization-failed: {error}"));
                return (response, None);
            }
        };
        let predicted_source_sha256 = match serde_json::to_vec_pretty(&stored_source(&source)) {
            Ok(bytes) => sha256(&bytes),
            Err(error) => {
                response
                    .blockers
                    .push(format!("source-serialization-failed: {error}"));
                return (response, None);
            }
        };
        let candidate = RepairCandidate {
            source_root: source_root.to_path_buf(),
            mission_id: request.mission_id,
            observed_revision: accepted.bundle.manifest.revision,
            observed_accepted_sha256: response
                .observed_accepted_sha256
                .clone()
                .unwrap_or_default(),
            observed_source_sha256: response.observed_source_sha256.clone().unwrap_or_default(),
            observed_payload_fingerprint: fingerprint,
            predicted_revision: next_revision,
            predicted_accepted_sha256,
            predicted_source_sha256,
            predicted_payload_fingerprint: payload_fingerprint(&projected),
            typed_source: source,
            accepted: next,
            rendered,
        };
        response.readiness = "ready".into();
        response.disposition = Some("repair".into());
        (response, Some(candidate))
    }

    fn apply_repair_record(
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

    fn rollback_repair_record(
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
        let rendered = self.store.render_projection(&restored.bundle, mission_id, None)?;
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

    fn repair_root_matches(&self, explicit_root: &Path, requested_root: &Path) -> bool {
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

    fn repair_journal_path(&self, journal_id: Uuid) -> PathBuf {
        self.store
            .root
            .parent()
            .unwrap_or(&self.store.root)
            .join(".repair-journals")
            .join(format!("{journal_id}.json"))
    }

    fn acquire_repair_journal_lock(&self, journal_id: Uuid) -> Result<File, MissionError> {
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

    fn repair_snapshot_path(&self, journal_id: Uuid, mission_id: Uuid) -> PathBuf {
        self.store
            .root
            .parent()
            .unwrap_or(&self.store.root)
            .join(".repair-snapshots")
            .join(journal_id.to_string())
            .join(mission_id.to_string())
    }

    fn persist_repair_journal(
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

pub fn response_from_journal(journal: &RepairJournal) -> RepairResponse {
    RepairResponse {
        status: match journal.state {
            RepairJournalState::Prepared => "prepared",
            RepairJournalState::Applying => "applying",
            RepairJournalState::Applied => "applied",
            RepairJournalState::Partial => "partial",
            RepairJournalState::RolledBack => "rolled-back",
            RepairJournalState::Blocked => "blocked",
        }
        .into(),
        records: journal
            .records
            .iter()
            .map(|record| RepairRecordResponse {
                mission_id: record.mission_id,
                readiness: if matches!(
                    record.state,
                    RepairRecordState::Failed | RepairRecordState::Blocked
                ) {
                    "blocked".into()
                } else {
                    "ready".into()
                },
                blockers: record.diagnostic.clone().into_iter().collect(),
                disposition: Some(
                    match record.state {
                        RepairRecordState::Pending => "pending",
                        RepairRecordState::SnapshotCreated => "snapshot-created",
                        RepairRecordState::Applied => "applied",
                        RepairRecordState::AlreadyConformant => "already-conformant",
                        RepairRecordState::RollingBack => "rolling-back",
                        RepairRecordState::Failed => "failed",
                        RepairRecordState::Blocked => "blocked",
                        RepairRecordState::RolledBack => "rolled-back",
                    }
                    .into(),
                ),
                observed_revision: Some(record.before_revision),
                observed_accepted_sha256: None,
                observed_source_sha256: None,
                payload_fingerprint: Some(record.before_payload_fingerprint.clone()),
                root_ok: true,
                reference_ok: true,
                provenance: record
                    .snapshot
                    .as_ref()
                    .map(|_| "journaled-snapshot".into()),
                predicted_revision: record.after_revision,
            })
            .collect(),
        candidate_overlay: BTreeMap::new(),
        planned_journal_identity: Some(journal.proposal_id.clone()),
        actual_journal_id: Some(journal.journal_id),
        journal: Some(journal.clone()),
        diagnostics: Vec::new(),
    }
}

fn merge_accepted_ownership(source: &mut TypedDraft, accepted: &TypedDraft) -> bool {
    let references: BTreeMap<_, _> = accepted
        .waypoints
        .iter()
        .map(|waypoint| {
            (
                waypoint.waypoint_id.as_str(),
                waypoint.ownership_refs.clone(),
            )
        })
        .collect();
    if source.waypoints.len() != references.len()
        || source
            .waypoints
            .iter()
            .any(|waypoint| !references.contains_key(waypoint.waypoint_id.as_str()))
    {
        return false;
    }
    for waypoint in &mut source.waypoints {
        if let Some(ownership) = references.get(waypoint.waypoint_id.as_str()) {
            waypoint.ownership_refs = ownership.clone();
        }
    }
    true
}

fn matches_hash(expected: &str, observed: &Option<String>) -> bool {
    observed.as_deref() == Some(expected)
}

fn fingerprint_files(directory: &Path) -> Result<BTreeMap<String, String>, MissionError> {
    let mut files = BTreeMap::new();
    collect_files(directory, directory, &mut files)?;
    Ok(files)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), MissionError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(repair_error(
                "repair-payload-symlink",
                "Mission payload contains a symlink.",
            ));
        }
        let path = entry.path();
        if kind.is_dir() {
            collect_files(root, &path, files)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).map_err(|_| {
                repair_error("repair-payload-path-invalid", "payload escaped its root.")
            })?;
            let mut components = Vec::new();
            for component in relative.components() {
                match component {
                    Component::Normal(value) => components.push(
                        value
                            .to_str()
                            .ok_or_else(|| {
                                repair_error(
                                    "repair-payload-path-invalid",
                                    "payload path is not valid UTF-8.",
                                )
                            })?
                            .to_owned(),
                    ),
                    _ => {
                        return Err(repair_error(
                            "repair-payload-path-invalid",
                            "payload contains an unsafe relative path.",
                        ));
                    }
                }
            }
            files.insert(components.join("/"), sha256(&fs::read(&path)?));
        } else {
            return Err(repair_error(
                "repair-payload-type-invalid",
                "Mission payload contains a non-file, non-directory entry.",
            ));
        }
    }
    Ok(())
}

fn payload_fingerprint(files: &BTreeMap<String, String>) -> String {
    let mut digest = Sha256::new();
    for (path, hash) in files {
        digest.update(path.as_bytes());
        digest.update([0]);
        digest.update(hash.as_bytes());
        digest.update([b'\n']);
    }
    format!("{:x}", digest.finalize())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn projected_fingerprint(
    before: &BTreeMap<String, String>,
    accepted: &AcceptedMission,
    source: &TypedDraft,
    rendered: &RenderedRoadmap,
    lifecycle: &MissionLifecycle,
) -> Result<BTreeMap<String, String>, MissionError> {
    let mut after = before.clone();
    after.retain(|path, _| {
        !path.starts_with("parts/") && !path.starts_with("generated/") && path != "mission.md"
    });
    let accepted_bytes = serde_json::to_vec_pretty(accepted)?;
    let source_bytes = serde_json::to_vec_pretty(&stored_source(source))?;
    let revision = accepted.bundle.manifest.revision;
    after.insert("accepted.json".into(), sha256(&accepted_bytes));
    after.insert(
        format!("history/revision-{revision}.json"),
        sha256(&accepted_bytes),
    );
    after.insert("source.json".into(), sha256(&source_bytes));
    after.insert(
        format!("history/source-revision-{revision}.json"),
        sha256(&source_bytes),
    );
    for waypoint in &source.waypoints {
        after.insert(
            format!("parts/{}.json", waypoint.waypoint_id),
            sha256(&serde_json::to_vec_pretty(&waypoint.part)?),
        );
    }
    for (name, bytes) in rendered.files() {
        after.insert(format!("generated/{name}"), sha256(bytes));
    }
    after.insert(
        "mission.md".into(),
        sha256(lifecycle.render_typed(source).as_bytes()),
    );
    Ok(after)
}

fn projected_rollback_fingerprint(
    before: &BTreeMap<String, String>,
    accepted: &AcceptedMission,
    source: Option<&TypedDraft>,
    rendered: &RenderedRoadmap,
    lifecycle: &MissionLifecycle,
) -> Result<BTreeMap<String, String>, MissionError> {
    let mut after = before.clone();
    after.retain(|path, _| {
        !path.starts_with("parts/")
            && !path.starts_with("generated/")
            && path != "mission.md"
            && path != "source.json"
    });
    let accepted_bytes = serde_json::to_vec_pretty(accepted)?;
    let revision = accepted.bundle.manifest.revision;
    after.insert("accepted.json".into(), sha256(&accepted_bytes));
    after.insert(
        format!("history/revision-{revision}.json"),
        sha256(&accepted_bytes),
    );
    if let Some(source) = source {
        let source_bytes = serde_json::to_vec_pretty(&stored_source(source))?;
        after.insert("source.json".into(), sha256(&source_bytes));
        after.insert(
            format!("history/source-revision-{revision}.json"),
            sha256(&source_bytes),
        );
        for waypoint in &source.waypoints {
            after.insert(
                format!("parts/{}.json", waypoint.waypoint_id),
                sha256(&serde_json::to_vec_pretty(&waypoint.part)?),
            );
        }
        after.insert(
            "mission.md".into(),
            sha256(lifecycle.render_typed(source).as_bytes()),
        );
    }
    for (name, bytes) in rendered.files() {
        after.insert(format!("generated/{name}"), sha256(bytes));
    }
    Ok(after)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), MissionError> {
    reject_symlink_ancestors(destination)?;
    if destination.exists() {
        return Err(repair_error(
            "repair-snapshot-exists",
            "refusing to overwrite an existing recovery snapshot.",
        ));
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let kind = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if kind.is_symlink() {
            return Err(repair_error(
                "repair-snapshot-symlink",
                "Mission payload contains a symlink.",
            ));
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            let mut input = File::open(entry.path())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
        } else {
            return Err(repair_error(
                "repair-snapshot-type-invalid",
                "Mission payload contains an unsupported filesystem entry.",
            ));
        }
    }
    File::open(destination)?.sync_all()?;
    if let Some(parent) = destination.parent() {
        sync_directory_chain(parent)?;
    }
    Ok(())
}

fn sync_directory_chain(directory: &Path) -> Result<(), MissionError> {
    let mut current = Some(directory);
    while let Some(path) = current {
        File::open(path)?.sync_all()?;
        current = path.parent();
    }
    Ok(())
}

fn load_source_from_directory(
    mission_dir: &Path,
    mission_id: Uuid,
) -> Result<TypedDraft, MissionError> {
    let source_path = mission_dir.join("source.json");
    let stored: StoredSource = serde_json::from_slice(&fs::read(source_path)?)?;
    if stored.mission_id != mission_id {
        return Err(repair_error(
            "repair-snapshot-identity-mismatch",
            "snapshot typed source identity differs from its Mission ID.",
        ));
    }
    let mut waypoints = Vec::with_capacity(stored.waypoints.len());
    for waypoint in stored.waypoints {
        if waypoint.part_file != format!("parts/{}.json", waypoint.waypoint_id) {
            return Err(repair_error(
                "repair-snapshot-part-path-invalid",
                "snapshot typed Part path is not API-managed.",
            ));
        }
        let part: PartSource =
            serde_json::from_slice(&fs::read(mission_dir.join(waypoint.part_file))?)?;
        waypoints.push(TypedWaypoint {
            waypoint_id: waypoint.waypoint_id,
            title: waypoint.title,
            prompt: waypoint.prompt,
            part,
            depends_on: waypoint.depends_on,
            validation: waypoint.validation,
            commit_checkpoint: waypoint.commit_checkpoint,
            non_goals: waypoint.non_goals,
            ownership_refs: waypoint.ownership_refs,
        });
    }
    Ok(TypedDraft {
        mission_id: stored.mission_id,
        schema_version: stored.schema_version,
        revision: stored.revision,
        title: stored.title,
        objective: stored.objective,
        waypoints,
    })
}

fn repair_error(code: &'static str, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "repair".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.into(),
        }],
    )
}

fn repair_diagnostic(error: &MissionError) -> String {
    match error {
        MissionError::Validation(_, diagnostics) => diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("; "),
        _ => error.to_string(),
    }
}

fn contains_symlink(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(_) => return true,
        }
    }
    false
}

fn reject_symlink_ancestors(path: &Path) -> Result<(), MissionError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(repair_error(
                    "repair-path-symlink",
                    "journal and snapshot paths must not contain symlinks.",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn repair_apply_stops_after_partial_failure_and_resume_finishes_remaining_records() {
        let workspace = TempDir::new().expect("workspace");
        let lifecycle = MissionLifecycle::open(workspace.path());
        let mission_ids = [
            divergent_mission(&lifecycle, "First repair record"),
            divergent_mission(&lifecycle, "Second repair record"),
            divergent_mission(&lifecycle, "Third repair record"),
        ];
        let (request, proposal_id) = approved_request(&lifecycle, workspace.path(), &mission_ids);
        let journal_id = Uuid::from_u128(0x8c2a_2504_6555_4ca3_9f22_1c8d_4f5a_7b90);
        let blocked_snapshot = lifecycle.repair_snapshot_path(journal_id, mission_ids[1]);
        fs::create_dir_all(&blocked_snapshot).expect("create conflicting snapshot");
        fs::write(blocked_snapshot.join("unowned"), b"not a recovery snapshot")
            .expect("write conflicting snapshot");

        let partial = lifecycle
            .repair_apply_with_journal_id(workspace.path(), request, &proposal_id, journal_id)
            .expect("partial repair response");
        let journal = partial.journal.as_ref().expect("durable partial journal");
        assert_eq!(partial.status, "partial");
        assert_eq!(journal.state, RepairJournalState::Partial);
        assert_eq!(journal.records[0].state, RepairRecordState::Applied);
        assert_eq!(journal.records[1].state, RepairRecordState::Failed);
        assert_eq!(journal.records[2].state, RepairRecordState::Pending);
        assert!(
            journal.records[1]
                .diagnostic
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("repair-snapshot-conflict"))
        );
        assert_eq!(
            lifecycle
                .get(mission_ids[0])
                .expect("first record")
                .bundle
                .manifest
                .revision,
            4
        );
        for mission_id in &mission_ids[1..] {
            assert_eq!(
                lifecycle
                    .get(*mission_id)
                    .expect("unchanged record")
                    .bundle
                    .manifest
                    .revision,
                3
            );
        }

        let external_edit = lifecycle
            .store
            .root
            .join(mission_ids[0].to_string())
            .join("external-edit.txt");
        fs::write(&external_edit, b"not owned by this journal").expect("write external edit");
        let drift = lifecycle
            .repair_resume(workspace.path(), journal_id)
            .expect_err("resume must refuse an edited applied record");
        assert!(matches!(
            drift,
            MissionError::Validation(_, diagnostics)
                if diagnostics[0].code == "repair-resume-drift"
        ));
        assert_eq!(
            lifecycle
                .get(mission_ids[1])
                .expect("second remains blocked")
                .bundle
                .manifest
                .revision,
            3
        );
        fs::remove_file(external_edit).expect("remove test-owned external edit");

        fs::remove_dir_all(&blocked_snapshot).expect("remove test-owned conflict");
        let resumed = lifecycle
            .repair_resume(workspace.path(), journal_id)
            .expect("resume repair");
        assert_eq!(resumed.status, "applied");
        let journal = resumed.journal.expect("completed journal");
        assert!(
            journal
                .records
                .iter()
                .all(|record| record.state == RepairRecordState::Applied)
        );
        for mission_id in mission_ids {
            assert_eq!(
                lifecycle
                    .get(mission_id)
                    .expect("repaired record")
                    .bundle
                    .manifest
                    .revision,
                4
            );
        }
    }

    #[test]
    fn repair_rollback_records_an_unchanged_failed_record_as_durable_compensation() {
        let workspace = TempDir::new().expect("workspace");
        let lifecycle = MissionLifecycle::open(workspace.path());
        let mission_id = divergent_mission(&lifecycle, "Unchanged failed repair record");
        let (request, proposal_id) = approved_request(&lifecycle, workspace.path(), &[mission_id]);
        let preview = lifecycle
            .repair_preview(workspace.path(), request)
            .expect("approved preview");
        let candidate = preview
            .candidate_overlay
            .get(&mission_id.to_string())
            .cloned()
            .expect("repair candidate");
        let before_payload_fingerprint = preview.records[0]
            .payload_fingerprint
            .clone()
            .expect("before payload fingerprint");
        let before_revision = preview.records[0]
            .observed_revision
            .expect("before revision");
        let journal_id = Uuid::from_u128(0x7a2b_2504_6555_4ca3_9f22_1c8d_4f5a_7b91);
        let mission_dir = lifecycle.store.root.join(mission_id.to_string());
        let snapshot = lifecycle.repair_snapshot_path(journal_id, mission_id);
        copy_tree(&mission_dir, &snapshot).expect("create durable before-state snapshot");
        let journal = RepairJournal {
            journal_id,
            source_root: workspace.path().to_path_buf(),
            proposal_id,
            state: RepairJournalState::Partial,
            records: vec![RepairJournalRecord {
                mission_id,
                before_revision,
                after_revision: Some(candidate.predicted_revision),
                before_payload_fingerprint: before_payload_fingerprint.clone(),
                after_payload_fingerprint: Some(candidate.predicted_payload_fingerprint.clone()),
                rollback_revision: None,
                rollback_payload_fingerprint: None,
                snapshot: Some(snapshot),
                state: RepairRecordState::Failed,
                diagnostic: Some("forced failure before authority write".into()),
                candidate: Some(candidate),
            }],
        };
        lifecycle
            .persist_repair_journal(&journal, true)
            .expect("persist failed repair journal");
        let before_payload = fingerprint_files(&mission_dir).expect("before payload");

        let rolled_back = lifecycle
            .repair_rollback(workspace.path(), journal_id)
            .expect("rollback unchanged record");
        assert_eq!(rolled_back.status, "rolled-back");
        assert_eq!(
            fingerprint_files(&mission_dir).expect("unchanged payload"),
            before_payload
        );
        let record = &rolled_back
            .journal
            .expect("durable rollback journal")
            .records[0];
        assert_eq!(record.state, RepairRecordState::RolledBack);
        assert_eq!(
            record.rollback_payload_fingerprint.as_deref(),
            Some(before_payload_fingerprint.as_str())
        );

        let repeated = lifecycle
            .repair_rollback(workspace.path(), journal_id)
            .expect("repeat rollback read-back");
        assert_eq!(repeated.status, "rolled-back");
    }

    #[test]
    fn concurrent_repair_resumes_preserve_durable_snapshot_progress() {
        use std::sync::{Arc, Barrier};

        let workspace = TempDir::new().expect("workspace");
        let lifecycle = MissionLifecycle::open(workspace.path());
        let mission_id = divergent_mission(&lifecycle, "Concurrent repair resume");
        let mission_dir = lifecycle.store.root.join(mission_id.to_string());
        fs::write(
            mission_dir.join("payload-padding.bin"),
            vec![b'x'; 4 * 1024 * 1024],
        )
        .expect("write payload to overlap concurrent resumes");
        let (request, proposal_id) = approved_request(&lifecycle, workspace.path(), &[mission_id]);
        let preview = lifecycle
            .repair_preview(workspace.path(), request)
            .expect("approved preview");
        let candidate = preview
            .candidate_overlay
            .get(&mission_id.to_string())
            .cloned()
            .expect("repair candidate");
        let before = &preview.records[0];
        let journal_id = Uuid::from_u128(0x9b3c_2504_6555_4ca3_9f22_1c8d_4f5a_7b92);
        let journal = RepairJournal {
            journal_id,
            source_root: workspace.path().to_path_buf(),
            proposal_id,
            state: RepairJournalState::Partial,
            records: vec![RepairJournalRecord {
                mission_id,
                before_revision: before.observed_revision.expect("before revision"),
                after_revision: Some(candidate.predicted_revision),
                before_payload_fingerprint: before
                    .payload_fingerprint
                    .clone()
                    .expect("before payload fingerprint"),
                after_payload_fingerprint: Some(candidate.predicted_payload_fingerprint.clone()),
                rollback_revision: None,
                rollback_payload_fingerprint: None,
                snapshot: None,
                state: RepairRecordState::Pending,
                diagnostic: None,
                candidate: Some(candidate),
            }],
        };
        lifecycle
            .persist_repair_journal(&journal, true)
            .expect("persist pending repair journal");
        let lock_path = lifecycle
            .repair_journal_path(journal_id)
            .with_file_name(format!(".{journal_id}.lock"));
        fs::write(lock_path, b"").expect("leave the stable lock file from a prior process");

        let barrier = Arc::new(Barrier::new(3));
        let root = workspace.path().to_path_buf();
        let workers = (0..2)
            .map(|_| {
                let barrier = Arc::clone(&barrier);
                let root = root.clone();
                std::thread::spawn(move || {
                    let lifecycle = MissionLifecycle::open(&root);
                    barrier.wait();
                    lifecycle.repair_resume(&root, journal_id)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        for worker in workers {
            let response = worker
                .join()
                .expect("resume worker")
                .expect("concurrent resume response");
            assert_eq!(response.status, "applied");
        }

        let journal = lifecycle
            .repair_journal(workspace.path(), journal_id)
            .expect("read completed journal");
        assert_eq!(journal.state, RepairJournalState::Applied);
        let record = &journal.records[0];
        assert_eq!(record.state, RepairRecordState::Applied);
        let snapshot = record.snapshot.as_ref().expect("durable snapshot pointer");
        assert!(snapshot.is_dir(), "snapshot pointer remains recoverable");
        assert_eq!(
            payload_fingerprint(&fingerprint_files(&mission_dir).expect("repaired payload")),
            record
                .after_payload_fingerprint
                .as_deref()
                .expect("after fingerprint")
        );
    }

    fn divergent_mission(lifecycle: &MissionLifecycle, title: &str) -> Uuid {
        let draft = lifecycle
            .create_draft(title.to_owned(), "Verify partial repair recovery.".into())
            .expect("create typed draft");
        let mission_id = draft.mission_id;
        let draft = TypedDraft {
            waypoints: vec![TypedWaypoint {
                waypoint_id: "repair".into(),
                title: "Repair record".into(),
                prompt: "Apply one durable repair.".into(),
                part: PartSource {
                    waypoint_id: "repair".into(),
                    document: Document {
                        blocks: vec![Block::Paragraph {
                            inlines: vec![Inline::Text {
                                text: "Typed repair fixture.".into(),
                            }],
                        }],
                    },
                },
                validation: vec![ValidationGate {
                    command: "cargo test".into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: "Record repair fixture.".into(),
                },
                ..TypedWaypoint::default()
            }],
            ..draft
        };
        let draft = lifecycle
            .save_draft(mission_id, draft, 1)
            .expect("save typed source");
        lifecycle
            .publish_draft(mission_id, draft, 2)
            .expect("publish typed source");

        let source_path = lifecycle
            .store
            .root
            .join(mission_id.to_string())
            .join("source.json");
        let mut source: serde_json::Value =
            serde_json::from_slice(&fs::read(&source_path).expect("read typed source"))
                .expect("parse typed source");
        source["revision"] = json!(2);
        fs::write(
            source_path,
            serde_json::to_vec_pretty(&source).expect("serialize divergent source"),
        )
        .expect("create controlled revision divergence");
        mission_id
    }

    fn approved_request(
        lifecycle: &MissionLifecycle,
        workspace: &Path,
        mission_ids: &[Uuid],
    ) -> (RepairRequest, String) {
        let request = RepairRequest {
            source_root: workspace.to_path_buf(),
            policy: REPAIR_POLICY.into(),
            records: mission_ids
                .iter()
                .map(|mission_id| RepairRecordRequest {
                    mission_id: *mission_id,
                    expected_revision: lifecycle
                        .get(*mission_id)
                        .expect("accepted record")
                        .bundle
                        .manifest
                        .revision,
                    accepted_sha256: String::new(),
                    source_sha256: String::new(),
                    payload_fingerprint: String::new(),
                })
                .collect(),
        };
        let discovery = lifecycle
            .repair_preview(workspace, request.clone())
            .expect("read-only preview");
        let mut approved = request;
        for (selection, observed) in approved.records.iter_mut().zip(discovery.records) {
            selection.expected_revision = observed.observed_revision.expect("observed revision");
            selection.accepted_sha256 = observed.observed_accepted_sha256.expect("accepted hash");
            selection.source_sha256 = observed.observed_source_sha256.expect("source hash");
            selection.payload_fingerprint =
                observed.payload_fingerprint.expect("payload fingerprint");
        }
        let preview = lifecycle
            .repair_preview(workspace, approved.clone())
            .expect("approved preview");
        assert_eq!(preview.status, "ready");
        (
            approved,
            preview
                .planned_journal_identity
                .expect("planned journal identity"),
        )
    }
}
