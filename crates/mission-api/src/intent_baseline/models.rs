use super::*;

pub const INTENT_SCHEMA_VERSION: u32 = 4;
pub const INTENT_RECEIPT_PROVIDER: &str = "session-intent-confirmation/4";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IntentOutcomeCategory {
    WorkingBehavior,
    Artifact,
    Decision,
    ResearchReview,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentEvidencePolicy {
    ExecutableBehavior { allow_mock: bool },
    DeterministicArtifact,
    CapturedDecision,
    ResearchRecord,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntentSourceSpan {
    pub source_sha256: String,
    pub start: usize,
    pub end: usize,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BaselineRequirementV4 {
    pub requirement_id: String,
    pub source: IntentSourceSpan,
    pub outcome: String,
    pub category: IntentOutcomeCategory,
    pub evidence_policy: IntentEvidencePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntentReceiptReference {
    pub session_id: String,
    #[schemars(with = "String")]
    pub challenge_id: Uuid,
}

/// Provider readback, never a receipt supplied directly by a transport caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentReceiptV4 {
    pub receipt_id: String,
    pub challenge_id: String,
    pub source_sha256: String,
    pub baseline_sha256: String,
    pub requester_event_id: String,
    pub captured_at: String,
    pub provider_version: String,
}

pub trait IntentReceiptProvider {
    fn confirmed_receipt(
        &self,
        mission_id: Uuid,
        reference: &IntentReceiptReference,
    ) -> Result<IntentReceiptV4, MissionError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentBaselineV4 {
    pub schema_version: u32,
    pub mission_id: Uuid,
    pub source_sha256: String,
    pub requirements: Vec<BaselineRequirementV4>,
    pub baseline_sha256: String,
    pub receipt_ref: IntentReceiptReference,
    pub receipt: IntentReceiptV4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "disposition", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentChangeV4 {
    Exclude {
        prior_requirement_id: String,
    },
    Replace {
        prior_requirement_id: String,
        requirement: BaselineRequirementV4,
    },
    Add {
        requirement: BaselineRequirementV4,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntentAmendmentRequest {
    pub expected_revision: u64,
    pub amendment_id: String,
    pub prior_chain_sha256: String,
    pub source: String,
    pub changes: Vec<IntentChangeV4>,
    pub reference: IntentReceiptReference,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentAmendmentV4 {
    pub schema_version: u32,
    pub amendment_id: String,
    pub sequence: u64,
    pub prior_chain_sha256: String,
    pub source: String,
    pub changes: Vec<IntentChangeV4>,
    pub amendment_sha256: String,
    pub receipt_ref: IntentReceiptReference,
    pub receipt: IntentReceiptV4,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveIntentV4 {
    pub baseline: IntentBaselineV4,
    pub amendments: Vec<IntentAmendmentV4>,
    pub requirements: Vec<BaselineRequirementV4>,
    pub excluded_requirement_ids: Vec<String>,
    pub chain_sha256: String,
}
