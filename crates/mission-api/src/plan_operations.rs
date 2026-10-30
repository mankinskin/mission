//! Revision-safe planning and deterministic review projections.
//! Operator evidence is bound to a semantic plan, never requester approval.

use super::*;
use sha2::{Digest, Sha256};
use std::cell::RefCell;

pub(super) const REVIEW_PLAN_FILE: &str = "review-plan.json";
pub(super) const EVIDENCE_FILE: &str = "evidence-bindings.json";
const REQUEST_DIR: &str = "plan-requests";
const MAX_REQUEST_ID: usize = 128;
const SNAPSHOT_RETRIES: u32 = 8;

/// A requirement and its owning waypoints; ownership is replaced wholesale.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequirement {
    pub requirement: Requirement,
    #[serde(default)]
    pub owner_waypoint_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanOperation {
    SetGoal {
        title: String,
        objective: String,
    },
    CreateWaypoint {
        waypoint: TypedWaypoint,
        #[serde(default)]
        after: Option<String>,
    },
    UpdateWaypoint {
        waypoint: TypedWaypoint,
    },
    /// Referenced work requires a replacement, which inherits requirement ownership.
    RemoveWaypoint {
        waypoint_id: String,
        #[serde(default)]
        replacement: Option<TypedWaypoint>,
    },
    ReorderWaypoints {
        order: Vec<String>,
    },
    SetRequirements {
        requirements: Vec<PlanRequirement>,
    },
    /// Append operator evidence bound to the current semantic plan.
    RecordEvidence {
        plan_digest: String,
        result: OperatorResult,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanOperationRequest {
    pub request_id: String,
    pub expected_revision: u64,
    pub operation: PlanOperation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanOperationOutcome {
    pub revision: u64,
    pub plan_digest: String,
    #[serde(default)]
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvidence {
    pub outcome: String,
    pub operator_command: String,
    pub evidence_pointer: String,
    /// None for evidence recorded outside plan operations.
    pub bound_plan_digest: Option<String>,
    pub current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewWaypoint {
    pub position: usize,
    pub waypoint_id: String,
    pub title: String,
    pub prompt: String,
    pub part: Document,
    pub commit_checkpoint: CommitCheckpoint,
    pub depends_on: Vec<String>,
    pub non_goals: Vec<String>,
    pub references: Vec<String>,
    pub requirement_ids: Vec<String>,
    pub validation: Vec<ValidationGate>,
    /// Declared gates are never executed by planning.
    pub validation_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_plan: Option<WaypointOutcomePlan>,
    /// Evidenced means bound operator evidence, not formal review.
    pub review_status: String,
    pub evidence: Vec<ReviewEvidence>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewPlan {
    pub mission_id: Uuid,
    pub schema_version: u32,
    pub revision: u64,
    pub title: String,
    pub objective: String,
    pub requirements: Vec<Requirement>,
    pub waypoints: Vec<ReviewWaypoint>,
    /// Semantic plan identity, excluding Mission ID, revision and evidence.
    pub plan_digest: String,
    pub evidence_identity: String,
    /// Requester review is never implied by this projection.
    pub formal_review: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceLedger {
    #[serde(default)]
    pub(super) entries: Vec<EvidenceBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) nonsemantic_revisions: Vec<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EvidenceBinding {
    pub(super) result_sha256: String,
    pub(super) plan_digest: String,
    pub(super) revision: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestRecord {
    schema_version: u32,
    mission_id: Uuid,
    request_id: String,
    request_sha256: String,
    revision: u64,
    plan_digest: String,
}

#[derive(Default)]
pub(super) struct PlanWrite<'a> {
    pub(super) adjust: Option<&'a dyn Fn(&mut MissionBundle) -> Result<(), MissionError>>,
    #[allow(clippy::type_complexity)]
    pub(super) extras:
        Option<&'a dyn Fn(&AcceptedMission) -> Result<Vec<(String, Vec<u8>)>, MissionError>>,
}

#[derive(Serialize)]
struct DigestInput<'a> {
    schema_version: u32,
    title: &'a str,
    objective: &'a str,
    requirements: &'a [Requirement],
    waypoints: Vec<DigestWaypoint<'a>>,
}

#[derive(Serialize)]
struct DigestWaypoint<'a> {
    waypoint_id: &'a str,
    title: &'a str,
    prompt: &'a str,
    part: &'a Document,
    depends_on: &'a [String],
    validation: &'a [ValidationGate],
    commit_checkpoint: &'a CommitCheckpoint,
    non_goals: &'a [String],
    requirement_ids: &'a [String],
    references: &'a [String],
    outcome_plan: Option<&'a WaypointOutcomePlan>,
}

mod helpers;
mod lifecycle;

pub(super) use helpers::*;
