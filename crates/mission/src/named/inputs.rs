use crate::{CriterionReference, TypedWaypoint};
use schemars::JsonSchema;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntakeInput {
    /// Existing absolute repository/workspace root owning the Mission store at .workflow-tools/mission. Named tools do not use the server launch directory.
    pub workspace_root: PathBuf,
    /// Optional exact Session store directory, not a repository root. Normally omit (or null) to use workspace_root/.workflow-tools/session; set only to select an explicitly separate store directory. No suffix is appended.
    pub session_root: Option<PathBuf>,
    /// Existing active Session identity returned by Session tooling; intake does not initialize a missing Session.
    pub session_id: String,
    /// Stable nonempty intake request identity. Reuse with identical input and roots for replay; changed input under the same identity conflicts.
    pub request_id: String,
    /// Verbatim requester text, preserved as immutable UTF-8 bytes in Session and managed Mission inputs.
    pub input: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceInput {
    /// Existing absolute repository/workspace root owning the Mission store at .workflow-tools/mission.
    pub workspace_root: PathBuf,
    /// Optional exact Session store directory, not a repository root. Normally omit (or null) to use workspace_root/.workflow-tools/session; set only to select an explicitly separate store directory. No suffix is appended.
    pub session_root: Option<PathBuf>,
    /// Session identity from the original successful mission_intake_create call.
    pub session_id: String,
    /// Original intake request identity; lookup does not create a new Mission.
    pub request_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadInput {
    /// Existing absolute repository/workspace root owning the Mission store at .workflow-tools/mission.
    pub workspace_root: PathBuf,
    /// Optional exact Session store directory, not a repository root. Normally omit (or null) to use workspace_root/.workflow-tools/session; set only to select an explicitly separate store directory. No suffix is appended.
    pub session_root: Option<PathBuf>,
    /// Non-nil Mission UUID returned by intake or an authoritative Mission read, not a waypoint or Session ID.
    pub mission_id: String,
    /// Optional exact Session identity filter for decision_refs only; omit/null returns all captured decision references. Plan, lifecycle, reviews and grants are not filtered.
    pub session_id: Option<String>,
}

macro_rules! mutation {
    ($name:ident { $($(#[$meta:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        mutation!($name [] { $($(#[$meta])* $field: $ty),* });
    };
    ($name:ident [$($request_meta:meta),*] { $($(#[$meta:meta])* $field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            /// Existing absolute repository/workspace root owning .workflow-tools/mission. Lifecycle operations use .workflow-tools/session under this root, not an intake-time separate Session store.
            pub workspace_root: PathBuf,
            /// Non-nil UUID of the stored Mission, obtained from intake or an authoritative read.
            pub mission_id: String,
            /// Stable operation identity: 1..=128 visible ASCII characters, with no spaces. Retry identical payloads only. Lifecycle/control requests require a non-nil UUID. Approval/replan requests must equal decision.challenge_id.
            $(#[$request_meta])*
            pub request_id: String,
            /// Exact current Mission plan revision from mission_plan_get; stale writes are rejected. Review/approval binds to this revision.
            pub expected_revision: u64,
            $($(#[$meta])* pub $field: $ty,)*
        }
    };
}

mutation!(PlanInput {
    /// Tagged edit: set_goal replaces title/objective; set_requirements replaces the requirement collection; record_evidence records a result bound to plan_digest without running its command.
    operation: PlanEdit
});
mutation!(CreateInput {
    /// Complete typed waypoint, including stable local ID and matching Part identity; not a filename or Mission UUID.
    waypoint: TypedWaypoint,
    /// Optional existing local waypoint ID to insert after; omit/null appends at the end.
    after: Option<String>
});
mutation!(UpdateInput {
    /// Complete replacement of the existing waypoint identified by waypoint.waypoint_id; preserve its stable local identity.
    waypoint: TypedWaypoint
});
mutation!(RemoveInput {
    /// Existing stable local waypoint ID to remove, not a route position or file path.
    waypoint_id: String,
    /// Optional complete replacement waypoint; omit/null removes without replacement, subject to provider validation.
    replacement: Option<TypedWaypoint>
});
mutation!(ReorderInput {
    /// Complete ordered list of stable local waypoint IDs; provider validates serial dependencies, missing IDs and duplicates.
    order: Vec<String>
});
mutation!(ReviewInput {
    /// Optional exact Session store directory, not a repository root. Normally omit (or null) to use workspace_root/.workflow-tools/session; set only to select an explicitly separate store directory. No suffix is appended.
    session_root: Option<PathBuf>,
    /// Existing Session identity that will receive the revision-bound requester challenge; not agent approval authority.
    session_id: String
});
mutation!(RecordReviewInput {
    /// Optional exact Session store directory, not a repository root. Normally omit (or null) to use workspace_root/.workflow-tools/session; set only to select an explicitly separate store directory. No suffix is appended.
    session_root: Option<PathBuf>,
    /// cold_walkthrough or formal; a formal review references a distinct passing current cold review.
    kind: crate::approval::ReviewKind,
    /// pass or fail; a passing review does not authorize publication or execution.
    verdict: crate::approval::ReviewVerdict,
    /// Reviewer identity recorded as evidence, not a requester decision or authority grant.
    reviewer: String,
    /// Digest of the exact reviewed current plan, obtained from the provider; not an arbitrary content label.
    plan_digest: String,
    /// Nonempty identity/location of review evidence retained by the reviewer.
    evidence_identity: String,
    /// Required for formal review: ID of its passing current cold_walkthrough; omit/null for a cold review.
    cold_review_id: Option<String>,
    /// One source-to-outcome disposition per confirmed baseline/amendment requirement.
    #[serde(default)]
    goal_coverage_findings: Vec<crate::approval::GoalCoverageFindingV4>,
    /// Source, intent-chain and plan-bound conclusion for this independent review.
    #[serde(default)]
    terminal_outcome_finding: Option<crate::approval::TerminalOutcomeFindingV4>,
    /// One complete contract-digest-bound scope audit for every current v4 waypoint.
    #[serde(default)]
    scope_findings: Vec<crate::WaypointScopeFindingV4>
});
mutation!(DecisionInput {
    /// Optional exact Session store directory, not a repository root. Normally omit (or null) to use workspace_root/.workflow-tools/session; set only to select an explicitly separate store directory. No suffix is appended.
    session_root: Option<PathBuf>,
    /// Exact resolvable Session host receipt reference. All four identities must match captured evidence; raw decisions, actor text and booleans are not accepted.
    decision: DecisionRef
});
fn uuid_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"string", "format":"uuid"})
}

mutation!(ControlInput [schemars(schema_with = "uuid_schema")] {
    /// Exact current lifecycle control_sequence from mission_plan_get; stale controls are rejected.
    expected_control_sequence: u64,
    /// Nonempty reason/outcome evidence for this control operation; does not itself grant approval.
    evidence: String
});
mutation!(CompleteInput [schemars(schema_with = "uuid_schema")] {
    /// Exact current lifecycle control_sequence from mission_plan_get.
    expected_control_sequence: u64,
    /// Nonempty completion evidence, checked with approval, prerequisites and linked Ticket state.
    evidence: String,
    /// Existing stable local waypoint ID being completed, not its route position.
    waypoint_id: String
});
mutation!(ActivateInput [schemars(schema_with = "uuid_schema")] {
    /// Existing stable local waypoint ID whose linked Ticket is activated. Single-session and child-Mission waypoints are unsupported.
    waypoint_id: String,
    /// Optional Ticket history author, not approval authority. Omit/null to use the Ticket provider default.
    author: Option<String>
});
mutation!(HandoffInput [schemars(schema_with = "uuid_schema")] {
    /// Exact current lifecycle control_sequence from mission_plan_get.
    expected_control_sequence: u64,
    /// Nonempty handoff reason/evidence; the provider journals transfer and recovery.
    evidence: String,
    /// Currently bound source Session identity, verified by the lifecycle provider.
    source_session: String,
    /// Existing destination Session identity to receive the active binding and managed pins.
    destination_session: String
});
mutation!(ReopenInput [schemars(schema_with = "uuid_schema")] {
    /// Exact current lifecycle control_sequence from mission_plan_get.
    expected_control_sequence: u64,
    /// Nonempty reason for reopening; linked Ticket transition is audited and approval invalidated.
    evidence: String,
    /// Expected greatest current completion sequence, not the last execution_order position.
    expected_completion_sequence: u64
});
mutation!(ResultsInput [schemars(schema_with = "uuid_schema")] {
    /// Exact current lifecycle control_sequence from mission_plan_get.
    expected_control_sequence: u64,
    /// Nonempty result-review evidence persisted before any waypoint additions.
    evidence: String,
    /// Ordered additions, each with its own request identity and expected plan revision. Partial failures retain the review and earlier successful additions.
    additions: Vec<Addition>
});

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Addition {
    /// Stable request identity: 1..=128 visible ASCII characters, with no spaces. Distinct from the lifecycle review request.
    pub request_id: String,
    /// Exact plan revision immediately before this addition; account for earlier additions.
    pub expected_revision: u64,
    /// Complete new typed waypoint with stable local ID and matching Part.
    pub waypoint: TypedWaypoint,
    /// Optional existing local waypoint ID to insert after; omit/null appends at the end.
    pub after: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DecisionRef {
    /// Session identity in the captured host receipt, not a newly invented requester identity.
    pub session_id: String,
    /// Challenge identity returned by mission_plan_prepare_review; must equal the outer request_id.
    pub challenge_id: String,
    /// Decision identity returned by the Session host capture provider.
    pub decision_id: String,
    /// Source event identity from the same captured Session host receipt.
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
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntentInput {
    /// Existing absolute owning workspace; Session authority uses its .workflow-tools/session store.
    pub workspace_root: PathBuf,
    /// Stored Mission UUID whose original requester intent is being read or confirmed.
    pub mission_id: String,
    /// Strict intent action; mutations require expected_revision and preparation/confirmation reference IDs.
    pub operation: crate::intent::IntentOperation,
}
