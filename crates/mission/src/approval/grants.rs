use super::*;

impl<'a> MissionApproval<'a> {
    pub fn grants(&self, id: Uuid) -> Result<Vec<ApprovalGrant>, ApprovalError> {
        let tx = self.begin(id)?;
        let mut out = Vec::new();
        for record in read_journal(&tx)? {
            if let Entry::Grant(g) = record.entry {
                let consumed = self
                    .session
                    .read_requester_consumption(&g.decision.session_id, &g.challenge_id)
                    .map_err(session_error)?;
                if consumed.is_some_and(|c| c.reference == g.decision && c.grant_id == g.grant_id) {
                    out.push(g);
                }
            }
        }
        Ok(out)
    }
    fn effective_grant(
        &self,
        tx: &ApprovalTransaction<'_>,
    ) -> Result<ApprovalGrant, ApprovalError> {
        let plan = tx.review_plan()?;
        let journal = read_journal(tx)?;
        let (index, grant) = journal
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, r)| match &r.entry {
                Entry::Grant(g) => Some((i, g.clone())),
                _ => None,
            })
            .ok_or_else(|| required("no requester approval grant exists"))?;
        for later in &journal[index + 1..] {
            match &later.entry {
                Entry::Decision(_) => return Err(required("later reject/replan superseded grant")),
                Entry::Review(r)
                    if r.verdict == ReviewVerdict::Fail && r.plan_digest == grant.plan_digest =>
                {
                    return Err(required("review failed after grant"));
                }
                _ => {}
            }
        }
        if plan.plan_digest != grant.plan_digest || tx.semantic_revision()? > grant.revision {
            return Err(stale("semantic plan changed; fresh approval required"));
        }
        match self
            .session
            .read_requester_consumption(&grant.decision.session_id, &grant.challenge_id)
            .map_err(session_error)?
        {
            Some(c) if c.reference == grant.decision && c.grant_id == grant.grant_id => {}
            Some(_) => return Err(conflict("Session consumption names another grant")),
            None => return Err(pending_error("grant not consumed in Session")),
        }
        if self
            .session
            .requester_session_finished(&grant.decision.session_id)
            .map_err(session_error)?
        {
            return Err(required(
                "granting Session finished; handoff requires a fresh challenge",
            ));
        }
        self.verify_chain(tx.mission_id(), &journal, index, &grant)?;
        let receipt = self.verify_receipt(tx.mission_id(), &grant.decision)?;
        if receipt.kind != DecisionKind::Approve
            || receipt.revision != grant.revision
            || receipt.plan_digest != grant.plan_digest
            || receipt.review_snapshot_digest != grant.review_snapshot_digest
        {
            return Err(conflict("reread receipt disagrees with grant"));
        }
        Ok(grant)
    }
    fn verify_chain(
        &self,
        id: Uuid,
        journal: &[Record],
        index: usize,
        grant: &ApprovalGrant,
    ) -> Result<(), ApprovalError> {
        let broken = |what: &str| corrupt(format!("grant {} chain: {what}", grant.grant_id));
        if grant.challenge_id != grant.decision.challenge_id
            || grant.grant_id != disposition_id("g", id, &grant.decision)
        {
            return Err(broken("grant identity disagrees with decision reference"));
        }
        let (ci, binding) = journal[..index]
            .iter()
            .enumerate()
            .find_map(|(i, r)| match &r.entry {
                Entry::Challenge(c) if c.challenge_id == grant.challenge_id => Some((i, c)),
                _ => None,
            })
            .ok_or_else(|| broken("no preceding challenge"))?;
        let review = |rid: &str| {
            journal[..ci].iter().find_map(|r| match &r.entry {
                Entry::Review(v) if v.review_id == rid => Some(v),
                _ => None,
            })
        };
        let cold = review(&binding.cold_review_id)
            .ok_or_else(|| broken("missing preceding cold review"))?;
        let formal = review(&binding.formal_review_id)
            .ok_or_else(|| broken("missing preceding formal review"))?;
        if cold.kind != ReviewKind::ColdWalkthrough
            || formal.kind != ReviewKind::Formal
            || cold.verdict != ReviewVerdict::Pass
            || formal.verdict != ReviewVerdict::Pass
            || formal.cold_review_id.as_deref() != Some(cold.review_id.as_str())
            || cold.reviewer == formal.reviewer
            || [cold, formal].iter().any(|r| {
                r.revision != binding.revision
                    || r.plan_digest != binding.plan_digest
                    || r.evidence_identity != binding.evidence_identity
            })
        {
            return Err(broken("supporting reviews are not a distinct passing pair"));
        }
        if binding.session_id != grant.decision.session_id
            || binding.revision != grant.revision
            || binding.plan_digest != grant.plan_digest
            || binding.review_snapshot_digest != grant.review_snapshot_digest
            || binding.cold_review_id != grant.cold_review_id
            || binding.formal_review_id != grant.formal_review_id
            || digest_from(
                id,
                binding.revision,
                &binding.plan_digest,
                &binding.evidence_identity,
                &binding.cold_review_id,
                &binding.formal_review_id,
            ) != grant.review_snapshot_digest
        {
            return Err(broken("grant, challenge and recomputed snapshot disagree"));
        }
        let challenge = self
            .session
            .read_requester_challenge(&grant.decision.session_id, &grant.challenge_id)
            .map_err(session_error)?;
        let request = &challenge.request;
        if request.mission_id != id
            || request.mission_root != self.workspace_root
            || request.session_id != binding.session_id
            || request.revision != binding.revision
            || request.plan_digest != binding.plan_digest
            || request.review_snapshot_digest != binding.review_snapshot_digest
        {
            return Err(conflict(
                "Session challenge disagrees with Mission journal/root",
            ));
        }
        Ok(())
    }
    pub fn check_entry(&self, id: Uuid, path: EntryPath) -> Result<ApprovalGrant, ApprovalError> {
        self.guard_entry(id, path).map(|guard| guard.grant.clone())
    }
    pub fn guard_entry(&self, id: Uuid, _path: EntryPath) -> Result<EntryGuard<'_>, ApprovalError> {
        let tx = self.begin(id)?;
        let grant = self.effective_grant(&tx)?;
        Ok(EntryGuard { _tx: tx, grant })
    }
}
