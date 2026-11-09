//! Eligibility consumer of read-only provider evidence. No provider runtime.
use crate::*;

pub(crate) struct EvidenceGate<'a> {
    pub workspace: &'a Path,
    pub admission: &'a EvidenceAdmissionV4,
}

fn api(error: impl std::fmt::Display) -> LifecycleError {
    control_error("provider_authority_unavailable", error.to_string())
}

impl EvidenceGate<'_> {
    pub(crate) fn binding(
        &self,
        tx: &ApprovalTransaction<'_>,
        waypoint_id: &str,
    ) -> Result<ProviderEvidenceBindingV4, LifecycleError> {
        let plan = tx.review_plan().map_err(api)?;
        let waypoint = plan
            .waypoints
            .iter()
            .find(|w| w.waypoint_id == waypoint_id)
            .ok_or_else(|| control_error("waypoint_absent", waypoint_id))?;
        let contract = waypoint.execution_contract.as_ref().ok_or_else(|| {
            control_error(
                "approval_policy_upgrade_required",
                "v4 execution contract required",
            )
        })?;
        let provider = crate::intent::SessionIntentProvider::new(self.workspace).map_err(api)?;
        let intent = tx
            .effective_intent(&provider)
            .map_err(api)?
            .ok_or_else(|| {
                control_error(
                    "approval_policy_upgrade_required",
                    "confirmed intent required",
                )
            })?;
        let root = self.workspace.to_string_lossy().into_owned();
        let environment_digest = self
            .admission
            .current_environment(&root, &contract.environment.profile)?;
        let mut prerequisites = BTreeMap::new();
        let state = tx.control_state()?;
        for dependency in &waypoint.depends_on {
            let completion = state
                .completions
                .iter()
                .rev()
                .find(|c| {
                    c.current && c.waypoint_id == *dependency && c.plan_digest == plan.plan_digest
                })
                .ok_or_else(|| control_error("prerequisite_evidence_required", dependency))?;
            let resolved = self.completion(tx, dependency, &completion.result_ref)?;
            self.require_pinned(tx, &resolved)?;
            let digest = resolved.record.handover.as_ref().map_or_else(
                || resolved.content_sha256.clone(),
                |a| a.content_sha256.clone(),
            );
            prerequisites.insert(dependency.clone(), digest);
            let producer = plan
                .waypoints
                .iter()
                .find(|w| w.waypoint_id == *dependency)
                .and_then(|w| w.execution_contract.as_ref());
            if let Some(handover) = producer.and_then(|c| c.handover.as_ref())
                && handover.consumer.waypoint_id == waypoint_id
            {
                let artifact = resolved.record.handover.as_ref().ok_or_else(|| {
                    control_error("handover_evidence_required", "producer artifact absent")
                })?;
                validate_handover_artifact(
                    artifact,
                    handover,
                    self.workspace,
                    &resolved.record.binding.contract_digest,
                    waypoint.execution_contract_digest.as_deref().unwrap_or(""),
                )?;
            }
        }
        let route = contract.provider_route.as_ref();
        Ok(ProviderEvidenceBindingV4 {
            mission_id: tx.mission_id(),
            owning_root: root,
            provider: route
                .map_or("mission-validation", |r| r.provider.as_str())
                .into(),
            provider_version: route.map_or("4", |r| r.version.as_str()).into(),
            route_id: route.map_or("automated-checks", |r| r.id.as_str()).into(),
            receipt_class: route
                .map_or("completion-v4", |r| r.receipt_class.as_str())
                .into(),
            profile: contract.environment.profile.clone(),
            environment_digest,
            source_digest: intent.baseline.source_sha256,
            intent_chain_digest: intent.chain_sha256,
            plan_digest: plan.plan_digest,
            waypoint_id: waypoint_id.into(),
            contract_digest: waypoint.execution_contract_digest.clone().ok_or_else(|| {
                control_error("provider_digest_mismatch", "contract digest absent")
            })?,
            prerequisite_artifact_digests: prerequisites,
        })
    }

    pub(crate) fn preflight(
        &self,
        tx: &ApprovalTransaction<'_>,
        waypoint_id: &str,
    ) -> Result<(), LifecycleError> {
        let plan = tx.review_plan().map_err(api)?;
        let waypoint = plan
            .waypoints
            .iter()
            .find(|w| w.waypoint_id == waypoint_id)
            .ok_or_else(|| control_error("waypoint_absent", waypoint_id))?;
        let contract = waypoint.execution_contract.as_ref().ok_or_else(|| {
            control_error("approval_policy_upgrade_required", "v4 contract required")
        })?;
        // Explicit no-runtime routes retain entry eligibility. They are not an
        // exemption from completion or downstream artifact evidence.
        let consumes_handover = plan
            .waypoints
            .iter()
            .filter_map(|w| {
                w.execution_contract
                    .as_ref()
                    .and_then(|c| c.handover.as_ref())
            })
            .any(|h| h.consumer.waypoint_id == waypoint_id);
        if contract.provider_route.is_none() && !consumes_handover {
            return Ok(());
        }
        let binding = self.binding(tx, waypoint_id)?;
        if contract.provider_route.is_some() {
            let verified = self.admission.preflight(&binding, contract)?;
            self.pin(tx, &verified)?;
        }
        Ok(())
    }

    pub(crate) fn entry(&self, tx: &ApprovalTransaction<'_>) -> Result<(), LifecycleError> {
        let plan = tx.review_plan().map_err(api)?;
        let state = tx.control_state()?;
        for waypoint in &plan.waypoints {
            if state.completions.iter().any(|c| {
                c.current
                    && c.waypoint_id == waypoint.waypoint_id
                    && c.plan_digest == plan.plan_digest
            }) {
                continue;
            }
            if waypoint.depends_on.iter().all(|d| {
                state
                    .completions
                    .iter()
                    .any(|c| c.current && c.waypoint_id == *d && c.plan_digest == plan.plan_digest)
            }) {
                self.preflight(tx, &waypoint.waypoint_id)?;
            }
        }
        Ok(())
    }

    pub(crate) fn completion(
        &self,
        tx: &ApprovalTransaction<'_>,
        waypoint_id: &str,
        receipt: &str,
    ) -> Result<VerifiedEvidenceV4, LifecycleError> {
        let binding = self.binding(tx, waypoint_id)?;
        let plan = tx.review_plan().map_err(api)?;
        let waypoint = plan
            .waypoints
            .iter()
            .find(|w| w.waypoint_id == waypoint_id)
            .ok_or_else(|| control_error("waypoint_absent", waypoint_id))?;
        let contract = waypoint.execution_contract.as_ref().ok_or_else(|| {
            control_error("approval_policy_upgrade_required", "v4 contract required")
        })?;
        let provider = crate::intent::SessionIntentProvider::new(self.workspace).map_err(api)?;
        let intent = tx
            .effective_intent(&provider)
            .map_err(api)?
            .ok_or_else(|| {
                control_error(
                    "approval_policy_upgrade_required",
                    "confirmed intent required",
                )
            })?;
        // Conservatively require every confirmed behavior requirement to allow
        // mocks. Editable contract/review/caller policy cannot authorize them.
        let behavior: Vec<_> = intent
            .requirements
            .iter()
            .filter(|r| r.category == IntentOutcomeCategory::WorkingBehavior)
            .collect();
        let allow_mock = !behavior.is_empty()
            && behavior.iter().all(|r| {
                matches!(
                    r.evidence_policy,
                    IntentEvidencePolicy::ExecutableBehavior { allow_mock: true }
                )
            });
        let verified = self.admission.completion(
            &ProviderEvidenceRefV4 {
                receipt: receipt.into(),
            },
            &binding,
            contract,
            allow_mock,
        )?;
        if let Some(handover) = &contract.handover
            && handover.producer.waypoint_id == waypoint_id
        {
            let artifact = verified.record.handover.as_ref().ok_or_else(|| {
                control_error("handover_evidence_required", "declared artifact absent")
            })?;
            let consumer = plan
                .waypoints
                .iter()
                .find(|w| w.waypoint_id == handover.consumer.waypoint_id)
                .ok_or_else(|| control_error("handover_contract_mismatch", "consumer absent"))?;
            validate_handover_artifact(
                artifact,
                handover,
                self.workspace,
                &binding.contract_digest,
                consumer.execution_contract_digest.as_deref().unwrap_or(""),
            )?;
        } else if verified.record.handover.is_some() {
            return Err(control_error(
                "handover_contract_mismatch",
                "undeclared producer artifact",
            ));
        }
        self.check_pin(tx, &verified)?;
        Ok(verified)
    }

    fn pin_name(verified: &VerifiedEvidenceV4) -> Result<String, LifecycleError> {
        let identity = serde_json::to_vec(&(
            "mission-provider-receipt/v4",
            &verified.record.binding.provider,
            &verified.record.binding.provider_version,
            &verified.record.reference,
        ))
        .map_err(api)?;
        Ok(format!(
            "runtime-evidence/{}.sha256",
            evidence_content_digest(&identity)
        ))
    }

    fn check_pin(
        &self,
        tx: &ApprovalTransaction<'_>,
        verified: &VerifiedEvidenceV4,
    ) -> Result<(), LifecycleError> {
        if let Some(bytes) = tx.read_record(&Self::pin_name(verified)?).map_err(api)?
            && bytes != verified.content_sha256.as_bytes()
        {
            return Err(control_error(
                "provider_replay_divergence",
                "opaque receipt changed content",
            ));
        }
        Ok(())
    }

    fn require_pinned(
        &self,
        tx: &ApprovalTransaction<'_>,
        verified: &VerifiedEvidenceV4,
    ) -> Result<(), LifecycleError> {
        self.check_pin(tx, verified)?;
        if tx
            .read_record(&Self::pin_name(verified)?)
            .map_err(api)?
            .is_none()
        {
            return Err(control_error(
                "completion_evidence_required",
                "legacy completion has no verified provider evidence",
            ));
        }
        Ok(())
    }

    pub(crate) fn pin(
        &self,
        tx: &ApprovalTransaction<'_>,
        verified: &VerifiedEvidenceV4,
    ) -> Result<(), LifecycleError> {
        self.check_pin(tx, verified)?;
        let name = Self::pin_name(verified)?;
        if tx.read_record(&name).map_err(api)?.is_none() {
            tx.write_record_new(&name, verified.content_sha256.as_bytes())
                .map_err(api)?;
        }
        Ok(())
    }

    pub(crate) fn completed(&self, tx: &ApprovalTransaction<'_>) -> Result<(), LifecycleError> {
        let plan = tx.review_plan().map_err(api)?;
        for completion in tx
            .control_state()?
            .completions
            .iter()
            .filter(|c| c.current && c.plan_digest == plan.plan_digest)
        {
            let verified = self.completion(tx, &completion.waypoint_id, &completion.result_ref)?;
            self.require_pinned(tx, &verified)?;
        }
        Ok(())
    }
}
