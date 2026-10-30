use super::*;

pub(super) fn plan_error(
    code: &'static str,
    message: &str,
    waypoint_id: Option<&str>,
) -> MissionError {
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

pub(super) fn not_found(id: &str) -> MissionError {
    plan_error(
        "waypoint_not_found",
        "no waypoint with this stable ID exists in the Mission.",
        Some(id),
    )
}

pub(super) fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn index_of(draft: &TypedDraft, id: &str) -> Result<usize, MissionError> {
    draft
        .waypoints
        .iter()
        .position(|waypoint| waypoint.waypoint_id == id)
        .ok_or_else(|| not_found(id))
}

pub(super) fn validate_request_id(request_id: &str) -> Result<(), MissionError> {
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

pub(super) fn read_optional(
    directory: &Path,
    path: &Path,
) -> Result<Option<Vec<u8>>, MissionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            typed_source::ensure_safe_path(directory, path)?;
            Ok(Some(fs::read(path)?))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn ledger_from_bytes(bytes: Option<&[u8]>) -> Result<EvidenceLedger, MissionError> {
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

pub(crate) fn read_ledger(directory: &Path) -> Result<EvidenceLedger, MissionError> {
    let bytes = read_optional(directory, &directory.join(EVIDENCE_FILE))?;
    ledger_from_bytes(bytes.as_deref())
}

pub(crate) fn nonsemantic_ledger(
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

pub(super) fn read_record(
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

pub(super) fn references(fragment: &WaypointFragment) -> Result<Vec<String>, MissionError> {
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
pub(crate) fn build_review_plan(
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

pub(super) fn snapshot_retryable(error: &MissionError) -> bool {
    match error {
        MissionError::NotFound(_) => true,
        MissionError::Io(error) => error.kind() == std::io::ErrorKind::NotFound,
        MissionError::Validation(_, diagnostics) => diagnostics
            .iter()
            .any(|d| matches!(d.code, "authority-diverged" | "projection_diverged")),
        _ => false,
    }
}
