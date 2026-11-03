use crate::{CriterionReference, TypedWaypoint};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntakeInput {
    pub workspace_root: PathBuf,
    pub session_root: Option<PathBuf>,
    pub session_id: String,
    pub request_id: String,
    pub input: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceInput {
    pub workspace_root: PathBuf,
    pub session_root: Option<PathBuf>,
    pub session_id: String,
    pub request_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadInput {
    pub workspace_root: PathBuf,
    pub session_root: Option<PathBuf>,
    pub mission_id: String,
    pub session_id: Option<String>,
}

macro_rules! mutation {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        mutation!($name [] { $($field: $ty),* });
    };
    ($name:ident [$($request_meta:meta),*] { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            pub workspace_root: PathBuf,
            pub mission_id: String,
            $(#[$request_meta])*
            pub request_id: String,
            pub expected_revision: u64,
            $(pub $field: $ty,)*
        }
    };
}

mutation!(PlanInput {
    operation: PlanEdit
});
mutation!(CreateInput { waypoint: TypedWaypoint, after: Option<String> });
mutation!(UpdateInput {
    waypoint: TypedWaypoint
});
mutation!(RemoveInput { waypoint_id: String, replacement: Option<TypedWaypoint> });
mutation!(ReorderInput { order: Vec<String> });
mutation!(ReviewInput { session_root: Option<PathBuf>, session_id: String });
mutation!(RecordReviewInput {
    session_root: Option<PathBuf>,
    kind: crate::approval::ReviewKind,
    verdict: crate::approval::ReviewVerdict,
    reviewer: String,
    plan_digest: String,
    evidence_identity: String,
    cold_review_id: Option<String>
});
mutation!(DecisionInput { session_root: Option<PathBuf>, decision: DecisionRef });
fn uuid_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"string", "format":"uuid"})
}

mutation!(ControlInput [schemars(schema_with = "uuid_schema")] { expected_control_sequence: u64, evidence: String });
mutation!(CompleteInput [schemars(schema_with = "uuid_schema")] {
    expected_control_sequence: u64, evidence: String, waypoint_id: String
});
mutation!(HandoffInput [schemars(schema_with = "uuid_schema")] {
    expected_control_sequence: u64, evidence: String,
    source_session: String, destination_session: String
});
mutation!(ReopenInput [schemars(schema_with = "uuid_schema")] {
    expected_control_sequence: u64, evidence: String, expected_completion_sequence: u64
});
mutation!(ResultsInput [schemars(schema_with = "uuid_schema")] {
    expected_control_sequence: u64, evidence: String, additions: Vec<Addition>
});

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Addition {
    pub request_id: String,
    pub expected_revision: u64,
    pub waypoint: TypedWaypoint,
    pub after: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionRef {
    pub session_id: String,
    pub challenge_id: String,
    pub decision_id: String,
    pub source_event_id: String,
}

impl From<DecisionRef> for session_api::RequesterDecisionRef {
    fn from(value: DecisionRef) -> Self {
        Self {
            session_id: value.session_id,
            challenge_id: value.challenge_id,
            decision_id: value.decision_id,
            source_event_id: value.source_event_id,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanEdit {
    SetGoal {
        title: String,
        objective: String,
    },
    SetRequirements {
        requirements: Vec<RequirementInput>,
    },
    RecordEvidence {
        plan_digest: String,
        result: EvidenceInput,
    },
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequirementInput {
    pub requirement: RequirementFields,
    #[serde(default)]
    pub owner_waypoint_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequirementFields {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub deferred: bool,
    #[serde(default)]
    pub canonical_criteria: Vec<CriterionReference>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceInput {
    pub waypoint_id: String,
    pub outcome: String,
    pub operator_command: String,
    pub evidence_pointer: String,
}

impl From<PlanEdit> for crate::PlanOperation {
    fn from(value: PlanEdit) -> Self {
        match value {
            PlanEdit::SetGoal { title, objective } => Self::SetGoal { title, objective },
            PlanEdit::SetRequirements { requirements } => Self::SetRequirements {
                requirements: requirements
                    .into_iter()
                    .map(|r| crate::PlanRequirement {
                        requirement: crate::Requirement {
                            id: r.requirement.id,
                            text: r.requirement.text,
                            deferred: r.requirement.deferred,
                            canonical_criteria: r.requirement.canonical_criteria,
                        },
                        owner_waypoint_ids: r.owner_waypoint_ids,
                    })
                    .collect(),
            },
            PlanEdit::RecordEvidence {
                plan_digest,
                result,
            } => Self::RecordEvidence {
                plan_digest,
                result: crate::OperatorResult {
                    waypoint_id: result.waypoint_id,
                    outcome: result.outcome,
                    operator_command: result.operator_command,
                    evidence_pointer: result.evidence_pointer,
                },
            },
        }
    }
}
