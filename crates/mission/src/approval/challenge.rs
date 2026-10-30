use super::*;

impl<'a> MissionApproval<'a> {
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
    pub(super) fn verify_receipt(
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
}
