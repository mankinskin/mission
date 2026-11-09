use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FidelityBindingV4 {
    pub source_sha256: String,
    pub baseline_sha256: String,
    pub amendment_chain_sha256: String,
    pub cold_review_binding_sha256: String,
    pub formal_review_binding_sha256: String,
}

fn upgrade() -> ApprovalError {
    ApprovalError::new(
        "approval_policy_upgrade_required",
        "New approval or entry requires confirmed intent and fresh source-goal coverage reviews.",
    )
}

impl MissionApproval<'_> {
    pub(super) fn fidelity_binding(
        &self,
        tx: &ApprovalTransaction<'_>,
        plan: &ReviewPlan,
        cold: &ReviewRecord,
        formal: &ReviewRecord,
    ) -> Result<FidelityBindingV4, ApprovalError> {
        let provider = crate::intent::SessionIntentProvider::new(&self.workspace_root)
            .map_err(integrity_error)?;
        let intent = tx
            .effective_intent(&provider)
            .map_err(integrity_error)?
            .ok_or_else(upgrade)?;
        for review in [cold, formal] {
            if review.source_goal_binding_sha256.is_none()
                || review.terminal_outcome_finding.is_none()
            {
                return Err(upgrade());
            }
            let input = ReviewInput {
                kind: review.kind,
                verdict: review.verdict,
                reviewer: review.reviewer.clone(),
                revision: review.revision,
                plan_digest: review.plan_digest.clone(),
                evidence_identity: review.evidence_identity.clone(),
                cold_review_id: review.cold_review_id.clone(),
                goal_coverage_findings: review.goal_coverage_findings.clone(),
                terminal_outcome_finding: review.terminal_outcome_finding.clone(),
            };
            coverage::validate_source_goal(Some(&intent), plan, &input)?;
        }
        if cold.kind != ReviewKind::ColdWalkthrough
            || formal.kind != ReviewKind::Formal
            || cold.verdict != ReviewVerdict::Pass
            || formal.verdict != ReviewVerdict::Pass
            || cold.reviewer == formal.reviewer
            || formal.cold_review_id.as_deref() != Some(cold.review_id.as_str())
        {
            return Err(required(
                "fidelity requires a distinct passing cold/formal pair",
            ));
        }
        Ok(FidelityBindingV4 {
            source_sha256: intent.baseline.source_sha256,
            baseline_sha256: intent.baseline.baseline_sha256,
            amendment_chain_sha256: intent.chain_sha256,
            cold_review_binding_sha256: coverage::review_binding_sha256(cold)?
                .ok_or_else(upgrade)?,
            formal_review_binding_sha256: coverage::review_binding_sha256(formal)?
                .ok_or_else(upgrade)?,
        })
    }

    pub(super) fn verify_fidelity_challenge(
        &self,
        tx: &ApprovalTransaction<'_>,
        journal: &[Record],
        binding: &ChallengeBinding,
    ) -> Result<(), ApprovalError> {
        let recorded = binding.fidelity_binding.as_ref().ok_or_else(upgrade)?;
        let find = |id: &str| {
            journal.iter().find_map(|record| match &record.entry {
                Entry::Review(review) if review.review_id == id => Some(review),
                _ => None,
            })
        };
        let cold =
            find(&binding.cold_review_id).ok_or_else(|| corrupt("missing cold review".into()))?;
        let formal = find(&binding.formal_review_id)
            .ok_or_else(|| corrupt("missing formal review".into()))?;
        let mut plan = tx.review_plan()?;
        if plan.plan_digest != binding.plan_digest || tx.semantic_revision()? > binding.revision {
            return Err(stale("semantic plan changed after fidelity approval"));
        }
        // Legal evidence/progress revisions do not change the reviewed semantic route.
        plan.revision = binding.revision;
        if self.fidelity_binding(tx, &plan, cold, formal)? != *recorded {
            return Err(stale(
                "source, intent, amendments or coverage changed after challenge",
            ));
        }
        Ok(())
    }
}
