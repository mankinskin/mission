use super::*;

impl<'a> MissionApproval<'a> {
    pub fn record_review(
        &self,
        id: Uuid,
        input: ReviewInput,
    ) -> Result<ReviewRecord, ApprovalError> {
        self.record_review_inner(id, input, None)
    }

    /// Record or replay one current-snapshot review under the provider lock.
    pub fn record_review_with_request_id(
        &self,
        id: Uuid,
        input: ReviewInput,
        request_id: &str,
    ) -> Result<ReviewRecord, ApprovalError> {
        if request_id.trim().is_empty() {
            return Err(required("a nonblank review request identity is required"));
        }
        self.record_review_inner(id, input, Some(request_id.to_owned()))
    }

    fn record_review_inner(
        &self,
        id: Uuid,
        input: ReviewInput,
        request_id: Option<String>,
    ) -> Result<ReviewRecord, ApprovalError> {
        let tx = self.begin(id)?;
        let output_binding = self.verify_integrity(&tx)?;
        let plan = tx.review_plan()?;
        let intent_provider = crate::intent::SessionIntentProvider::new(&self.workspace_root)
            .map_err(integrity_error)?;
        let effective_intent = tx
            .effective_intent(&intent_provider)
            .map_err(integrity_error)?;
        coverage::validate_source_goal(effective_intent.as_ref(), &plan, &input)?;
        scope::validate_scope_review(&plan, &input)?;
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
        if let Some(request_id) = request_id.as_ref()
            && let Some(record) = reviews(&journal)
                .into_iter()
                .find(|record| record.request_id.as_ref() == Some(request_id))
        {
            if record.revision == input.revision
                && record.plan_digest == input.plan_digest
                && record.evidence_identity == input.evidence_identity
                && record.kind == input.kind
                && record.verdict == input.verdict
                && record.reviewer == input.reviewer
                && record.cold_review_id == input.cold_review_id
                && record.goal_coverage_findings == input.goal_coverage_findings
                && record.terminal_outcome_finding == input.terminal_outcome_finding
                && record.scope_findings == input.scope_findings
                && record.output_contract_binding.as_deref() == Some(output_binding.as_str())
            {
                return Ok(record.clone());
            }
            return Err(conflict(
                "review request identity was reused with different content",
            ));
        }
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
                            && r.output_contract_binding.as_deref() == Some(output_binding.as_str())
                            && input
                                .terminal_outcome_finding
                                .as_ref()
                                .is_none_or(|terminal| coverage::same_source_snapshot(r, terminal))
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
        let mut record = ReviewRecord {
            review_id: format!("review-{:08}", journal.len() + 1),
            request_id,
            kind: input.kind,
            verdict: input.verdict,
            reviewer: input.reviewer,
            revision: plan.revision,
            plan_digest: plan.plan_digest,
            evidence_identity: plan.evidence_identity,
            cold_review_id: input.cold_review_id,
            output_contract_binding: Some(output_binding),
            goal_coverage_findings: input.goal_coverage_findings,
            terminal_outcome_finding: input.terminal_outcome_finding,
            source_goal_binding_sha256: None,
            scope_findings: input.scope_findings,
            scope_binding_sha256: None,
        };
        record.source_goal_binding_sha256 = coverage::review_binding_sha256(&record)?;
        record.scope_binding_sha256 = scope::review_scope_binding_sha256(&record)?;
        append(&tx, &mut journal, Entry::Review(record.clone()))?;
        Ok(record)
    }
    pub fn reviews(&self, id: Uuid) -> Result<Vec<ReviewRecord>, ApprovalError> {
        let tx = self.begin(id)?;
        Ok(reviews(&read_journal(&tx)?).into_iter().cloned().collect())
    }
}
