use super::*;

pub(super) fn apply_amendment(
    effective: &mut EffectiveIntentV4,
    amendment: IntentAmendmentV4,
    provider: &impl IntentReceiptProvider,
) -> Result<(), MissionError> {
    let id = effective.baseline.mission_id;
    if amendment.schema_version != INTENT_SCHEMA_VERSION
        || amendment.amendment_id.trim().is_empty()
        || amendment.changes.is_empty()
        || amendment.sequence != effective.amendments.len() as u64 + 1
        || amendment.prior_chain_sha256 != effective.chain_sha256
        || effective
            .amendments
            .iter()
            .any(|a| a.amendment_id == amendment.amendment_id)
        || amendment.amendment_sha256
            != intent_amendment_digest(
                id,
                &amendment.amendment_id,
                &amendment.prior_chain_sha256,
                &amendment.source,
                &amendment.changes,
            )?
        || amendment.receipt
            != receipt(
                provider,
                id,
                &amendment.receipt_ref,
                &intent_source_digest(amendment.source.as_bytes()),
                &amendment.amendment_sha256,
            )?
    {
        return Err(intent_error(
            "intent-conflict",
            "Amendment chain, identity or receipt mismatch.",
        ));
    }
    for old in std::iter::once(&effective.baseline.receipt)
        .chain(effective.amendments.iter().map(|a| &a.receipt))
    {
        if old.receipt_id == amendment.receipt.receipt_id
            || old.challenge_id == amendment.receipt.challenge_id
            || old.requester_event_id == amendment.receipt.requester_event_id
        {
            return Err(intent_error(
                "intent-conflict",
                "Each amendment requires a fresh confirmation.",
            ));
        }
    }
    apply_changes(effective, &amendment.source, &amendment.changes)?;
    effective.chain_sha256 = digest(
        "mission-intent-chain/v4",
        &(&effective.chain_sha256, &amendment),
    )?;
    effective.amendments.push(amendment);
    Ok(())
}

pub(super) fn apply_changes(
    effective: &mut EffectiveIntentV4,
    source: &str,
    changes: &[IntentChangeV4],
) -> Result<(), MissionError> {
    if source.trim().is_empty() {
        return Err(intent_error(
            "intent-source-invalid",
            "Requester amendment source cannot be empty.",
        ));
    }
    let mut seen: BTreeSet<String> = effective
        .baseline
        .requirements
        .iter()
        .map(|r| r.requirement_id.clone())
        .collect();
    for old in &effective.amendments {
        for change in &old.changes {
            if let IntentChangeV4::Add { requirement }
            | IntentChangeV4::Replace { requirement, .. } = change
            {
                seen.insert(requirement.requirement_id.clone());
            }
        }
    }
    let mut touched = BTreeSet::new();
    for change in changes {
        if let IntentChangeV4::Exclude {
            prior_requirement_id,
        }
        | IntentChangeV4::Replace {
            prior_requirement_id,
            ..
        } = change
        {
            if !touched.insert(prior_requirement_id)
                || !effective
                    .requirements
                    .iter()
                    .any(|r| r.requirement_id == *prior_requirement_id)
            {
                return Err(intent_error(
                    "intent-conflict",
                    "Amendments must name distinct active original IDs.",
                ));
            }
            effective
                .requirements
                .retain(|r| r.requirement_id != *prior_requirement_id);
            effective
                .excluded_requirement_ids
                .push(prior_requirement_id.clone());
        }
        if let IntentChangeV4::Add { requirement } | IntentChangeV4::Replace { requirement, .. } =
            change
        {
            validate_requirements(std::slice::from_ref(requirement), source)?;
            if !seen.insert(requirement.requirement_id.clone()) {
                return Err(intent_error(
                    "intent-conflict",
                    "Requirement IDs cannot be reused.",
                ));
            }
            effective.requirements.push(requirement.clone());
        }
    }
    effective
        .requirements
        .sort_by(|a, b| a.requirement_id.cmp(&b.requirement_id));
    effective.excluded_requirement_ids.sort();
    Ok(())
}
