//! Contracts opt into typed-source v4/native-bundle v3. This preserves legacy
//! v2/v3 source and v1/v2 bundle bytes and digests when the field is absent.
use super::*;
use sha2::{Digest, Sha256};

pub const EXECUTION_CONTRACT_SCHEMA_VERSION: u32 = 4;
pub const CONTRACT_BUNDLE_SCHEMA_VERSION: u32 = 3;
pub const CONTRACT_SOURCE_SCHEMA_VERSION: u32 = 4;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaypointExecutionContractV4 {
    pub schema_version: u32,
    pub outcome_category: WaypointOutcomeCategoryV4,
    pub expected_outcome: String,
    pub work_kind: WaypointWorkKind,
    pub rationale: String,
    pub authoritative_context: Vec<ExecutionContextRefV4>,
    pub boundary: ExecutionBoundaryV4,
    pub declared_dependency_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_dependencies_reason: Option<String>,
    pub preconditions: Vec<ExecutionConditionV4>,
    pub postconditions: Vec<ExecutionConditionV4>,
    pub checks: Vec<ExecutionCheckV4>,
    pub evidence_requirements: Vec<ExecutionEvidenceRequirementV4>,
    pub regression_check_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_regression_checks_reason: Option<String>,
    pub tool_policy: ExecutionToolPolicyV4,
    pub environment: ExecutionEnvironmentV4,
    pub failure_policy: ExecutionFailurePolicyV4,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handover: Option<ExecutionHandoverV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_handover_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_route: Option<ExecutionProviderRouteV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_provider_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(test, allow(dead_code))]
#[serde(deny_unknown_fields)]
pub struct WaypointScopeFindingV4 {
    pub waypoint_id: String,
    pub execution_contract_digest: String,
    pub outcome_count: usize,
    pub completeness: ScopeCriterionFindingV4,
    pub compactness: ScopeCriterionFindingV4,
    pub approachability: ScopeCriterionFindingV4,
    pub single_coherent_outcome: ScopeCriterionFindingV4,
    pub single_agent_one_sitting: ScopeCriterionFindingV4,
    pub boundary_task: String,
    pub boundary_paths: Vec<String>,
    pub context_ids: Vec<String>,
    pub acceptance_check_ids: Vec<String>,
    pub rationale: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(test, allow(dead_code))]
#[serde(rename_all = "snake_case")]
pub enum ScopeCriterionVerdictV4 {
    Pass,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(test, allow(dead_code))]
#[serde(deny_unknown_fields)]
pub struct ScopeCriterionFindingV4 {
    pub verdict: ScopeCriterionVerdictV4,
    pub rationale: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WaypointOutcomeCategoryV4 {
    WorkingBehavior,
    Artifact,
    Decision,
    ResearchReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionContextRefV4 {
    pub id: String,
    pub reference: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionBoundaryV4 {
    pub task: String,
    pub paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionConditionV4 {
    pub id: String,
    pub description: String,
    pub check_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionCheckKindV4 {
    Preflight,
    Acceptance,
    Regression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEvidenceClassV4 {
    ExecutableBehavior,
    DeterministicArtifact,
    CapturedDecision,
    ResearchRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCheckV4 {
    pub id: String,
    pub command: String,
    pub cwd: String,
    pub kind: ExecutionCheckKindV4,
    pub evidence_class: ExecutionEvidenceClassV4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEvidenceRequirementV4 {
    pub id: String,
    pub class: ExecutionEvidenceClassV4,
    pub check_ids: Vec<String>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionToolActionV4 {
    FilesystemRead,
    FilesystemWrite,
    CliReadOnly,
    CliMutating,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPermissionV4 {
    Read,
    Write,
    Execute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionToolPolicyV4 {
    pub actions: Vec<ExecutionToolActionV4>,
    pub read_roots: Vec<String>,
    pub write_roots: Vec<String>,
    pub permissions: Vec<ExecutionPermissionV4>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_write_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEnvironmentV4 {
    pub profile: String,
    pub description: String,
    pub preflight_check_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionFailureDispositionV4 {
    Block,
    Escalate,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionFailurePolicyV4 {
    pub trigger: String,
    pub diagnostic_action: String,
    pub max_attempts: u8,
    pub exhaustion: ExecutionFailureDispositionV4,
    pub escalation_target: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionHandoverV4 {
    pub id: String,
    pub producer: ExecutionHandoverEndpointV4,
    pub consumer: ExecutionHandoverEndpointV4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionHandoverEndpointV4 {
    pub waypoint_id: String,
    pub artifact_id: String,
    pub content_reference: String,
    pub schema: String,
    pub root: String,
    pub check_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProviderRouteV4 {
    pub id: String,
    pub provider: String,
    pub version: String,
    pub receipt_class: String,
    pub preflight_check_ids: Vec<String>,
}

/// Digest of the complete contract's ordered, typed JSON representation.
pub fn waypoint_execution_contract_digest(
    contract: &WaypointExecutionContractV4,
) -> Result<String, serde_json::Error> {
    let bytes = serde_json::to_vec(contract)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

mod rendering;
mod validation;
mod validation_details;
pub(crate) use rendering::render_execution_contract;
pub(crate) use validation::{validate_bundle_contracts, validate_contract_set};
