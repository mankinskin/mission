//! Revision-safe planning and deterministic review projections.
//! Operator evidence is bound to a semantic plan, never requester approval.

use super::*;
use sha2::{Digest, Sha256};
use std::cell::RefCell;

pub(super) const REVIEW_PLAN_FILE: &str = "review-plan.json";
pub(super) const EVIDENCE_FILE: &str = "evidence-bindings.json";
const REQUEST_DIR: &str = "plan-requests";
const MAX_REQUEST_ID: usize = 128;
const SNAPSHOT_RETRIES: u32 = 8;

/// A requirement and its owning waypoints; ownership is replaced wholesale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequirement {
    pub requirement: Requirement,
    #[serde(default)]
    pub owner_waypoint_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanOperation {
    SetGoal {
        title: String,
        objective: String,
    },
    CreateWaypoint {
        waypoint: TypedWaypoint,
        #[serde(default)]
        after: Option<String>,
    },
    UpdateWaypoint {
        waypoint: TypedWaypoint,
    },
    /// Referenced work requires a replacement, which inherits requirement ownership.
    RemoveWaypoint {
        waypoint_id: String,
        #[serde(default)]
        replacement: Option<TypedWaypoint>,
    },
    ReorderWaypoints {
        order: Vec<String>,
    },
    SetRequirements {
        requirements: Vec<PlanRequirement>,
    },
    /// Append operator evidence bound to the current semantic plan.
    RecordEvidence {
        plan_digest: String,
        result: OperatorResult,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanOperationRequest {
    pub request_id: String,
    pub expected_revision: u64,
    pub operation: PlanOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanOperationOutcome {
    pub revision: u64,
    pub plan_digest: String,
    #[serde(default)]
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvidence {
    pub outcome: String,
    pub operator_command: String,
    pub evidence_pointer: String,
    /// None for evidence recorded outside plan operations.
    pub bound_plan_digest: Option<String>,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewWaypoint {
    pub position: usize,
    pub waypoint_id: String,
    pub title: String,
    pub prompt: String,
    pub part: Document,
    pub commit_checkpoint: CommitCheckpoint,
    pub depends_on: Vec<String>,
    pub non_goals: Vec<String>,
    pub references: Vec<String>,
    pub requirement_ids: Vec<String>,
    pub validation: Vec<ValidationGate>,
    /// Declared gates are never executed by planning.
    pub validation_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_plan: Option<WaypointOutcomePlan>,
    /// Evidenced means bound operator evidence, not formal review.
    pub review_status: String,
    pub evidence: Vec<ReviewEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPlan {
    pub mission_id: Uuid,
    pub schema_version: u32,
    pub revision: u64,
    pub title: String,
    pub objective: String,
    pub requirements: Vec<Requirement>,
    pub waypoints: Vec<ReviewWaypoint>,
    /// Semantic plan identity, excluding Mission ID, revision and evidence.
    pub plan_digest: String,
    pub evidence_identity: String,
    /// Requester review is never implied by this projection.
    pub formal_review: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceLedger {
    #[serde(default)]
    pub(super) entries: Vec<EvidenceBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) nonsemantic_revisions: Vec<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceBinding {
    pub(super) result_sha256: String,
    pub(super) plan_digest: String,
    pub(super) revision: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestRecord {
    schema_version: u32,
    mission_id: Uuid,
    request_id: String,
    request_sha256: String,
    revision: u64,
    plan_digest: String,
}

#[derive(Default)]
pub(super) struct PlanWrite<'a> {
    pub(super) adjust: Option<&'a dyn Fn(&mut MissionBundle) -> Result<(), MissionError>>,
    #[allow(clippy::type_complexity)]
    pub(super) extras:
        Option<&'a dyn Fn(&AcceptedMission) -> Result<Vec<(String, Vec<u8>)>, MissionError>>,
}

#[derive(Serialize)]
struct DigestInput<'a> {
    schema_version: u32,
    title: &'a str,
    objective: &'a str,
    requirements: &'a [Requirement],
    waypoints: Vec<DigestWaypoint<'a>>,
}

#[derive(Serialize)]
struct DigestWaypoint<'a> {
    waypoint_id: &'a str,
    title: &'a str,
    prompt: &'a str,
    part: &'a Document,
    depends_on: &'a [String],
    validation: &'a [ValidationGate],
    commit_checkpoint: &'a CommitCheckpoint,
    non_goals: &'a [String],
    requirement_ids: &'a [String],
    references: &'a [String],
    outcome_plan: Option<&'a WaypointOutcomePlan>,
}

fn plan_error(code: &'static str, message: &str, waypoint_id: Option<&str>) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "plan-operation".into(),
            pointer: String::new(),
            waypoint_id: waypoint_id.map(str::to_owned),
            message: message.into(),
        }],
    )
}

fn not_found(id: &str) -> MissionError {
    plan_error(
        "waypoint_not_found",
        "no waypoint with this stable ID exists in the Mission.",
        Some(id),
    )
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn index_of(draft: &TypedDraft, id: &str) -> Result<usize, MissionError> {
    draft
        .waypoints
        .iter()
        .position(|waypoint| waypoint.waypoint_id == id)
        .ok_or_else(|| not_found(id))
}

fn validate_request_id(request_id: &str) -> Result<(), MissionError> {
    if request_id.is_empty()
        || request_id.len() > MAX_REQUEST_ID
        || !request_id.chars().all(|c| c.is_ascii_graphic())
    {
        return Err(plan_error(
            "invalid_request_id",
            "request_id must be 1..=128 visible ASCII characters.",
            None,
        ));
    }
    Ok(())
}

fn read_optional(directory: &Path, path: &Path) -> Result<Option<Vec<u8>>, MissionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            typed_source::ensure_safe_path(directory, path)?;
            Ok(Some(fs::read(path)?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn ledger_from_bytes(bytes: Option<&[u8]>) -> Result<EvidenceLedger, MissionError> {
    let Some(bytes) = bytes else {
        return Ok(EvidenceLedger::default());
    };
    let ledger: EvidenceLedger = serde_json::from_slice(bytes).map_err(|_| {
        plan_error(
            "invalid_evidence_ledger",
            "evidence-bindings.json must be strict JSON.",
            None,
        )
    })?;
    if ledger
        .entries
        .iter()
        .any(|entry| entry.result_sha256.len() != 64 || entry.plan_digest.len() != 64)
    {
        return Err(plan_error(
            "invalid_evidence_ledger",
            "evidence bindings must carry SHA-256 identities.",
            None,
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    if ledger.nonsemantic_revisions.iter().any(|revision| {
        *revision == 0
            || !seen.insert(*revision)
            || ledger
                .entries
                .iter()
                .any(|entry| entry.revision == *revision)
    }) {
        return Err(plan_error(
            "invalid_evidence_ledger",
            "nonsemantic revisions must be unique, positive and distinct from evidence revisions.",
            None,
        ));
    }
    Ok(ledger)
}

pub(super) fn read_ledger(directory: &Path) -> Result<EvidenceLedger, MissionError> {
    let bytes = read_optional(directory, &directory.join(EVIDENCE_FILE))?;
    ledger_from_bytes(bytes.as_deref())
}

pub(super) fn nonsemantic_ledger(
    directory: &Path,
    old: &AcceptedMission,
    old_source: &TypedDraft,
    next: &AcceptedMission,
    next_source: &TypedDraft,
) -> Result<Vec<(String, Vec<u8>)>, MissionError> {
    let mut ledger = read_ledger(directory)?;
    let current = old.bundle.manifest.revision;
    if ledger.entries.iter().any(|entry| entry.revision > current)
        || ledger.nonsemantic_revisions.iter().any(|r| *r > current)
    {
        return Err(plan_error(
            "projection_diverged",
            "evidence ledger classifies a revision beyond the current plan revision.",
            None,
        ));
    }
    let before = build_review_plan(old, old_source, &ledger)?.plan_digest;
    let after = build_review_plan(next, next_source, &ledger)?.plan_digest;
    if before != after {
        return Ok(Vec::new());
    }
    ledger
        .nonsemantic_revisions
        .push(next.bundle.manifest.revision);
    Ok(vec![(
        EVIDENCE_FILE.to_owned(),
        serde_json::to_vec_pretty(&ledger)?,
    )])
}

fn read_record(
    directory: &Path,
    mission_id: Uuid,
    request_id: &str,
) -> Result<Option<RequestRecord>, MissionError> {
    let path = directory
        .join(REQUEST_DIR)
        .join(format!("{}.json", sha(request_id.as_bytes())));
    let Some(bytes) = read_optional(directory, &path)? else {
        return Ok(None);
    };
    let record: RequestRecord = serde_json::from_slice(&bytes).map_err(|_| {
        plan_error(
            "invalid_request_record",
            "stored request record is not strict JSON.",
            None,
        )
    })?;
    if record.schema_version != 1
        || record.mission_id != mission_id
        || record.request_id != request_id
        || record.request_sha256.len() != 64
        || record.plan_digest.len() != 64
    {
        return Err(plan_error(
            "invalid_request_record",
            "stored request record does not match its identity.",
            None,
        ));
    }
    Ok(Some(record))
}

fn references(fragment: &WaypointFragment) -> Result<Vec<String>, MissionError> {
    let mut refs = Vec::new();
    refs.extend(fragment.ticket_ref.clone());
    refs.extend(fragment.child_mission_ref.clone());
    for id in &fragment.relationships.specification_ids {
        refs.push(format!("ce://default/spec/{id}"));
    }
    for criterion in &fragment.relationships.criteria {
        refs.push(format!("criterion:{}", serde_json::to_string(criterion)?));
    }
    for execution in &fragment.relationships.executions {
        refs.push(format!("execution:{}", execution.execution_id));
    }
    Ok(refs)
}

/// Pure deterministic projection of one authoritative revision.
pub(super) fn build_review_plan(
    accepted: &AcceptedMission,
    source: &TypedDraft,
    ledger: &EvidenceLedger,
) -> Result<ReviewPlan, MissionError> {
    let bundle = &accepted.bundle;
    let source_order: Vec<&str> = source
        .waypoints
        .iter()
        .map(|waypoint| waypoint.waypoint_id.as_str())
        .collect();
    let accepted_order: Vec<&str> = bundle
        .manifest
        .execution_order
        .iter()
        .map(String::as_str)
        .collect();
    if source_order != accepted_order || source.revision != bundle.manifest.revision {
        return Err(plan_error(
            "projection_diverged",
            "source and accepted revision or waypoint order differ.",
            None,
        ));
    }
    let mut pairs = Vec::new();
    for waypoint in &source.waypoints {
        let fragment = bundle
            .waypoints
            .iter()
            .find(|fragment| fragment.waypoint_id == waypoint.waypoint_id)
            .ok_or_else(|| not_found(&waypoint.waypoint_id))?;
        pairs.push((waypoint, fragment, references(fragment)?));
    }
    let plan_digest = sha(&serde_json::to_vec(&DigestInput {
        schema_version: source.schema_version,
        title: &source.title,
        objective: &source.objective,
        requirements: &bundle.manifest.requirements,
        waypoints: pairs
            .iter()
            .map(|(waypoint, fragment, refs)| DigestWaypoint {
                waypoint_id: &waypoint.waypoint_id,
                title: &waypoint.title,
                prompt: &waypoint.prompt,
                part: &waypoint.part.document,
                depends_on: &waypoint.depends_on,
                validation: &waypoint.validation,
                commit_checkpoint: &waypoint.commit_checkpoint,
                non_goals: &waypoint.non_goals,
                requirement_ids: &fragment.requirement_ids,
                references: refs,
                outcome_plan: waypoint.outcome_plan.as_ref(),
            })
            .collect(),
    })?);
    // Bind repeated results one-to-one in recording order.
    let mut used = vec![false; ledger.entries.len()];
    let mut items: Vec<(&OperatorResult, String, Option<String>)> = Vec::new();
    for result in &bundle.manifest.operator_results {
        let result_sha = sha(&serde_json::to_vec(result)?);
        let position = ledger
            .entries
            .iter()
            .enumerate()
            .position(|(index, entry)| !used[index] && entry.result_sha256 == result_sha);
        if let Some(index) = position {
            used[index] = true;
        }
        let bound = position.map(|index| ledger.entries[index].plan_digest.clone());
        items.push((result, result_sha, bound));
    }
    let evidence_identity = sha(&serde_json::to_vec(
        &items
            .iter()
            .map(|(_, result_sha, bound)| (result_sha, bound))
            .collect::<Vec<_>>(),
    )?);
    let waypoints = pairs
        .iter()
        .enumerate()
        .map(|(index, (waypoint, fragment, refs))| {
            let evidence: Vec<ReviewEvidence> = items
                .iter()
                .filter(|(result, _, _)| result.waypoint_id == waypoint.waypoint_id)
                .map(|(result, _, bound)| ReviewEvidence {
                    outcome: result.outcome.clone(),
                    operator_command: result.operator_command.clone(),
                    evidence_pointer: result.evidence_pointer.clone(),
                    bound_plan_digest: bound.clone(),
                    current: bound.as_deref() == Some(plan_digest.as_str()),
                })
                .collect();
            ReviewWaypoint {
                position: index + 1,
                waypoint_id: waypoint.waypoint_id.clone(),
                title: waypoint.title.clone(),
                prompt: waypoint.prompt.clone(),
                part: waypoint.part.document.clone(),
                commit_checkpoint: waypoint.commit_checkpoint.clone(),
                depends_on: waypoint.depends_on.clone(),
                non_goals: waypoint.non_goals.clone(),
                references: refs.clone(),
                requirement_ids: fragment.requirement_ids.clone(),
                validation: waypoint.validation.clone(),
                validation_status: "declared".into(),
                outcome_plan: waypoint.outcome_plan.clone(),
                review_status: if evidence.iter().any(|item| item.current) {
                    "evidenced".into()
                } else {
                    "pending".into()
                },
                evidence,
            }
        })
        .collect();
    Ok(ReviewPlan {
        mission_id: accepted.mission_id,
        schema_version: source.schema_version,
        revision: bundle.manifest.revision,
        title: source.title.clone(),
        objective: source.objective.clone(),
        requirements: bundle.manifest.requirements.clone(),
        waypoints,
        plan_digest,
        evidence_identity,
        formal_review: "not-recorded".into(),
    })
}

fn snapshot_retryable(error: &MissionError) -> bool {
    match error {
        MissionError::NotFound(_) => true,
        MissionError::Io(error) => error.kind() == std::io::ErrorKind::NotFound,
        MissionError::Validation(_, diagnostics) => diagnostics
            .iter()
            .any(|d| matches!(d.code, "authority-diverged" | "projection_diverged")),
        _ => false,
    }
}

impl MissionLifecycle {
    /// Read one coherent authoritative revision without writing or repairing files.
    pub fn review_plan(&self, mission_id: Uuid) -> Result<ReviewPlan, MissionError> {
        let mut attempt = 0;
        loop {
            match self.review_plan_once(mission_id) {
                Err(error) if attempt < SNAPSHOT_RETRIES && snapshot_retryable(&error) => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                other => return other,
            }
        }
    }

    fn review_plan_once(&self, mission_id: Uuid) -> Result<ReviewPlan, MissionError> {
        // Preserve NotFound for absent Missions; never read existing ones unguarded.
        self.store.get(mission_id)?;
        let _guard =
            typed_source::TypedLock::acquire_shared_existing(&self.store.root, mission_id)?;
        self.review_plan_unlocked(mission_id)
    }

    pub(crate) fn review_plan_unlocked(
        &self,
        mission_id: Uuid,
    ) -> Result<ReviewPlan, MissionError> {
        let first = self.store.get(mission_id)?;
        let source = self.get_mission_source(mission_id)?;
        let ledger = read_ledger(&self.mission_directory(mission_id))?;
        let second = self.store.get(mission_id)?;
        let revision = first.bundle.manifest.revision;
        if second.bundle.manifest.revision != revision
            || source.revision != revision
            || ledger.entries.iter().any(|entry| entry.revision > revision)
            || ledger.nonsemantic_revisions.iter().any(|r| *r > revision)
        {
            return Err(plan_error(
                "projection_diverged",
                "authority changed while the review plan was being read.",
                None,
            ));
        }
        build_review_plan(&first, &source, &ledger)
    }

    pub(crate) fn semantic_revision_unlocked(&self, mission_id: Uuid) -> Result<u64, MissionError> {
        let revision = self.store.get(mission_id)?.bundle.manifest.revision;
        let ledger = read_ledger(&self.mission_directory(mission_id))?;
        let evidence: std::collections::BTreeSet<u64> = ledger
            .entries
            .iter()
            .map(|entry| entry.revision)
            .chain(ledger.nonsemantic_revisions.iter().copied())
            .collect();
        Ok((1..=revision)
            .rev()
            .find(|candidate| !evidence.contains(candidate))
            .unwrap_or(revision))
    }

    /// Apply one CAS/idempotent mutation under one lock and one atomic publication.
    pub fn apply_plan_operation(
        &self,
        mission_id: Uuid,
        request: PlanOperationRequest,
    ) -> Result<PlanOperationOutcome, MissionError> {
        validate_request_id(&request.request_id)?;
        let request_sha = sha(&serde_json::to_vec(&(
            request.expected_revision,
            &request.operation,
        ))?);
        self.store.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.store.root, mission_id)?;
        let directory = self.mission_directory(mission_id);
        if let Some(record) = read_record(&directory, mission_id, &request.request_id)? {
            if record.request_sha256 != request_sha {
                return Err(plan_error(
                    "request_id_conflict",
                    "request_id was already used for different content.",
                    None,
                ));
            }
            return Ok(PlanOperationOutcome {
                revision: record.revision,
                plan_digest: record.plan_digest,
                replayed: true,
            });
        }
        let source = self.get_mission_source(mission_id)?;
        let accepted = self.store.get(mission_id)?;
        let actual = accepted.bundle.manifest.revision;
        if actual != request.expected_revision {
            return Err(MissionError::StaleRevision {
                expected: request.expected_revision,
                actual,
            });
        }
        let ledger = read_ledger(&directory)?;
        let mut draft = source.clone();
        #[allow(clippy::type_complexity)]
        let mut adjust: Option<
            Box<dyn Fn(&mut MissionBundle) -> Result<(), MissionError>>,
        > = None;
        let mut new_ledger = ledger.clone();
        match request.operation.clone() {
            PlanOperation::SetGoal { title, objective } => {
                if title.trim().is_empty() || objective.trim().is_empty() {
                    return Err(plan_error(
                        "invalid_goal",
                        "title and objective must be nonblank.",
                        None,
                    ));
                }
                draft.title = title;
                draft.objective = objective;
            }
            PlanOperation::CreateWaypoint { waypoint, after } => {
                let at = match after {
                    None => draft.waypoints.len(),
                    Some(id) => index_of(&draft, &id)? + 1,
                };
                draft.waypoints.insert(at, waypoint);
            }
            PlanOperation::UpdateWaypoint { waypoint } => {
                let at = index_of(&draft, &waypoint.waypoint_id)?;
                draft.waypoints[at] = waypoint;
            }
            PlanOperation::RemoveWaypoint {
                waypoint_id,
                replacement,
            } => {
                let at = index_of(&draft, &waypoint_id)?;
                let retained = accepted.bundle.waypoints.iter().any(|fragment| {
                    fragment.waypoint_id == waypoint_id
                        && fragment.state == Some(WaypointState::Completed)
                }) || accepted
                    .bundle
                    .manifest
                    .operator_results
                    .iter()
                    .any(|result| result.waypoint_id == waypoint_id);
                if retained {
                    return Err(plan_error(
                        "retained-waypoint-required",
                        "completed or result-bearing waypoints are retained history.",
                        Some(&waypoint_id),
                    ));
                }
                match replacement {
                    Some(replacement) => {
                        if replacement.waypoint_id == waypoint_id {
                            return Err(plan_error(
                                "invalid_replacement",
                                "a replacement must have a new stable ID; use update to edit in place.",
                                Some(&waypoint_id),
                            ));
                        }
                        let inherited = accepted
                            .bundle
                            .waypoints
                            .iter()
                            .find(|fragment| fragment.waypoint_id == waypoint_id)
                            .map(|fragment| fragment.requirement_ids.clone())
                            .unwrap_or_default();
                        let new_id = replacement.waypoint_id.clone();
                        adjust = Some(Box::new(move |bundle: &mut MissionBundle| {
                            if let Some(fragment) = bundle
                                .waypoints
                                .iter_mut()
                                .find(|fragment| fragment.waypoint_id == new_id)
                            {
                                fragment.requirement_ids = inherited.clone();
                            }
                            Ok(())
                        }));
                        draft.waypoints[at] = replacement;
                    }
                    None => {
                        if draft
                            .waypoints
                            .iter()
                            .any(|other| other.depends_on.contains(&waypoint_id))
                        {
                            return Err(plan_error(
                                "replacement_required",
                                "other waypoints depend on this one; supply a valid replacement in the same operation.",
                                Some(&waypoint_id),
                            ));
                        }
                        draft.waypoints.remove(at);
                    }
                }
            }
            PlanOperation::ReorderWaypoints { order } => {
                for id in &order {
                    index_of(&draft, id)?;
                }
                let unique: BTreeSet<&String> = order.iter().collect();
                if order.len() != draft.waypoints.len() || unique.len() != order.len() {
                    return Err(plan_error(
                        "invalid_order",
                        "order must list every waypoint exactly once.",
                        None,
                    ));
                }
                let mut remaining = std::mem::take(&mut draft.waypoints);
                for id in &order {
                    let at = remaining
                        .iter()
                        .position(|waypoint| &waypoint.waypoint_id == id)
                        .ok_or_else(|| not_found(id))?;
                    draft.waypoints.push(remaining.remove(at));
                }
            }
            PlanOperation::SetRequirements { requirements } => {
                for requirement in &requirements {
                    for owner in &requirement.owner_waypoint_ids {
                        index_of(&draft, owner)?;
                    }
                }
                adjust = Some(Box::new(move |bundle: &mut MissionBundle| {
                    bundle.manifest.requirements = requirements
                        .iter()
                        .map(|item| item.requirement.clone())
                        .collect();
                    for fragment in &mut bundle.waypoints {
                        fragment.requirement_ids = requirements
                            .iter()
                            .filter(|item| item.owner_waypoint_ids.contains(&fragment.waypoint_id))
                            .map(|item| item.requirement.id.clone())
                            .collect();
                    }
                    Ok(())
                }));
            }
            PlanOperation::RecordEvidence {
                plan_digest,
                result,
            } => {
                index_of(&draft, &result.waypoint_id)?;
                let current = build_review_plan(&accepted, &source, &ledger)?.plan_digest;
                if plan_digest != current {
                    return Err(plan_error(
                        "revision_conflict",
                        "evidence is bound to a different plan digest than the current plan.",
                        None,
                    ));
                }
                new_ledger.entries.push(EvidenceBinding {
                    result_sha256: sha(&serde_json::to_vec(&result)?),
                    plan_digest,
                    revision: actual + 1,
                });
                adjust = Some(Box::new(move |bundle: &mut MissionBundle| {
                    bundle.manifest.operator_results.push(result.clone());
                    Ok(())
                }));
            }
        }
        draft.revision = actual.checked_add(1).ok_or_else(|| {
            plan_error(
                "revision_exhausted",
                "revision cannot be incremented.",
                None,
            )
        })?;
        let order: Vec<String> = draft
            .waypoints
            .iter()
            .map(|waypoint| waypoint.waypoint_id.clone())
            .collect();
        for (index, waypoint) in draft.waypoints.iter_mut().enumerate() {
            waypoint.depends_on = if index == 0 {
                vec![]
            } else {
                vec![order[index - 1].clone()]
            };
        }
        let recording_evidence = matches!(request.operation, PlanOperation::RecordEvidence { .. });
        let record_name = format!("{REQUEST_DIR}/{}.json", sha(request.request_id.as_bytes()));
        let outcome_digest = RefCell::new(None::<String>);
        let extras = |next: &AcceptedMission| -> Result<Vec<(String, Vec<u8>)>, MissionError> {
            let plan = build_review_plan(next, &draft, &new_ledger)?;
            *outcome_digest.borrow_mut() = Some(plan.plan_digest.clone());
            let record = RequestRecord {
                schema_version: 1,
                mission_id,
                request_id: request.request_id.clone(),
                request_sha256: request_sha.clone(),
                revision: next.bundle.manifest.revision,
                plan_digest: plan.plan_digest,
            };
            let mut files = vec![(record_name.clone(), serde_json::to_vec_pretty(&record)?)];
            if recording_evidence {
                files.push((
                    EVIDENCE_FILE.to_owned(),
                    serde_json::to_vec_pretty(&new_ledger)?,
                ));
            }
            Ok(files)
        };
        let write = PlanWrite {
            adjust: adjust.as_deref(),
            extras: Some(&extras),
        };
        self.save_typed_locked(&draft, actual, true, &write)?;
        let plan_digest = outcome_digest.into_inner().ok_or_else(|| {
            plan_error(
                "projection_diverged",
                "publication did not build a plan.",
                None,
            )
        })?;
        Ok(PlanOperationOutcome {
            revision: actual + 1,
            plan_digest,
            replayed: false,
        })
    }
}
