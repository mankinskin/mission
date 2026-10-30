use super::*;

impl<'a> MissionApproval<'a> {
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
}
