//! Revision-bound requester approval at the trusted facade boundary.
//! Session owns host ingress; Mission owns the append-only review/grant journal.
use mission_api::{
    AcceptedMission, ApprovalTransaction, MissionError, MissionLifecycle, ReviewPlan, WaypointState,
};
use serde::{Deserialize, Serialize};
use session_api::{
    DecisionKind, RequesterChallengeRequest, RequesterDecisionReceipt, RequesterDecisionRef,
    SessionError, SessionStoreConfig, USER_PROMPT_SUBMIT, parse_decision_prompt,
};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalError {
    code: &'static str,
    message: String,
}
impl ApprovalError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn message(&self) -> &str {
        &self.message
    }
}
impl fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApprovalError {}
impl From<MissionError> for ApprovalError {
    fn from(error: MissionError) -> Self {
        let code = match error.diagnostics().and_then(|d| d.first()).map(|d| d.code) {
            Some("approval-record-exists") => "approval_conflict",
            _ => "approval_authority_unavailable",
        };
        Self::new(code, error.to_string())
    }
}
fn session_error(error: SessionError) -> ApprovalError {
    let code = match error.requester_code() {
        Some("requester_decision_conflict" | "requester_challenge_conflict") => "approval_conflict",
        Some("requester_decision_pending") => "approval_pending",
        _ => "approval_authority_unavailable",
    };
    ApprovalError::new(code, error.to_string())
}
fn required(message: &str) -> ApprovalError {
    ApprovalError::new("approval_required", message)
}
fn stale(message: &str) -> ApprovalError {
    ApprovalError::new("approval_stale", message)
}
fn conflict(message: &str) -> ApprovalError {
    ApprovalError::new("approval_conflict", message)
}
fn pending_error(message: &str) -> ApprovalError {
    ApprovalError::new("approval_pending", message)
}
fn unavailable(message: &str) -> ApprovalError {
    ApprovalError::new("approval_authority_unavailable", message)
}
fn corrupt(message: String) -> ApprovalError {
    ApprovalError::new("approval_journal_corrupt", message)
}
fn sha(input: &str) -> String {
    Sha256::digest(input.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewKind {
    ColdWalkthrough,
    Formal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Pass,
    Fail,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryPath {
    Activation,
    Dispatch,
    Resume,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStep {
    AfterChallengeJournal,
    BeforeGrant,
    AfterGrant,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewInput {
    pub kind: ReviewKind,
    pub verdict: ReviewVerdict,
    pub reviewer: String,
    pub revision: u64,
    pub plan_digest: String,
    pub evidence_identity: String,
    pub cold_review_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRecord {
    pub review_id: String,
    pub kind: ReviewKind,
    pub verdict: ReviewVerdict,
    pub reviewer: String,
    pub revision: u64,
    pub plan_digest: String,
    pub evidence_identity: String,
    pub cold_review_id: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChallengeBinding {
    challenge_id: String,
    session_id: String,
    revision: u64,
    plan_digest: String,
    evidence_identity: String,
    review_snapshot_digest: String,
    cold_review_id: String,
    formal_review_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalGrant {
    pub grant_id: String,
    pub decision: RequesterDecisionRef,
    pub challenge_id: String,
    pub revision: u64,
    pub plan_digest: String,
    pub review_snapshot_digest: String,
    pub cold_review_id: String,
    pub formal_review_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Disposition {
    disposition_id: String,
    decision: RequesterDecisionRef,
    challenge_id: String,
    kind: DecisionKind,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Entry {
    Review(ReviewRecord),
    Challenge(ChallengeBinding),
    Grant(ApprovalGrant),
    Decision(Disposition),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema_version: u32,
    mission_id: Uuid,
    seq: u64,
    entry: Entry,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedChallenge {
    pub challenge_id: String,
    pub revision: u64,
    pub plan_digest: String,
    pub review_snapshot_digest: String,
    pub approve_line: String,
    pub reject_line: String,
    pub replan_line: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedDecision {
    pub kind: DecisionKind,
    pub grant: Option<ApprovalGrant>,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingApproval {
    pub challenge_id: String,
    pub stage: String,
}
pub struct EntryGuard<'a> {
    _tx: ApprovalTransaction<'a>,
    pub grant: ApprovalGrant,
}
impl EntryGuard<'_> {
    pub fn record_progress(
        &self,
        waypoint_id: &str,
        state: WaypointState,
        expected: u64,
    ) -> Result<AcceptedMission, ApprovalError> {
        Ok(self._tx.record_progress(waypoint_id, state, expected)?)
    }
}

fn read_journal(tx: &ApprovalTransaction<'_>) -> Result<Vec<Record>, ApprovalError> {
    let mut out = Vec::new();
    for (index, name) in tx.list_records("journal")?.iter().enumerate() {
        let seq = index as u64 + 1;
        if *name != format!("{seq:08}.json") {
            return Err(corrupt(format!(
                "journal entry {name} breaks the contiguous sequence"
            )));
        }
        let bytes = tx
            .read_record(&format!("journal/{name}"))?
            .ok_or_else(|| corrupt(format!("journal entry {name} vanished")))?;
        let record: Record = serde_json::from_slice(&bytes)
            .map_err(|error| corrupt(format!("journal entry {name}: {error}")))?;
        if record.schema_version != SCHEMA_VERSION
            || record.mission_id != tx.mission_id()
            || record.seq != seq
        {
            return Err(corrupt(format!(
                "journal entry {name} does not match its identity"
            )));
        }
        out.push(record);
    }
    Ok(out)
}
fn append(
    tx: &ApprovalTransaction<'_>,
    journal: &mut Vec<Record>,
    entry: Entry,
) -> Result<(), ApprovalError> {
    let seq = journal.len() as u64 + 1;
    let record = Record {
        schema_version: SCHEMA_VERSION,
        mission_id: tx.mission_id(),
        seq,
        entry,
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| corrupt(error.to_string()))?;
    tx.write_record_new(&format!("journal/{seq:08}.json"), &bytes)?;
    journal.push(record);
    Ok(())
}
fn reviews(journal: &[Record]) -> Vec<&ReviewRecord> {
    journal
        .iter()
        .filter_map(|r| {
            if let Entry::Review(v) = &r.entry {
                Some(v)
            } else {
                None
            }
        })
        .collect()
}
fn current_reviews(journal: &[Record], plan: &ReviewPlan) -> Option<(ReviewRecord, ReviewRecord)> {
    let same = |r: &ReviewRecord| {
        r.revision == plan.revision
            && r.plan_digest == plan.plan_digest
            && r.evidence_identity == plan.evidence_identity
    };
    let all = reviews(journal);
    let cold = all
        .iter()
        .rev()
        .find(|r| r.kind == ReviewKind::ColdWalkthrough && same(r))?;
    if cold.verdict != ReviewVerdict::Pass {
        return None;
    }
    let formal = all
        .iter()
        .rev()
        .find(|r| r.kind == ReviewKind::Formal && same(r))?;
    (formal.verdict == ReviewVerdict::Pass
        && formal.cold_review_id.as_deref() == Some(cold.review_id.as_str())
        && cold.reviewer != formal.reviewer)
        .then(|| ((*cold).clone(), (*formal).clone()))
}
fn digest_from(
    id: Uuid,
    revision: u64,
    plan: &str,
    evidence: &str,
    cold: &str,
    formal: &str,
) -> String {
    sha(&format!(
        "mission-review-snapshot/1\n{id}\n{revision}\n{plan}\n{evidence}\n{cold}\n{formal}"
    ))
}
fn snapshot_digest(id: Uuid, plan: &ReviewPlan, cold: &str, formal: &str) -> String {
    digest_from(
        id,
        plan.revision,
        &plan.plan_digest,
        &plan.evidence_identity,
        cold,
        formal,
    )
}
fn disposition_id(prefix: &str, id: Uuid, r: &RequesterDecisionRef) -> String {
    let digest = sha(&format!(
        "mission-disposition/1\n{id}\n{}\n{}\n{}\n{}",
        r.session_id, r.challenge_id, r.decision_id, r.source_event_id
    ));
    format!("{prefix}-{}", &digest[..32])
}

pub struct MissionApproval<'a> {
    workspace_root: PathBuf,
    lifecycle: MissionLifecycle,
    session: &'a SessionStoreConfig,
}
impl<'a> MissionApproval<'a> {
    pub fn open(workspace_root: &Path, session: &'a SessionStoreConfig) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
            lifecycle: MissionLifecycle::open(workspace_root),
            session,
        }
    }
    fn begin(&self, id: Uuid) -> Result<ApprovalTransaction<'_>, ApprovalError> {
        Ok(self.lifecycle.begin_approval_transaction(id)?)
    }
}

mod challenge;
mod decision;
mod grants;
mod review;
