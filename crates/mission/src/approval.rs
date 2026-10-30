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
    pub fn record_review(
        &self,
        id: Uuid,
        input: ReviewInput,
    ) -> Result<ReviewRecord, ApprovalError> {
        let tx = self.begin(id)?;
        let plan = tx.review_plan()?;
        if input.revision != plan.revision
            || input.plan_digest != plan.plan_digest
            || input.evidence_identity != plan.evidence_identity
        {
            return Err(stale("the review snapshot is no longer current"));
        }
        if input.reviewer.trim().is_empty() {
            return Err(required("a named reviewer is required"));
        }
        let mut journal = read_journal(&tx)?;
        match input.kind {
            ReviewKind::ColdWalkthrough if input.cold_review_id.is_some() => {
                return Err(required(
                    "a cold walkthrough cannot reference another review",
                ));
            }
            ReviewKind::Formal => {
                let cold_id = input.cold_review_id.as_ref().ok_or_else(|| {
                    required("a formal review must reference its cold walkthrough")
                })?;
                let cold = reviews(&journal)
                    .into_iter()
                    .find(|r| r.review_id == *cold_id)
                    .filter(|r| {
                        r.kind == ReviewKind::ColdWalkthrough
                            && r.verdict == ReviewVerdict::Pass
                            && r.revision == plan.revision
                            && r.plan_digest == plan.plan_digest
                            && r.evidence_identity == plan.evidence_identity
                    })
                    .ok_or_else(|| {
                        required("no passing cold walkthrough of this snapshot exists")
                    })?;
                if cold.reviewer == input.reviewer {
                    return Err(ApprovalError::new(
                        "approval_review_not_distinct",
                        "the formal reviewer must differ from the cold reviewer",
                    ));
                }
            }
            _ => {}
        }
        let record = ReviewRecord {
            review_id: format!("review-{:08}", journal.len() + 1),
            kind: input.kind,
            verdict: input.verdict,
            reviewer: input.reviewer,
            revision: plan.revision,
            plan_digest: plan.plan_digest,
            evidence_identity: plan.evidence_identity,
            cold_review_id: input.cold_review_id,
        };
        append(&tx, &mut journal, Entry::Review(record.clone()))?;
        Ok(record)
    }
    pub fn reviews(&self, id: Uuid) -> Result<Vec<ReviewRecord>, ApprovalError> {
        let tx = self.begin(id)?;
        Ok(reviews(&read_journal(&tx)?).into_iter().cloned().collect())
    }
    pub fn prepare_challenge(
        &self,
        id: Uuid,
        session_id: &str,
        challenge_id: &str,
    ) -> Result<PreparedChallenge, ApprovalError> {
        self.prepare_challenge_with_fault(id, session_id, challenge_id, None)
    }
    pub fn prepare_challenge_with_fault(
        &self,
        id: Uuid,
        session_id: &str,
        challenge_id: &str,
        fault: Option<ApprovalStep>,
    ) -> Result<PreparedChallenge, ApprovalError> {
        if !self.workspace_root.is_absolute() {
            return Err(unavailable("the Mission root must be absolute"));
        }
        let tx = self.begin(id)?;
        let plan = tx.review_plan()?;
        let mut journal = read_journal(&tx)?;
        let (cold, formal) = current_reviews(&journal, &plan).ok_or_else(|| {
            required("passing current cold and distinct formal reviews are required")
        })?;
        let binding = ChallengeBinding {
            challenge_id: challenge_id.into(),
            session_id: session_id.into(),
            revision: plan.revision,
            plan_digest: plan.plan_digest.clone(),
            evidence_identity: plan.evidence_identity.clone(),
            review_snapshot_digest: snapshot_digest(id, &plan, &cold.review_id, &formal.review_id),
            cold_review_id: cold.review_id,
            formal_review_id: formal.review_id,
        };
        let existing = journal.iter().find_map(|r| match &r.entry {
            Entry::Challenge(c) if c.challenge_id == challenge_id => Some(c.clone()),
            _ => None,
        });
        match existing {
            Some(existing) if existing == binding => {}
            Some(_) => return Err(conflict("the challenge id is bound to another snapshot")),
            None => {
                append(&tx, &mut journal, Entry::Challenge(binding.clone()))?;
                if fault == Some(ApprovalStep::AfterChallengeJournal) {
                    return Err(pending_error(
                        "challenge journaled but not registered with Session",
                    ));
                }
            }
        }
        let challenge = self
            .session
            .register_requester_challenge(&RequesterChallengeRequest {
                session_id: session_id.into(),
                challenge_id: challenge_id.into(),
                mission_id: id,
                mission_root: self.workspace_root.clone(),
                revision: binding.revision,
                plan_digest: binding.plan_digest.clone(),
                review_snapshot_digest: binding.review_snapshot_digest.clone(),
            })
            .map_err(session_error)?;
        Ok(PreparedChallenge {
            challenge_id: challenge_id.into(),
            revision: binding.revision,
            plan_digest: binding.plan_digest,
            review_snapshot_digest: binding.review_snapshot_digest,
            approve_line: self
                .session
                .render_decision_prompt(&challenge, DecisionKind::Approve),
            reject_line: self
                .session
                .render_decision_prompt(&challenge, DecisionKind::Reject),
            replan_line: self
                .session
                .render_decision_prompt(&challenge, DecisionKind::Replan),
        })
    }
    fn verify_receipt(
        &self,
        id: Uuid,
        reference: &RequesterDecisionRef,
    ) -> Result<RequesterDecisionReceipt, ApprovalError> {
        let receipt = self
            .session
            .read_requester_decision(reference)
            .map_err(session_error)?;
        if receipt.reference != *reference || receipt.mission_id != id {
            return Err(conflict("receipt reference or Mission mismatch"));
        }
        if !receipt.ingress.trusted_local_host
            || receipt.ingress.hook_event_name != USER_PROMPT_SUBMIT
        {
            return Err(unavailable(
                "receipt lacks trusted host UserPromptSubmit ingress",
            ));
        }
        let raw = receipt.read_raw_prompt().map_err(session_error)?;
        let parsed = parse_decision_prompt(&raw)
            .ok_or_else(|| unavailable("raw prompt is not a standalone decision"))?;
        if parsed.kind != receipt.kind
            || parsed.mission_id != receipt.mission_id
            || parsed.revision != receipt.revision
            || parsed.plan_digest != receipt.plan_digest
            || parsed.review_snapshot_digest != receipt.review_snapshot_digest
            || parsed.challenge_id != receipt.reference.challenge_id
        {
            return Err(unavailable("raw prompt disagrees with receipt"));
        }
        Ok(receipt)
    }
    pub fn accept_decision(
        &self,
        id: Uuid,
        reference: &RequesterDecisionRef,
    ) -> Result<AcceptedDecision, ApprovalError> {
        self.accept_decision_with_fault(id, reference, None)
    }
    pub fn accept_decision_with_fault(
        &self,
        id: Uuid,
        reference: &RequesterDecisionRef,
        fault: Option<ApprovalStep>,
    ) -> Result<AcceptedDecision, ApprovalError> {
        let tx = self.begin(id)?;
        let receipt = self.verify_receipt(id, reference)?;
        let mut journal = read_journal(&tx)?;
        let record = journal
            .iter()
            .find_map(|r| match &r.entry {
                Entry::Challenge(c) if c.challenge_id == reference.challenge_id => Some(c.clone()),
                _ => None,
            })
            .ok_or_else(|| unavailable("challenge was not prepared by this Mission"))?;
        let challenge = self
            .session
            .read_requester_challenge(&reference.session_id, &reference.challenge_id)
            .map_err(session_error)?;
        let request = &challenge.request;
        if record.session_id != reference.session_id
            || request.session_id != record.session_id
            || request.mission_id != id
            || request.mission_root != self.workspace_root
            || request.revision != record.revision
            || request.plan_digest != record.plan_digest
            || request.review_snapshot_digest != record.review_snapshot_digest
            || receipt.revision != record.revision
            || receipt.plan_digest != record.plan_digest
            || receipt.review_snapshot_digest != record.review_snapshot_digest
        {
            return Err(conflict("receipt and Session/Mission challenges disagree"));
        }
        let approve = receipt.kind == DecisionKind::Approve;
        let disposition = disposition_id(if approve { "g" } else { "d" }, id, reference);
        let grant_now = ApprovalGrant {
            grant_id: disposition.clone(),
            decision: reference.clone(),
            challenge_id: record.challenge_id.clone(),
            revision: record.revision,
            plan_digest: record.plan_digest.clone(),
            review_snapshot_digest: record.review_snapshot_digest.clone(),
            cold_review_id: record.cold_review_id.clone(),
            formal_review_id: record.formal_review_id.clone(),
        };
        let decision_now = Disposition {
            disposition_id: disposition.clone(),
            decision: reference.clone(),
            challenge_id: record.challenge_id.clone(),
            kind: receipt.kind,
        };
        let prior_grant = journal.iter().find_map(|r| match &r.entry {
            Entry::Grant(g) if g.challenge_id == record.challenge_id => Some(g.clone()),
            _ => None,
        });
        let prior_decision = journal.iter().find_map(|r| match &r.entry {
            Entry::Decision(d) if d.challenge_id == record.challenge_id => Some(d.clone()),
            _ => None,
        });
        if prior_grant.as_ref().is_some_and(|g| *g != grant_now)
            || prior_decision.as_ref().is_some_and(|d| *d != decision_now)
            || (prior_grant.is_some() && !approve)
            || (prior_decision.is_some() && approve)
        {
            return Err(conflict(
                "challenge already has a divergent accepted decision",
            ));
        }
        let fresh = prior_grant.is_none() && prior_decision.is_none();
        if fresh {
            let plan = tx.review_plan()?;
            if plan.revision != record.revision
                || plan.plan_digest != record.plan_digest
                || plan.evidence_identity != record.evidence_identity
            {
                return Err(stale("plan changed after the challenge"));
            }
            match current_reviews(&journal, &plan) {
                Some((cold, formal))
                    if cold.review_id == record.cold_review_id
                        && formal.review_id == record.formal_review_id => {}
                Some(_) => return Err(stale("supporting reviews were superseded")),
                None => return Err(required("supporting reviews no longer pass")),
            }
            if self
                .session
                .requester_session_finished(&reference.session_id)
                .map_err(session_error)?
            {
                return Err(unavailable(
                    "finished Session decisions are historical only",
                ));
            }
            if fault == Some(ApprovalStep::BeforeGrant) {
                return Err(pending_error("decision verified without disposition"));
            }
            append(
                &tx,
                &mut journal,
                if approve {
                    Entry::Grant(grant_now.clone())
                } else {
                    Entry::Decision(decision_now)
                },
            )?;
            if fault == Some(ApprovalStep::AfterGrant) {
                return Err(pending_error("disposition recorded but unconsumed"));
            }
        }
        let existing = self
            .session
            .read_requester_consumption(&reference.session_id, &reference.challenge_id)
            .map_err(session_error)?;
        match &existing {
            Some(c) if c.reference != *reference || c.grant_id != disposition => {
                return Err(conflict("different disposition consumed challenge"));
            }
            Some(_) => {}
            None => {
                if approve
                    && !fresh
                    && (tx.review_plan()?.plan_digest != record.plan_digest
                        || tx.semantic_revision()? > record.revision)
                {
                    return Err(stale(
                        "plan changed after the grant was journaled; a fresh challenge is required",
                    ));
                }
                self.session
                    .consume_requester_challenge(reference, &disposition)
                    .map_err(session_error)?;
            }
        }
        Ok(AcceptedDecision {
            kind: receipt.kind,
            grant: approve.then_some(grant_now),
            replayed: !fresh && existing.is_some(),
        })
    }
    fn pending_stage(&self, journal: &[Record], c: &ChallengeBinding) -> Option<&'static str> {
        if self
            .session
            .read_requester_challenge(&c.session_id, &c.challenge_id)
            .is_err()
        {
            return Some("challenge-unregistered");
        }
        match self
            .session
            .find_requester_decision_for_challenge(&c.session_id, &c.challenge_id)
        {
            Ok(Some(_)) => {}
            Ok(None) => return None,
            Err(_) => return Some("session-unreadable"),
        }
        if !journal.iter().any(|r| match &r.entry {
            Entry::Grant(g) => g.challenge_id == c.challenge_id,
            Entry::Decision(d) => d.challenge_id == c.challenge_id,
            _ => false,
        }) {
            return Some("decision-unaccepted");
        }
        match self
            .session
            .read_requester_consumption(&c.session_id, &c.challenge_id)
        {
            Ok(Some(_)) => None,
            Ok(None) => Some("disposition-unconsumed"),
            Err(_) => Some("session-unreadable"),
        }
    }
    pub fn pending(&self, id: Uuid) -> Result<Vec<PendingApproval>, ApprovalError> {
        let tx = self.begin(id)?;
        let journal = read_journal(&tx)?;
        Ok(journal
            .iter()
            .filter_map(|r| match &r.entry {
                Entry::Challenge(c) => {
                    self.pending_stage(&journal, c)
                        .map(|stage| PendingApproval {
                            challenge_id: c.challenge_id.clone(),
                            stage: stage.into(),
                        })
                }
                _ => None,
            })
            .collect())
    }
    pub fn grants(&self, id: Uuid) -> Result<Vec<ApprovalGrant>, ApprovalError> {
        let tx = self.begin(id)?;
        let mut out = Vec::new();
        for record in read_journal(&tx)? {
            if let Entry::Grant(g) = record.entry {
                let consumed = self
                    .session
                    .read_requester_consumption(&g.decision.session_id, &g.challenge_id)
                    .map_err(session_error)?;
                if consumed.is_some_and(|c| c.reference == g.decision && c.grant_id == g.grant_id) {
                    out.push(g);
                }
            }
        }
        Ok(out)
    }
    fn effective_grant(
        &self,
        tx: &ApprovalTransaction<'_>,
    ) -> Result<ApprovalGrant, ApprovalError> {
        let plan = tx.review_plan()?;
        let journal = read_journal(tx)?;
        let (index, grant) = journal
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, r)| match &r.entry {
                Entry::Grant(g) => Some((i, g.clone())),
                _ => None,
            })
            .ok_or_else(|| required("no requester approval grant exists"))?;
        for later in &journal[index + 1..] {
            match &later.entry {
                Entry::Decision(_) => return Err(required("later reject/replan superseded grant")),
                Entry::Review(r)
                    if r.verdict == ReviewVerdict::Fail && r.plan_digest == grant.plan_digest =>
                {
                    return Err(required("review failed after grant"));
                }
                _ => {}
            }
        }
        if plan.plan_digest != grant.plan_digest || tx.semantic_revision()? > grant.revision {
            return Err(stale("semantic plan changed; fresh approval required"));
        }
        match self
            .session
            .read_requester_consumption(&grant.decision.session_id, &grant.challenge_id)
            .map_err(session_error)?
        {
            Some(c) if c.reference == grant.decision && c.grant_id == grant.grant_id => {}
            Some(_) => return Err(conflict("Session consumption names another grant")),
            None => return Err(pending_error("grant not consumed in Session")),
        }
        if self
            .session
            .requester_session_finished(&grant.decision.session_id)
            .map_err(session_error)?
        {
            return Err(required(
                "granting Session finished; handoff requires a fresh challenge",
            ));
        }
        self.verify_chain(tx.mission_id(), &journal, index, &grant)?;
        let receipt = self.verify_receipt(tx.mission_id(), &grant.decision)?;
        if receipt.kind != DecisionKind::Approve
            || receipt.revision != grant.revision
            || receipt.plan_digest != grant.plan_digest
            || receipt.review_snapshot_digest != grant.review_snapshot_digest
        {
            return Err(conflict("reread receipt disagrees with grant"));
        }
        Ok(grant)
    }
    fn verify_chain(
        &self,
        id: Uuid,
        journal: &[Record],
        index: usize,
        grant: &ApprovalGrant,
    ) -> Result<(), ApprovalError> {
        let broken = |what: &str| corrupt(format!("grant {} chain: {what}", grant.grant_id));
        if grant.challenge_id != grant.decision.challenge_id
            || grant.grant_id != disposition_id("g", id, &grant.decision)
        {
            return Err(broken("grant identity disagrees with decision reference"));
        }
        let (ci, binding) = journal[..index]
            .iter()
            .enumerate()
            .find_map(|(i, r)| match &r.entry {
                Entry::Challenge(c) if c.challenge_id == grant.challenge_id => Some((i, c)),
                _ => None,
            })
            .ok_or_else(|| broken("no preceding challenge"))?;
        let review = |rid: &str| {
            journal[..ci].iter().find_map(|r| match &r.entry {
                Entry::Review(v) if v.review_id == rid => Some(v),
                _ => None,
            })
        };
        let cold = review(&binding.cold_review_id)
            .ok_or_else(|| broken("missing preceding cold review"))?;
        let formal = review(&binding.formal_review_id)
            .ok_or_else(|| broken("missing preceding formal review"))?;
        if cold.kind != ReviewKind::ColdWalkthrough
            || formal.kind != ReviewKind::Formal
            || cold.verdict != ReviewVerdict::Pass
            || formal.verdict != ReviewVerdict::Pass
            || formal.cold_review_id.as_deref() != Some(cold.review_id.as_str())
            || cold.reviewer == formal.reviewer
            || [cold, formal].iter().any(|r| {
                r.revision != binding.revision
                    || r.plan_digest != binding.plan_digest
                    || r.evidence_identity != binding.evidence_identity
            })
        {
            return Err(broken("supporting reviews are not a distinct passing pair"));
        }
        if binding.session_id != grant.decision.session_id
            || binding.revision != grant.revision
            || binding.plan_digest != grant.plan_digest
            || binding.review_snapshot_digest != grant.review_snapshot_digest
            || binding.cold_review_id != grant.cold_review_id
            || binding.formal_review_id != grant.formal_review_id
            || digest_from(
                id,
                binding.revision,
                &binding.plan_digest,
                &binding.evidence_identity,
                &binding.cold_review_id,
                &binding.formal_review_id,
            ) != grant.review_snapshot_digest
        {
            return Err(broken("grant, challenge and recomputed snapshot disagree"));
        }
        let challenge = self
            .session
            .read_requester_challenge(&grant.decision.session_id, &grant.challenge_id)
            .map_err(session_error)?;
        let request = &challenge.request;
        if request.mission_id != id
            || request.mission_root != self.workspace_root
            || request.session_id != binding.session_id
            || request.revision != binding.revision
            || request.plan_digest != binding.plan_digest
            || request.review_snapshot_digest != binding.review_snapshot_digest
        {
            return Err(conflict(
                "Session challenge disagrees with Mission journal/root",
            ));
        }
        Ok(())
    }
    pub fn check_entry(&self, id: Uuid, path: EntryPath) -> Result<ApprovalGrant, ApprovalError> {
        self.guard_entry(id, path).map(|guard| guard.grant.clone())
    }
    pub fn guard_entry(&self, id: Uuid, _path: EntryPath) -> Result<EntryGuard<'_>, ApprovalError> {
        let tx = self.begin(id)?;
        let grant = self.effective_grant(&tx)?;
        Ok(EntryGuard { _tx: tx, grant })
    }
}
