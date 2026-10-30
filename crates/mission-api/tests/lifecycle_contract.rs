//! W1 (MN-W1-01..04) contract tests for the shared lifecycle types, mappings,
//! version-1 control journal and capability traits.

use mission_api::lifecycle::MissionProjection;
use mission_api::{
    Capability, CommitCheckpoint, ControlEvent, ControlKind, ControlRequest, ControlTarget,
    DirectWaypoint, EligibilityDenial, ExecutionEligibility, LIFECYCLE_JOURNAL_VERSION,
    LifecycleError, LifecycleJournal, LifecycleState, MissionFacts, MissionLifecycle,
    PauseResumeCapability, ReopenCapability, TargetControl, ValidationGate, WaypointIdentity,
    WaypointState, project_mission, project_waypoint,
};
use mission_api::{LifecycleSnapshot, LifecycleStateExt, LifecycleSubject};
use tempfile::TempDir;
use uuid::Uuid;

use LifecycleState::*;

#[path = "lifecycle_contract/support.rs"]
mod support;
use support::*;

#[path = "lifecycle_contract/journal.rs"]
mod journal;
#[path = "lifecycle_contract/mappings.rs"]
mod mappings;
#[path = "lifecycle_contract/mission_subject.rs"]
mod mission_subject;
