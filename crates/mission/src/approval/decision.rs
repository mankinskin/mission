use super::*;

impl<'a> MissionApproval<'a> {
    pub fn accept_decision(
        &self,
        id: Uuid,
        reference: &RequesterDecisionRef,
    ) -> Result<AcceptedDecision, ApprovalError> {
        self.accept_decision_with_fault(id, reference, None)
    }
    pub fn accept_decision_with_fault(
        &self,
        id: Uuid,
        reference: &RequesterDecisionRef,
        fault: Option<ApprovalStep>,
    ) -> Result<AcceptedDecision, ApprovalError> {
        let tx = self.begin(id)?;
        let receipt = self.verify_receipt(id, reference)?;
        let mut journal = read_journal(&tx)?;
        let record = journal
            .iter()
            .find_map(|r| match &r.entry {
                Entry::Challenge(c) if c.challenge_id == reference.challenge_id => Some(c.clone()),
                _ => None,
            })
            .ok_or_else(|| unavailable("challenge was not prepared by this Mission"))?;
        let challenge = self
            .session
            .read_requester_challenge(&reference.session_id, &reference.challenge_id)
            .map_err(session_error)?;
        let request = &challenge.request;
        if record.session_id != reference.session_id
            || request.session_id != record.session_id
            || request.mission_id != id
            || request.mission_root != self.workspace_root
            || request.revision != record.revision
            || request.plan_digest != record.plan_digest
            || request.review_snapshot_digest != record.review_snapshot_digest
            || receipt.revision != record.revision
            || receipt.plan_digest != record.plan_digest
            || receipt.review_snapshot_digest != record.review_snapshot_digest
        {
            return Err(conflict("receipt and Session/Mission challenges disagree"));
        }
        let approve = receipt.kind == DecisionKind::Approve;
        let disposition = disposition_id(if approve { "g" } else { "d" }, id, reference);
        let grant_now = ApprovalGrant {
            grant_id: disposition.clone(),
            decision: reference.clone(),
            challenge_id: record.challenge_id.clone(),
            revision: record.revision,
            plan_digest: record.plan_digest.clone(),
            review_snapshot_digest: record.review_snapshot_digest.clone(),
            cold_review_id: record.cold_review_id.clone(),
            formal_review_id: record.formal_review_id.clone(),
        };
        let decision_now = Disposition {
            disposition_id: disposition.clone(),
            decision: reference.clone(),
            challenge_id: record.challenge_id.clone(),
            kind: receipt.kind,
        };
        let prior_grant = journal.iter().find_map(|r| match &r.entry {
            Entry::Grant(g) if g.challenge_id == record.challenge_id => Some(g.clone()),
            _ => None,
        });
        let prior_decision = journal.iter().find_map(|r| match &r.entry {
            Entry::Decision(d) if d.challenge_id == record.challenge_id => Some(d.clone()),
            _ => None,
        });
        if prior_grant.as_ref().is_some_and(|g| *g != grant_now)
            || prior_decision.as_ref().is_some_and(|d| *d != decision_now)
            || (prior_grant.is_some() && !approve)
            || (prior_decision.is_some() && approve)
        {
            return Err(conflict(
                "challenge already has a divergent accepted decision",
            ));
        }
        let fresh = prior_grant.is_none() && prior_decision.is_none();
        if fresh {
            let plan = tx.review_plan()?;
            if plan.revision != record.revision
                || plan.plan_digest != record.plan_digest
                || plan.evidence_identity != record.evidence_identity
            {
                return Err(stale("plan changed after the challenge"));
            }
            match current_reviews(&journal, &plan) {
                Some((cold, formal))
                    if cold.review_id == record.cold_review_id
                        && formal.review_id == record.formal_review_id => {}
                Some(_) => return Err(stale("supporting reviews were superseded")),
                None => return Err(required("supporting reviews no longer pass")),
            }
            if self
                .session
                .requester_session_finished(&reference.session_id)
                .map_err(session_error)?
            {
                return Err(unavailable(
                    "finished Session decisions are historical only",
                ));
            }
            if fault == Some(ApprovalStep::BeforeGrant) {
                return Err(pending_error("decision verified without disposition"));
            }
            append(
                &tx,
                &mut journal,
                if approve {
                    Entry::Grant(grant_now.clone())
                } else {
                    Entry::Decision(decision_now)
                },
            )?;
            if fault == Some(ApprovalStep::AfterGrant) {
                return Err(pending_error("disposition recorded but unconsumed"));
            }
        }
        let existing = self
            .session
            .read_requester_consumption(&reference.session_id, &reference.challenge_id)
            .map_err(session_error)?;
        match &existing {
            Some(c) if c.reference != *reference || c.grant_id != disposition => {
                return Err(conflict("different disposition consumed challenge"));
            }
            Some(_) => {}
            None => {
                if approve
                    && !fresh
                    && (tx.review_plan()?.plan_digest != record.plan_digest
                        || tx.semantic_revision()? > record.revision)
                {
                    return Err(stale(
                        "plan changed after the grant was journaled; a fresh challenge is required",
                    ));
                }
                self.session
                    .consume_requester_challenge(reference, &disposition)
                    .map_err(session_error)?;
            }
        }
        Ok(AcceptedDecision {
            kind: receipt.kind,
            grant: approve.then_some(grant_now),
            replayed: !fresh && existing.is_some(),
        })
    }
    fn pending_stage(&self, journal: &[Record], c: &ChallengeBinding) -> Option<&'static str> {
        if self
            .session
            .read_requester_challenge(&c.session_id, &c.challenge_id)
            .is_err()
        {
            return Some("challenge-unregistered");
        }
        match self
            .session
            .find_requester_decision_for_challenge(&c.session_id, &c.challenge_id)
        {
            Ok(Some(_)) => {}
            Ok(None) => return None,
            Err(_) => return Some("session-unreadable"),
        }
        if !journal.iter().any(|r| match &r.entry {
            Entry::Grant(g) => g.challenge_id == c.challenge_id,
            Entry::Decision(d) => d.challenge_id == c.challenge_id,
            _ => false,
        }) {
            return Some("decision-unaccepted");
        }
        match self
            .session
            .read_requester_consumption(&c.session_id, &c.challenge_id)
        {
            Ok(Some(_)) => None,
            Ok(None) => Some("disposition-unconsumed"),
            Err(_) => Some("session-unreadable"),
        }
    }
    pub fn pending(&self, id: Uuid) -> Result<Vec<PendingApproval>, ApprovalError> {
        let tx = self.begin(id)?;
        let journal = read_journal(&tx)?;
        Ok(journal
            .iter()
            .filter_map(|r| match &r.entry {
                Entry::Challenge(c) => {
                    self.pending_stage(&journal, c)
                        .map(|stage| PendingApproval {
                            challenge_id: c.challenge_id.clone(),
                            stage: stage.into(),
                        })
                }
                _ => None,
            })
            .collect())
    }
}
