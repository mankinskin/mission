use super::*;

pub(super) fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn validate(
    record: &CompletionEvidenceV4,
    reference: &ProviderEvidenceRefV4,
    expected: &ProviderEvidenceBindingV4,
    contract: &WaypointExecutionContractV4,
    purpose: EvidencePurposeV4,
    allow_mock: bool,
) -> Result<(), LifecycleError> {
    let reject = |code, message| Err(control_error(code, message));
    if record.schema_version != 4 {
        return reject(
            "provider_version_incompatible",
            "evidence schema must be v4",
        );
    }
    if record.reference != *reference || record.purpose != purpose {
        return reject(
            "provider_receipt_mismatch",
            "receipt identity or purpose differs",
        );
    }
    let b = &record.binding;
    if b.provider != expected.provider
        || b.route_id != expected.route_id
        || b.receipt_class != expected.receipt_class
    {
        return reject("provider_identity_mismatch", "provider/route/class differs");
    }
    if b.provider_version != expected.provider_version {
        return reject("provider_version_incompatible", "provider version differs");
    }
    if b.owning_root != expected.owning_root {
        return reject("provider_root_mismatch", "owning root differs");
    }
    if b.profile != expected.profile || b.environment_digest != expected.environment_digest {
        return reject("provider_profile_mismatch", "profile/environment differs");
    }
    if b.waypoint_id != expected.waypoint_id || b.mission_id != expected.mission_id {
        return reject("provider_waypoint_mismatch", "Mission/waypoint differs");
    }
    if b.source_digest != expected.source_digest
        || b.intent_chain_digest != expected.intent_chain_digest
        || b.plan_digest != expected.plan_digest
        || b.contract_digest != expected.contract_digest
        || b.prerequisite_artifact_digests != expected.prerequisite_artifact_digests
        || b.prerequisite_artifact_digests
            .keys()
            .collect::<BTreeSet<_>>()
            != contract
                .declared_dependency_ids
                .iter()
                .collect::<BTreeSet<_>>()
        || waypoint_execution_contract_digest(contract).as_ref().ok() != Some(&b.contract_digest)
        || [
            &b.source_digest,
            &b.intent_chain_digest,
            &b.plan_digest,
            &b.contract_digest,
            &b.environment_digest,
        ]
        .iter()
        .any(|d| !is_digest(d))
        || b.prerequisite_artifact_digests
            .values()
            .any(|d| !is_digest(d))
    {
        return reject(
            "provider_digest_mismatch",
            "source/intent/plan/contract/prerequisites differ",
        );
    }
    match record.validity {
        ProviderValidityV4::Stale => {
            return reject("provider_evidence_stale", "provider declares stale");
        }
        ProviderValidityV4::Revoked => {
            return reject("provider_evidence_revoked", "provider revoked receipt");
        }
        ProviderValidityV4::Current => {}
    }
    if !record.ready {
        return reject("provider_unready", "provider is not ready");
    }
    if record.diagnostic_reruns > contract.failure_policy.max_attempts.min(1) {
        return reject(
            "failure_bound_exhausted",
            "one diagnostic rerun maximum; block/escalate",
        );
    }
    if record.diagnostic_reruns > 0
        && record
            .fix_justification
            .as_deref()
            .is_none_or(|j| j.trim().is_empty())
    {
        return reject(
            "diagnostic_fix_required",
            "diagnostic rerun requires a justified fix",
        );
    }
    let conditions = match purpose {
        EvidencePurposeV4::Preflight => &contract.preconditions,
        EvidencePurposeV4::Completion => &contract.postconditions,
    };
    let expected_conditions: BTreeSet<_> = conditions.iter().map(|c| c.id.as_str()).collect();
    let actual_conditions: BTreeSet<_> = record.condition_ids.iter().map(String::as_str).collect();
    if actual_conditions != expected_conditions
        || actual_conditions.len() != record.condition_ids.len()
    {
        return reject(
            "condition_evidence_required",
            "all exact declared conditions required",
        );
    }
    let checks: Vec<_> = contract
        .checks
        .iter()
        .filter(|check| applicable_execution_check(contract, check, purpose))
        .collect();
    let actual: BTreeMap<_, _> = record
        .checks
        .iter()
        .map(|c| (c.check_id.as_str(), c))
        .collect();
    if actual.len() != record.checks.len()
        || checks.len() != actual.len()
        || checks.iter().any(|c| !actual.contains_key(c.id.as_str()))
    {
        return reject(
            "automated_check_evidence_required",
            "all applicable declared checks required, exactly once",
        );
    }
    if checks.is_empty() {
        return reject("zero_executed_checks", "nonzero automated checks required");
    }
    for check in checks {
        let result = actual[check.id.as_str()];
        if result.check_digest
            != execution_check_digest(check)
                .map_err(|e| control_error("provider_evidence_corrupt", e.to_string()))?
        {
            return reject(
                "check_metadata_mismatch",
                "command/cwd/kind/class hash differs",
            );
        }
        if result.evidence_class != check.evidence_class {
            return reject("evidence_class_mismatch", "automated check class differs");
        }
        if result.executed == 0 {
            return reject(
                "zero_executed_checks",
                "required check executed zero tests/checks",
            );
        }
        if result.failed != 0 || result.skipped != 0 {
            return reject(
                "automated_check_failed",
                "failed/skipped required check; block/escalate",
            );
        }
        if result.mock
            && (!allow_mock
                || purpose == EvidencePurposeV4::Preflight
                || result.evidence_class != ExecutionEvidenceClassV4::ExecutableBehavior)
        {
            return reject(
                "mock_evidence_prohibited",
                "mock is diagnostic-only under confirmed intent policy",
            );
        }
    }
    Ok(())
}

pub(super) fn handover(
    artifact: &HandoverArtifactV4,
    expected: &ExecutionHandoverV4,
    owning_root: &Path,
    producer_digest: &str,
    consumer_digest: &str,
) -> Result<(), LifecycleError> {
    if artifact.id != expected.id
        || artifact.producer != expected.producer
        || artifact.consumer != expected.consumer
        || artifact.producer_contract_digest != producer_digest
        || artifact.consumer_contract_digest != consumer_digest
        || !is_digest(producer_digest)
        || !is_digest(consumer_digest)
    {
        return Err(control_error(
            "handover_contract_mismatch",
            "exact producer/consumer/precondition contract differs",
        ));
    }
    let declared_root = Path::new(&expected.producer.root);
    if declared_root.is_absolute()
        || declared_root
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        || artifact.owning_root
            != declared_root
                .components()
                .fold(owning_root.to_path_buf(), |mut root, part| {
                    if let Component::Normal(name) = part {
                        root.push(name);
                    }
                    root
                })
                .to_string_lossy()
    {
        return Err(control_error(
            "handover_root_mismatch",
            "artifact owning root differs",
        ));
    }
    if !artifact.schema_valid {
        return Err(control_error(
            "handover_schema_invalid",
            "provider schema check failed",
        ));
    }
    if artifact.content.is_empty()
        || evidence_content_digest(&artifact.content) != artifact.content_sha256
    {
        return Err(control_error(
            "handover_content_mismatch",
            "artifact content hash differs or content is absent",
        ));
    }
    Ok(())
}
