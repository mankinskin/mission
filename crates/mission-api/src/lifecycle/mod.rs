//! Mission-owned lifecycle identities, journal and domain projections.
mod journal;
mod mapping;
mod projection;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

pub use journal::*;
pub use mapping::LifecycleStateExt;
pub use memory_kernel::lifecycle::{
    ApprovalCapability, BasicLifecycle, CancellationCapability, Capability, CompletionCapability,
    ControlKind, ControlRequest, HandoffCapability, LifecycleRead, LifecycleState,
    LifecycleSubject, LifecycleTransition, PauseResumeCapability, PlanningCapability,
    ReopenCapability, TargetControl, TransitionRequest,
};
pub use projection::*;

/// Version of the Mission-owned lifecycle journal schema.
pub const LIFECYCLE_JOURNAL_VERSION: u32 = 1;
pub const LIFECYCLE_JOURNAL_FILE: &str = "lifecycle-journal.json";

/// Native domain that owns the retained state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeDomain {
    Ticket,
    Mission,
    MissionWaypoint,
}

/// Stable, abstract identity of a lifecycle subject.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WaypointIdentity {
    Ticket {
        ticket_id: Uuid,
    },
    Mission {
        mission_id: Uuid,
    },
    Waypoint {
        mission_id: Uuid,
        waypoint_id: String,
    },
}

/// Why execution is denied. Kept separate from the shared state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", content = "state", rename_all = "kebab-case")]
pub enum EligibilityDenial {
    MissionPaused,
    MissionCancelled,
    State(LifecycleState),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "eligibility", rename_all = "kebab-case")]
pub enum ExecutionEligibility {
    Eligible,
    Denied { denial: EligibilityDenial },
}

impl ExecutionEligibility {
    pub fn is_eligible(&self) -> bool {
        matches!(self, Self::Eligible)
    }

    /// Eligibility from the shared state and the owning Mission's controls.
    /// Mission pause/cancel deny execution without changing the shared state.
    pub fn effective(state: LifecycleState, mission: &TargetControl) -> Self {
        if mission.terminal == Some(LifecycleState::Cancelled) {
            return Self::Denied {
                denial: EligibilityDenial::MissionCancelled,
            };
        }
        if mission.paused_from.is_some() {
            return Self::Denied {
                denial: EligibilityDenial::MissionPaused,
            };
        }
        match state {
            LifecycleState::Planned | LifecycleState::Active => Self::Eligible,
            other => Self::Denied {
                denial: EligibilityDenial::State(other),
            },
        }
    }
}

/// Shared read model with retained native information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleSnapshot {
    pub identity: WaypointIdentity,
    pub state: LifecycleState,
    pub native_domain: NativeDomain,
    pub native_state: String,
    pub native_revision: u64,
    pub owning_root: PathBuf,
    pub control_sequence: u64,
    pub eligibility: ExecutionEligibility,
    /// A legacy all-completed Mission without an explicit completion event.
    pub result_review_needed: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("ticket_state_unmapped: Ticket state '{0}' has no shared lifecycle mapping")]
    TicketStateUnmapped(String),
    #[error("foreign_authority_absent: {0}")]
    ForeignAuthorityAbsent(String),
    #[error("unsupported_capability: {capability} is not supported by {provider}")]
    UnsupportedCapability {
        capability: Capability,
        provider: String,
    },
    #[error("mission_unavailable: {0}")]
    MissionUnavailable(String),
    #[error("lifecycle_journal_corrupt: {0}")]
    JournalCorrupt(String),
    #[error("invalid_lifecycle_schema: {0}")]
    InvalidSchema(String),
    #[error("lifecycle_journal_unsupported_version: found {found}, supported {supported}")]
    JournalUnsupportedVersion { found: String, supported: u32 },
    #[error("lifecycle_journal_io: {0}")]
    JournalIo(String),
    #[error("control_history_invalid: sequence {seq}: {reason}")]
    InvalidControlHistory { seq: u64, reason: String },
    #[error("illegal_transition: {0}")]
    IllegalTransition(String),
    #[error("foreign_transition_rejected: {0}")]
    TransitionRejected(String),
    #[error("approval_gate: {code}: {message}")]
    ApprovalGate { code: &'static str, message: String },
}

impl LifecycleError {
    /// Stable machine-readable diagnostic code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::TicketStateUnmapped(_) => "ticket_state_unmapped",
            Self::ForeignAuthorityAbsent(_) => "foreign_authority_absent",
            Self::UnsupportedCapability { .. } => "unsupported_capability",
            Self::MissionUnavailable(_) => "mission_unavailable",
            Self::JournalCorrupt(_) => "lifecycle_journal_corrupt",
            Self::InvalidSchema(_) => "invalid_lifecycle_schema",
            Self::JournalUnsupportedVersion { .. } => "lifecycle_journal_unsupported_version",
            Self::JournalIo(_) => "lifecycle_journal_io",
            Self::InvalidControlHistory { .. } => "control_history_invalid",
            Self::IllegalTransition(_) => "illegal_transition",
            Self::TransitionRejected(_) => "foreign_transition_rejected",
            Self::ApprovalGate { code, .. } => code,
        }
    }

    pub fn unsupported(capability: Capability, provider: &str) -> Self {
        Self::UnsupportedCapability {
            capability,
            provider: provider.to_string(),
        }
    }
}

impl From<memory_kernel::lifecycle::LifecycleError> for LifecycleError {
    fn from(error: memory_kernel::lifecycle::LifecycleError) -> Self {
        use memory_kernel::lifecycle::LifecycleError as Generic;
        match error {
            Generic::UnsupportedCapability {
                capability,
                provider,
            } => Self::UnsupportedCapability {
                capability,
                provider,
            },
            Generic::IllegalTransition(reason) => Self::IllegalTransition(reason),
            Generic::StateUnmapped(state) => Self::TicketStateUnmapped(state),
            Generic::InvalidSchema(reason) => Self::InvalidSchema(reason),
        }
    }
}
