//! Read-only evidence boundary. Receipts are opaque provider identifiers, not paths.
use super::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;

mod validation;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderEvidenceRefV4 {
    pub receipt: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePurposeV4 {
    Preflight,
    Completion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderValidityV4 {
    Current,
    Stale,
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderEvidenceBindingV4 {
    #[schemars(with = "String")]
    pub mission_id: Uuid,
    pub owning_root: String,
    pub provider: String,
    pub provider_version: String,
    pub route_id: String,
    pub receipt_class: String,
    pub profile: String,
    pub environment_digest: String,
    pub source_digest: String,
    pub intent_chain_digest: String,
    pub plan_digest: String,
    pub waypoint_id: String,
    pub contract_digest: String,
    pub prerequisite_artifact_digests: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AutomatedCheckEvidenceV4 {
    pub check_id: String,
    pub check_digest: String,
    pub evidence_class: ExecutionEvidenceClassV4,
    pub executed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub mock: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HandoverArtifactV4 {
    pub id: String,
    pub producer: ExecutionHandoverEndpointV4,
    pub consumer: ExecutionHandoverEndpointV4,
    pub producer_contract_digest: String,
    pub consumer_contract_digest: String,
    pub owning_root: String,
    pub content_sha256: String,
    pub content: Vec<u8>,
    /// Provider-owned schema check, in addition to exact declared schema identity.
    pub schema_valid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompletionEvidenceV4 {
    pub schema_version: u32,
    pub reference: ProviderEvidenceRefV4,
    pub purpose: EvidencePurposeV4,
    pub binding: ProviderEvidenceBindingV4,
    pub ready: bool,
    pub validity: ProviderValidityV4,
    pub condition_ids: Vec<String>,
    pub checks: Vec<AutomatedCheckEvidenceV4>,
    pub handover: Option<HandoverArtifactV4>,
    pub diagnostic_reruns: u8,
    pub fix_justification: Option<String>,
}

/// Bytes and their independently returned provider hash, read from its own store/API.
pub struct ResolvedProviderEvidenceV4 {
    pub content: Vec<u8>,
    pub content_sha256: String,
}

/// Implementations must reread current provider state, including revocation.
/// Mission never invokes an executable or accepts transport-supplied records.
pub trait ReadOnlyEvidenceProviderV4: Send + Sync {
    fn environment_digest(
        &self,
        owning_root: &str,
        profile: &str,
    ) -> Result<String, LifecycleError>;
    fn preflight_receipt(
        &self,
        binding: &ProviderEvidenceBindingV4,
    ) -> Result<ProviderEvidenceRefV4, LifecycleError>;
    fn resolve(
        &self,
        reference: &ProviderEvidenceRefV4,
    ) -> Result<ResolvedProviderEvidenceV4, LifecycleError>;
}

/// No production provider has been delivered. Test injection is typed API only;
/// there is deliberately no serialized adapter selector or filesystem fallback.
#[cfg_attr(
    not(feature = "test-evidence-provider"),
    doc = "
The default build must not expose fixture injection:
```compile_fail,E0599
let _ = mission_api::EvidenceAdmissionV4::test_only;
```
"
)]
#[derive(Clone, Default)]
pub struct EvidenceAdmissionV4 {
    test_provider: Option<Arc<dyn ReadOnlyEvidenceProviderV4>>,
}

pub struct VerifiedEvidenceV4 {
    pub record: CompletionEvidenceV4,
    pub content_sha256: String,
}

impl EvidenceAdmissionV4 {
    /// Disposable tests only. CLI/MCP always construct `Default`.
    #[cfg(feature = "test-evidence-provider")]
    pub fn test_only(provider: Arc<dyn ReadOnlyEvidenceProviderV4>) -> Self {
        Self {
            test_provider: Some(provider),
        }
    }

    fn provider(&self) -> Result<&dyn ReadOnlyEvidenceProviderV4, LifecycleError> {
        self.test_provider.as_deref().ok_or_else(|| {
            control_error(
                "capability_unavailable",
                "trusted read-only evidence provider is unavailable; fixtures are not production capabilities",
            )
        })
    }

    pub fn current_environment(
        &self,
        owning_root: &str,
        profile: &str,
    ) -> Result<String, LifecycleError> {
        let digest = self.provider()?.environment_digest(owning_root, profile)?;
        if !validation::is_digest(&digest) {
            return Err(control_error(
                "provider_environment_invalid",
                "environment digest",
            ));
        }
        Ok(digest)
    }

    pub fn preflight(
        &self,
        binding: &ProviderEvidenceBindingV4,
        contract: &WaypointExecutionContractV4,
    ) -> Result<VerifiedEvidenceV4, LifecycleError> {
        let reference = self.provider()?.preflight_receipt(binding)?;
        self.verify(
            &reference,
            binding,
            contract,
            EvidencePurposeV4::Preflight,
            false,
        )
    }

    pub fn completion(
        &self,
        reference: &ProviderEvidenceRefV4,
        binding: &ProviderEvidenceBindingV4,
        contract: &WaypointExecutionContractV4,
        allow_mock: bool,
    ) -> Result<VerifiedEvidenceV4, LifecycleError> {
        self.verify(
            reference,
            binding,
            contract,
            EvidencePurposeV4::Completion,
            allow_mock,
        )
    }

    fn verify(
        &self,
        reference: &ProviderEvidenceRefV4,
        binding: &ProviderEvidenceBindingV4,
        contract: &WaypointExecutionContractV4,
        purpose: EvidencePurposeV4,
        allow_mock: bool,
    ) -> Result<VerifiedEvidenceV4, LifecycleError> {
        if reference.receipt.trim().is_empty() {
            return Err(control_error(
                "provider_receipt_missing",
                "opaque receipt required",
            ));
        }
        let resolved = self.provider()?.resolve(reference)?;
        if !validation::is_digest(&resolved.content_sha256)
            || evidence_content_digest(&resolved.content) != resolved.content_sha256
        {
            return Err(control_error(
                "provider_evidence_corrupt",
                "provider content hash mismatch",
            ));
        }
        let record: CompletionEvidenceV4 = serde_json::from_slice(&resolved.content)
            .map_err(|e| control_error("provider_evidence_corrupt", e.to_string()))?;
        validation::validate(&record, reference, binding, contract, purpose, allow_mock)?;
        Ok(VerifiedEvidenceV4 {
            record,
            content_sha256: resolved.content_sha256,
        })
    }
}

pub fn evidence_content_digest(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

/// Domain-separated ordered typed check JSON; command/cwd/class/kind are bound.
pub fn execution_check_digest(check: &ExecutionCheckV4) -> Result<String, serde_json::Error> {
    serde_json::to_vec(&("mission-automated-check/v4", check))
        .map(|bytes| evidence_content_digest(&bytes))
}

/// Condition links remain authoritative even when a check has another kind.
pub fn applicable_execution_check(
    contract: &WaypointExecutionContractV4,
    check: &ExecutionCheckV4,
    purpose: EvidencePurposeV4,
) -> bool {
    let conditions = match purpose {
        EvidencePurposeV4::Preflight => &contract.preconditions,
        EvidencePurposeV4::Completion => &contract.postconditions,
    };
    conditions.iter().any(|c| c.check_ids.contains(&check.id))
        || match purpose {
            EvidencePurposeV4::Preflight => {
                check.kind == ExecutionCheckKindV4::Preflight
                    || contract.environment.preflight_check_ids.contains(&check.id)
                    || contract
                        .provider_route
                        .as_ref()
                        .is_some_and(|r| r.preflight_check_ids.contains(&check.id))
            }
            EvidencePurposeV4::Completion => {
                check.kind != ExecutionCheckKindV4::Preflight
                    || contract.regression_check_ids.contains(&check.id)
                    || contract
                        .evidence_requirements
                        .iter()
                        .any(|r| r.check_ids.contains(&check.id))
            }
        }
}

/// Exact producer and consumer contract identities, not a filename assertion.
pub fn validate_handover_artifact(
    artifact: &HandoverArtifactV4,
    handover: &ExecutionHandoverV4,
    owning_root: &Path,
    producer_digest: &str,
    consumer_digest: &str,
) -> Result<(), LifecycleError> {
    validation::handover(
        artifact,
        handover,
        owning_root,
        producer_digest,
        consumer_digest,
    )
}

pub(crate) fn validate_provider_receipt_pins(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), MissionError> {
    for (name, bytes) in files {
        if let Some(name) = name.strip_prefix("approvals/runtime-evidence/") {
            let valid = name
                .strip_suffix(".sha256")
                .is_some_and(validation::is_digest)
                && std::str::from_utf8(bytes).is_ok_and(validation::is_digest);
            if !valid {
                return Err(MissionError::Validation(
                    1,
                    vec![contract_diag(
                        "provider-receipt-journal-corrupt",
                        name,
                        "Invalid immutable provider receipt pin.",
                    )],
                ));
            }
        }
    }
    Ok(())
}
