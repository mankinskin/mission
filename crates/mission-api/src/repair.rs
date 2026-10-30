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

mod helpers;
mod record_changes;
mod record_plan;
mod recovery;
mod request;

use helpers::*;

#[cfg(test)]
mod tests;
