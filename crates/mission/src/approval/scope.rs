use super::*;
use mission_api::{
    ExecutionCheckKindV4, ScopeCriterionVerdictV4, WaypointExecutionContractV4,
    waypoint_execution_contract_digest,
};
use sha2::{Digest, Sha256};

pub(super) fn validate_scope_review(
    plan: &ReviewPlan,
    input: &ReviewInput,
) -> Result<Option<String>, ApprovalError> {
    let has_contract = plan.waypoints.iter().any(|waypoint| {
        waypoint.execution_contract.is_some() || waypoint.execution_contract_digest.is_some()
    });
    if !has_contract && input.scope_findings.is_empty() {
        return Ok(None);
    }

    let contract_set_digest = contract_set_binding_sha256(plan)?;
    if input.scope_findings.len() != plan.waypoints.len() {
        return Err(ApprovalError::new(
            "approval_scope_audit_required",
            "Every current contract-bearing waypoint needs a complete scope audit before review can authorize approval.",
        ));
    }

    for (waypoint, finding) in plan.waypoints.iter().zip(&input.scope_findings) {
        let contract = waypoint.execution_contract.as_ref().ok_or_else(upgrade)?;
        validate_finding(waypoint, contract, finding, input.verdict)?;
    }

    Ok(Some(contract_set_digest))
}

pub(super) fn contract_set_binding_sha256(plan: &ReviewPlan) -> Result<String, ApprovalError> {
    if plan.waypoints.is_empty() {
        return Err(upgrade());
    }
    let mut identities = Vec::with_capacity(plan.waypoints.len());
    for waypoint in &plan.waypoints {
        let contract = waypoint.execution_contract.as_ref().ok_or_else(upgrade)?;
        let computed = waypoint_execution_contract_digest(contract)
            .map_err(|error| corrupt(error.to_string()))?;
        let provider_digest = waypoint
            .execution_contract_digest
            .as_deref()
            .ok_or_else(upgrade)?;
        if computed != provider_digest {
            return Err(stale(
                "provider execution-contract digest does not match the reviewed contract",
            ));
        }
        identities.push((waypoint.waypoint_id.as_str(), provider_digest));
    }
    let bytes = serde_json::to_vec(&("mission-execution-contract-set/v4", identities))
        .map_err(|error| corrupt(error.to_string()))?;
    Ok(hex(&Sha256::digest(bytes)))
}

pub(super) fn review_scope_binding_sha256(
    record: &ReviewRecord,
) -> Result<Option<String>, ApprovalError> {
    if record.scope_findings.is_empty() {
        if record.scope_binding_sha256.is_some() {
            return Err(corrupt(
                "scope review binding exists without scope findings".into(),
            ));
        }
        return Ok(None);
    }
    let bytes = serde_json::to_vec(&(
        "mission-waypoint-scope-review/v4",
        record.review_id.as_str(),
        record.kind,
        record.verdict,
        record.revision,
        record.plan_digest.as_str(),
        record.evidence_identity.as_str(),
        record.cold_review_id.as_deref(),
        &record.scope_findings,
    ))
    .map_err(|error| corrupt(error.to_string()))?;
    Ok(Some(hex(&Sha256::digest(bytes))))
}

fn validate_finding(
    waypoint: &mission_api::ReviewWaypoint,
    contract: &WaypointExecutionContractV4,
    finding: &WaypointScopeFindingV4,
    verdict: ReviewVerdict,
) -> Result<(), ApprovalError> {
    let contract_digest = waypoint
        .execution_contract_digest
        .as_deref()
        .ok_or_else(upgrade)?;
    if finding.waypoint_id != waypoint.waypoint_id
        || finding.execution_contract_digest != contract_digest
    {
        return Err(stale(
            "scope finding is bound to another waypoint or execution-contract digest",
        ));
    }

    let context_ids: Vec<_> = contract
        .authoritative_context
        .iter()
        .map(|context| context.id.clone())
        .collect();
    let acceptance_check_ids: Vec<_> = contract
        .checks
        .iter()
        .filter(|check| check.kind == ExecutionCheckKindV4::Acceptance)
        .map(|check| check.id.clone())
        .collect();
    if finding.boundary_task != contract.boundary.task
        || finding.boundary_paths != contract.boundary.paths
        || finding.context_ids != context_ids
        || finding.acceptance_check_ids != acceptance_check_ids
        || context_ids.is_empty()
        || acceptance_check_ids.is_empty()
    {
        return Err(ApprovalError::new(
            "approval_scope_evidence_invalid",
            "Scope evidence must identify the exact declared boundary, authoritative context, and acceptance checks.",
        ));
    }

    if finding.outcome_count != 1 {
        return Err(not_manageable(
            "A waypoint must have one coherent expected outcome; split multiple outcomes into separately ticket-backed waypoints.",
        ));
    }
    let criteria = [
        &finding.completeness,
        &finding.compactness,
        &finding.approachability,
        &finding.single_coherent_outcome,
        &finding.single_agent_one_sitting,
    ];
    if finding.rationale.trim().is_empty()
        || criteria
            .iter()
            .any(|criterion| criterion.rationale.trim().is_empty())
    {
        return Err(ApprovalError::new(
            "approval_scope_audit_incomplete",
            "Each scope criterion and the single-agent assessment needs a rationale grounded in the declared context and boundary.",
        ));
    }
    if verdict == ReviewVerdict::Pass
        && criteria
            .iter()
            .any(|criterion| criterion.verdict != ScopeCriterionVerdictV4::Pass)
    {
        return Err(not_manageable(
            "The scope audit failed; decompose the waypoint into ticket-backed work packages that fit the existing single-agent, one-sitting threshold.",
        ));
    }
    Ok(())
}

fn not_manageable(message: &str) -> ApprovalError {
    ApprovalError::new("approval_scope_not_manageable", message)
}

fn upgrade() -> ApprovalError {
    ApprovalError::new(
        "approval_policy_upgrade_required",
        "New approval or entry requires complete execution contracts and current waypoint scope audits.",
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
