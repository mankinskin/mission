//! Internal mission roadmap bundle, durable store, and deterministic renderer.
//!
//! This crate deliberately has no transport dependencies.  The future public
//! `mission` crate is responsible for loading files and publishing projections.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod operator_results;
mod outcome_plan;
pub use outcome_plan::*;
mod output_history;
pub use output_history::*;
mod approval_transaction;
mod artifact_contract;
pub mod lifecycle;
mod lifecycle_facade;
mod mission_status;
mod mission_store;
mod plan_operations;
mod render;
mod repair;
mod review_entry;
mod schema_renderers;
mod typed_migration;
mod typed_source;
mod validation;
mod waypoint_relationships;

pub use approval_transaction::ApprovalTransaction;
pub use artifact_contract::{
    ARTIFACT_PROVENANCE_SCHEMA_VERSION, MissionArtifactProvenance, MissionFileFormat,
    MissionFileProvenance, MissionFileRole, resolve_artifact_reference,
};
pub use lifecycle::*;
pub use plan_operations::{
    PlanOperation, PlanOperationOutcome, PlanOperationRequest, PlanRequirement, ReviewEvidence,
    ReviewPlan, ReviewWaypoint,
};
pub use repair::*;
pub use schema_renderers::*;
pub(crate) use schema_renderers::{contract_diag, one_diagnostic};
pub use typed_migration::*;
pub use typed_source::TicketLinkLock;
pub use typed_source::*;
pub use waypoint_relationships::*;

pub use render::{AcceptedMission, LabelMapping, RenderedRoadmap, render, render_with_snapshot};
pub(crate) use render::{escape, render_with_consumer};
pub use validation::{collect_bundle, validate_bundle};
pub(crate) use validation::{
    diag, first_incomplete_state, mission_reference, mission_reference_id, validate_manifest_shape,
};
/// Legacy native authoring version; v2 requires explicit authored outcome plans.
pub const SCHEMA_VERSION: u32 = 1;
/// Legacy renderer retained byte-for-byte; native v2 uses mission-api/6.
pub const RENDERER_VERSION: &str = "mission-api/5";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub schema_version: u32,
    pub revision: u64,
    pub title: String,
    pub objective: String,
    pub waypoint_files: Vec<String>,
    pub execution_order: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub validation_gates: Vec<ValidationGate>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// Operator-supplied evidence. Recording this data never executes it.
    #[serde(default)]
    pub operator_results: Vec<OperatorResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorResult {
    pub waypoint_id: String,
    pub outcome: String,
    pub operator_command: String,
    pub evidence_pointer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub deferred: bool,
    /// References only; specification criteria remain authoritative in Spec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canonical_criteria: Vec<CriterionReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidationGate {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaypointFragment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_plan: Option<WaypointOutcomePlan>,
    #[serde(alias = "id")]
    pub waypoint_id: String,
    pub title: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub mode: WaypointMode,
    pub scope: Scope,
    #[serde(default)]
    pub state: Option<WaypointState>,
    #[serde(default)]
    pub ticket_ref: Option<String>,
    /// Canonical reference to an accepted child mission.  This is deliberately
    /// separate from ticket ownership and from the execution scope.
    #[serde(default)]
    pub child_mission_ref: Option<String>,
    #[serde(default, skip_serializing_if = "WaypointRelationships::is_empty")]
    pub relationships: WaypointRelationships,
    pub session_package: String,
    pub prompt: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub requirement_ids: Vec<String>,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub validation: Vec<ValidationGate>,
    pub commit_checkpoint: CommitCheckpoint,
    pub part_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaypointMode {
    ExecutionSideEffect,
    Research,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Scope {
    SingleSession,
    TicketBacked { ticket_ref: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaypointState {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommitCheckpoint {
    pub message: String,
}

/// Complete local waypoint content supplied by the transport facade.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectWaypoint {
    pub waypoint_id: String,
    pub title: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub prompt: String,
    pub validation: Vec<ValidationGate>,
    pub commit_checkpoint: CommitCheckpoint,
    #[serde(default)]
    pub non_goals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionBundle {
    pub manifest: BundleManifest,
    pub waypoints: Vec<WaypointFragment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub source: String,
    pub pointer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waypoint_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum MissionError {
    #[error("bundle validation failed ({0} diagnostic(s))")]
    Validation(usize, Vec<Diagnostic>),
    #[error("stale revision: expected current revision {expected}, found {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("mission {0} was not found")]
    NotFound(Uuid),
    #[error("publication recovery is required for mission {mission_id}: {message}")]
    PublicationRecovery { mission_id: Uuid, message: String },
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl MissionError {
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Validation(_, diagnostics) => Some(diagnostics),
            _ => None,
        }
    }
}

/// Collect a manifest and exactly its declared, safe, manifest-relative fragments.
pub struct MissionStore {
    root: PathBuf,
    #[cfg(test)]
    test_event_metadata: Option<(Uuid, u64)>,
}

/// File-backed lifecycle boundary used by the shared Mission facade.
pub struct MissionLifecycle {
    store: MissionStore,
}

/// The API-owned identity allocated for a create operation and its outcome.
///
/// A rejected create retains this identity for callers that report the attempted
/// mission, while no accepted record is written.
pub struct CreateMissionResult {
    pub mission_id: Uuid,
    pub result: Result<AcceptedMission, MissionError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waypoint(id: &str, depends_on: Vec<String>) -> WaypointFragment {
        WaypointFragment {
            outcome_plan: None,
            waypoint_id: id.into(),
            title: id.into(),
            depends_on,
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            relationships: WaypointRelationships::default(),
            child_mission_ref: None,
            session_package: id.into(),
            prompt: "implement".into(),
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: vec![],
            validation: vec![ValidationGate {
                command: "cargo test".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "checkpoint".into(),
            },
            part_markdown: "## Outcome".into(),
        }
    }
    fn roadmap(revision: u64) -> MissionBundle {
        MissionBundle {
            manifest: BundleManifest {
                schema_version: SCHEMA_VERSION,
                revision,
                title: "Mission".into(),
                objective: "Objective".into(),
                waypoint_files: vec![],
                execution_order: vec!["first".into(), "second".into()],
                artifacts: vec![],
                requirements: vec![],
                validation_gates: vec![],
                notes: vec![],
                operator_results: vec![],
            },
            waypoints: vec![
                waypoint("first", vec![]),
                waypoint("second", vec!["first".into()]),
            ],
        }
    }

    #[test]
    fn published_waypoints_may_have_empty_parts_when_other_required_content_is_present() {
        let mut bundle = roadmap(1);
        bundle.waypoints[0].part_markdown.clear();

        assert!(
            validate_bundle(&bundle).is_ok(),
            "Part content is optional; required waypoint metadata and validation gates remain enforced"
        );
    }

    #[test]
    fn store_readback_preserves_manifest_and_history_mapping() {
        let directory = tempfile::tempdir().unwrap();
        let store = MissionStore::init(directory.path()).unwrap();
        let mission_id = Uuid::new_v4();
        store.import(mission_id, roadmap(1), 0).unwrap();
        let mut second = roadmap(2);
        second.manifest.execution_order = vec!["second".into(), "first".into()];
        second.waypoints[0].depends_on = vec!["second".into()];
        second.waypoints[1].depends_on = vec![];
        store.import(mission_id, second, 1).unwrap();
        assert_eq!(store.get(mission_id).unwrap().bundle.manifest.revision, 2);
        assert_eq!(
            store.revision_mapping(mission_id, 1).unwrap().labels["W1"],
            "first"
        );
    }
    #[test]
    fn shared_writer_boundary_refuses_unauthorized_in_progress_entry() {
        let directory = tempfile::tempdir().unwrap();
        let store = MissionStore::init(directory.path()).unwrap();
        let id = Uuid::new_v4();
        store.import(id, roadmap(1), 0).unwrap();
        let mut entering = store.get(id).unwrap();
        entering.bundle.waypoints[1].state = Some(WaypointState::InProgress);
        let code = |result: Result<(), MissionError>| {
            result.unwrap_err().diagnostics().unwrap()[0]
                .code
                .to_string()
        };
        assert_eq!(
            code(store.write_authority(&entering, None, None)),
            "approval_required"
        );
        assert_eq!(
            code(store.write_authority_with_extras(&entering, None, None, &[])),
            "approval_required"
        );
        assert_eq!(
            code(store.write_authority_with_inputs(&entering, None, None, None)),
            "approval_required"
        );
        assert_eq!(
            code(store.write_authority_for_repair(&entering, None, None, false)),
            "approval_required"
        );
        assert_eq!(
            store.get(id).unwrap().bundle.waypoints[1].state,
            Some(WaypointState::Pending)
        );
    }

    #[test]
    fn roadmap_render_is_pure_and_returns_generated_bytes() {
        let roadmap = roadmap(1);
        assert_eq!(
            render(&roadmap, Uuid::nil()).unwrap().roadmap,
            render(&roadmap, Uuid::nil()).unwrap().roadmap
        );
    }

    #[test]
    fn import_does_not_replace_a_corrupt_current_record() {
        let directory = tempfile::tempdir().unwrap();
        let store = MissionStore::init(directory.path()).unwrap();
        let mission_id = Uuid::new_v4();
        store.import(mission_id, roadmap(1), 0).unwrap();
        let accepted_path = store
            .root
            .join(mission_id.to_string())
            .join("accepted.json");
        fs::write(&accepted_path, "{").unwrap();

        assert!(matches!(
            store.import(mission_id, roadmap(2), 1),
            Err(MissionError::Json(_))
        ));
        assert_eq!(fs::read_to_string(accepted_path).unwrap(), "{");
    }
}
