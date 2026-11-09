use super::*;

pub(crate) fn intent_error(code: &'static str, message: impl Into<String>) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "intent".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.into(),
        }],
    )
}

pub fn intent_source_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn digest<T: Serialize>(domain: &str, value: &T) -> Result<String, MissionError> {
    let mut hash = Sha256::new();
    hash.update(domain.as_bytes());
    hash.update(b"\n");
    hash.update(serde_json::to_vec(value)?);
    Ok(format!("{hash:x}", hash = hash.finalize()))
}

pub fn intent_baseline_digest(
    mission_id: Uuid,
    requirements: &[BaselineRequirementV4],
) -> Result<String, MissionError> {
    let mut sorted = requirements.to_vec();
    sorted.sort_by(|a, b| a.requirement_id.cmp(&b.requirement_id));
    digest("mission-intent-baseline/v4", &(mission_id, sorted))
}

pub fn intent_amendment_digest(
    mission_id: Uuid,
    amendment_id: &str,
    prior_chain: &str,
    source: &str,
    changes: &[IntentChangeV4],
) -> Result<String, MissionError> {
    digest(
        "mission-intent-amendment/v4",
        &(mission_id, amendment_id, prior_chain, source, changes),
    )
}

pub(super) fn validate_requirements(
    requirements: &[BaselineRequirementV4],
    source: &str,
) -> Result<(), MissionError> {
    if requirements.is_empty() {
        return Err(intent_error(
            "intent-baseline-invalid",
            "Intent requires at least one requirement.",
        ));
    }
    let mut ids = BTreeSet::new();
    let sha = intent_source_digest(source.as_bytes());
    for requirement in requirements {
        let span = &requirement.source;
        if requirement.requirement_id.trim().is_empty()
            || requirement.outcome.trim().is_empty()
            || !ids.insert(&requirement.requirement_id)
            || span.source_sha256 != sha
            || span.start >= span.end
            || source.get(span.start..span.end) != Some(span.excerpt.as_str())
        {
            return Err(intent_error(
                "intent-source-invalid",
                "Unique IDs and exact nonempty UTF-8 source spans are required.",
            ));
        }
        let compatible = matches!(
            (&requirement.category, &requirement.evidence_policy),
            (
                IntentOutcomeCategory::WorkingBehavior,
                IntentEvidencePolicy::ExecutableBehavior { .. }
            ) | (
                IntentOutcomeCategory::Artifact,
                IntentEvidencePolicy::DeterministicArtifact
            ) | (
                IntentOutcomeCategory::Decision,
                IntentEvidencePolicy::CapturedDecision
            ) | (
                IntentOutcomeCategory::ResearchReview,
                IntentEvidencePolicy::ResearchRecord
            )
        );
        if !compatible {
            return Err(intent_error(
                "intent-evidence-incompatible",
                "Outcome and evidence policy are incompatible.",
            ));
        }
    }
    Ok(())
}

pub(super) fn receipt(
    provider: &impl IntentReceiptProvider,
    id: Uuid,
    reference: &IntentReceiptReference,
    source_sha: &str,
    intent_sha: &str,
) -> Result<IntentReceiptV4, MissionError> {
    let receipt = provider.confirmed_receipt(id, reference)?;
    if reference.session_id.trim().is_empty()
        || reference.challenge_id.is_nil()
        || receipt.challenge_id != reference.challenge_id.to_string()
        || receipt.source_sha256 != source_sha
        || receipt.baseline_sha256 != intent_sha
        || receipt.provider_version != INTENT_RECEIPT_PROVIDER
        || receipt.requester_event_id.trim().is_empty()
        || receipt.captured_at.trim().is_empty()
        || Uuid::parse_str(&receipt.receipt_id).is_err()
    {
        return Err(intent_error(
            "intent-confirmation-mismatch",
            "Trusted receipt must bind the exact source, proposed intent and provider version.",
        ));
    }
    Ok(receipt)
}
