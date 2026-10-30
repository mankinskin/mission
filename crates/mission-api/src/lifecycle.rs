//! Shared lifecycle projections, waypoint capability contracts and the
//! version-1 Mission control journal schema.
//!
//! This module owns *types, conversions, validation and projection only*.
//! Ticket state stays in the Ticket store (adapters live in the `mission`
//! facade) and durable control writes/recovery belong to a later slice; nothing
//! here persists lifecycle state or extends the legacy plan manifests.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::WaypointState;

/// Version of the Mission-owned lifecycle journal schema.
pub const LIFECYCLE_JOURNAL_VERSION: u32 = 1;
/// File name of the journal inside a UUID Mission directory.
pub const LIFECYCLE_JOURNAL_FILE: &str = "lifecycle-journal.json";

/// The six shared lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LifecycleState {
    Planned,
    Active,
    Paused,
    Blocked,
    Completed,
    Cancelled,
}

impl LifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    /// States a pause may be taken from and a resume restores.
    pub fn is_resumable(self) -> bool {
        matches!(self, Self::Planned | Self::Active | Self::Blocked)
    }

    /// Maps a Ticket-native state. Unknown states are explicit errors.
    pub fn from_ticket_state(state: &str) -> Result<Self, LifecycleError> {
        match state {
            "planning" | "ready" => Ok(Self::Planned),
            "in-implementation" | "in-review" => Ok(Self::Active),
            "on-hold" => Ok(Self::Paused),
            "blocked" => Ok(Self::Blocked),
            "done" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(LifecycleError::TicketStateUnmapped(other.to_string())),
        }
    }

    /// Maps the legacy single-session native state; paused/cancelled never
    /// come from this enum.
    pub fn from_waypoint_state(state: &WaypointState) -> Self {
        match state {
            WaypointState::Pending => Self::Planned,
            WaypointState::InProgress => Self::Active,
            WaypointState::Completed => Self::Completed,
            WaypointState::Blocked => Self::Blocked,
        }
    }
}

impl fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

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

/// Separately implementable capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Read,
    Transition,
    Planning,
    Approval,
    PauseResume,
    Cancellation,
    Completion,
    ResultReview,
    Handoff,
    Reopen,
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Read => "read",
            Self::Transition => "transition",
            Self::Planning => "planning",
            Self::Approval => "approval",
            Self::PauseResume => "pause-resume",
            Self::Cancellation => "cancellation",
            Self::Completion => "completion",
            Self::ResultReview => "result-review",
            Self::Handoff => "handoff",
            Self::Reopen => "reopen",
        };
        f.write_str(name)
    }
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
            Self::JournalUnsupportedVersion { .. } => "lifecycle_journal_unsupported_version",
            Self::JournalIo(_) => "lifecycle_journal_io",
            Self::InvalidControlHistory { .. } => "control_history_invalid",
            Self::IllegalTransition(_) => "illegal_transition",
            Self::TransitionRejected(_) => "foreign_transition_rejected",
        }
    }

    pub fn unsupported(capability: Capability, provider: &str) -> Self {
        Self::UnsupportedCapability {
            capability,
            provider: provider.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Version-1 control journal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ControlTarget {
    Mission,
    Waypoint { waypoint_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlKind {
    Pause,
    Resume,
    Cancel,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlEvent {
    pub seq: u64,
    pub op_id: Uuid,
    pub target: ControlTarget,
    /// Sequence of the preceding event (0 for the first event).
    pub prior_seq: u64,
    pub event: ControlKind,
    /// Nonterminal state before the event (for resume: the restored state).
    pub prior_state: LifecycleState,
    /// Required for completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Replayed control state for one target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetControl {
    pub paused_from: Option<LifecycleState>,
    pub terminal: Option<LifecycleState>,
    pub last_seq: u64,
}

impl TargetControl {
    fn apply(&mut self, event: &ControlEvent) -> Result<(), String> {
        if let Some(terminal) = self.terminal {
            return Err(format!("event after terminal {terminal} control"));
        }
        let nonterminal = event.prior_state.is_resumable();
        match event.event {
            ControlKind::Pause => {
                if self.paused_from.is_some() {
                    return Err("pause of an already paused target".into());
                }
                if !nonterminal {
                    return Err(format!(
                        "pause requires a nonterminal prior state, got {}",
                        event.prior_state
                    ));
                }
                self.paused_from = Some(event.prior_state);
            }
            ControlKind::Resume => {
                let Some(paused_from) = self.paused_from else {
                    return Err("resume of a target that is not paused".into());
                };
                if event.prior_state != paused_from {
                    return Err(format!(
                        "resume must restore {paused_from}, got {}",
                        event.prior_state
                    ));
                }
                self.paused_from = None;
            }
            ControlKind::Cancel => {
                let expected = self.paused_from.unwrap_or(event.prior_state);
                if !nonterminal || event.prior_state != expected {
                    return Err(format!(
                        "cancel requires nonterminal prior state {expected}, got {}",
                        event.prior_state
                    ));
                }
                self.paused_from = None;
                self.terminal = Some(LifecycleState::Cancelled);
            }
            ControlKind::Complete => {
                if self.paused_from.is_some() {
                    return Err("completion of a paused target is rejected".into());
                }
                if !nonterminal {
                    return Err(format!(
                        "complete requires a nonterminal prior state, got {}",
                        event.prior_state
                    ));
                }
                if event
                    .evidence
                    .as_deref()
                    .is_none_or(|e| e.trim().is_empty())
                {
                    return Err("completion requires non-empty evidence".into());
                }
                self.terminal = Some(LifecycleState::Completed);
            }
        }
        self.last_seq = event.seq;
        Ok(())
    }
}

/// Replayed controls for every target of one Mission.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlProjection {
    pub last_seq: u64,
    targets: BTreeMap<ControlTarget, TargetControl>,
    op_ids: BTreeSet<Uuid>,
}

impl ControlProjection {
    pub fn target(&self, target: &ControlTarget) -> TargetControl {
        self.targets.get(target).cloned().unwrap_or_default()
    }

    fn apply(&mut self, event: &ControlEvent) -> Result<(), LifecycleError> {
        let invalid = |reason: String| LifecycleError::InvalidControlHistory {
            seq: event.seq,
            reason,
        };
        if event.seq == 0 || event.seq <= self.last_seq {
            return Err(invalid(format!("sequence must exceed {}", self.last_seq)));
        }
        if event.prior_seq != self.last_seq {
            return Err(invalid(format!(
                "prior sequence {} does not match preceding sequence {}",
                event.prior_seq, self.last_seq
            )));
        }
        if self.op_ids.contains(&event.op_id) {
            return Err(invalid(format!("duplicate operation id {}", event.op_id)));
        }
        self.targets
            .entry(event.target.clone())
            .or_default()
            .apply(event)
            .map_err(invalid)?;
        self.op_ids.insert(event.op_id);
        self.last_seq = event.seq;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleJournal {
    pub version: u32,
    pub events: Vec<ControlEvent>,
}

impl Default for LifecycleJournal {
    /// The empty journal; equivalent to a legacy Mission without a journal.
    fn default() -> Self {
        Self {
            version: LIFECYCLE_JOURNAL_VERSION,
            events: Vec::new(),
        }
    }
}

impl LifecycleJournal {
    pub fn path(workspace_root: &Path, mission_id: Uuid) -> PathBuf {
        workspace_root
            .join(".workflow-tools")
            .join("mission")
            .join("missions")
            .join(mission_id.to_string())
            .join(LIFECYCLE_JOURNAL_FILE)
    }

    pub fn to_json(&self) -> Result<String, LifecycleError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))
    }

    /// Parses and validates a journal; unsupported versions and corrupt
    /// content are explicit errors.
    pub fn from_json(text: &str) -> Result<Self, LifecycleError> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))?;
        let version = value
            .get("version")
            .ok_or_else(|| LifecycleError::JournalCorrupt("missing version".into()))?;
        if version.as_u64() != Some(u64::from(LIFECYCLE_JOURNAL_VERSION)) {
            return Err(LifecycleError::JournalUnsupportedVersion {
                found: version.to_string(),
                supported: LIFECYCLE_JOURNAL_VERSION,
            });
        }
        let journal: Self = serde_json::from_value(value)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))?;
        journal.projection()?;
        Ok(journal)
    }

    /// Reads the journal of a Mission. A missing file is a legacy Mission with
    /// no controls (not an approval); every other failure is an error.
    pub fn read(workspace_root: &Path, mission_id: Uuid) -> Result<Self, LifecycleError> {
        match fs::read_to_string(Self::path(workspace_root, mission_id)) {
            Ok(text) => Self::from_json(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(LifecycleError::JournalIo(error.to_string())),
        }
    }

    /// Replays and validates the full history.
    pub fn projection(&self) -> Result<ControlProjection, LifecycleError> {
        let mut projection = ControlProjection::default();
        for event in &self.events {
            projection.apply(event)?;
        }
        Ok(projection)
    }

    /// Rejects waypoint targets that are not members of the Mission.
    pub fn validate_waypoint_targets(
        &self,
        known: &BTreeSet<String>,
    ) -> Result<(), LifecycleError> {
        for event in &self.events {
            if let ControlTarget::Waypoint { waypoint_id } = &event.target
                && !known.contains(waypoint_id)
            {
                return Err(LifecycleError::InvalidControlHistory {
                    seq: event.seq,
                    reason: format!(
                        "target waypoint '{waypoint_id}' is not a control target of this Mission"
                    ),
                });
            }
        }
        Ok(())
    }

    /// Builds the next legal event for `target` without persisting it. Fails
    /// with `illegal_transition` for edges the control contract forbids.
    pub fn next_event(
        &self,
        target: ControlTarget,
        kind: ControlKind,
        current_state: LifecycleState,
        op_id: Uuid,
        evidence: Option<String>,
    ) -> Result<ControlEvent, LifecycleError> {
        let mut projection = self.projection()?;
        let control = projection.target(&target);
        let prior_state = match kind {
            ControlKind::Resume => control.paused_from.unwrap_or(current_state),
            _ if current_state == LifecycleState::Paused => {
                control.paused_from.unwrap_or(current_state)
            }
            _ => current_state,
        };
        let event = ControlEvent {
            seq: projection.last_seq + 1,
            op_id,
            target,
            prior_seq: projection.last_seq,
            event: kind,
            prior_state,
            evidence,
        };
        projection.apply(&event).map_err(|error| match error {
            LifecycleError::InvalidControlHistory { reason, .. } => {
                LifecycleError::IllegalTransition(reason)
            }
            other => other,
        })?;
        Ok(event)
    }
}

// ---------------------------------------------------------------------------
// Projections
// ---------------------------------------------------------------------------

/// Projects a single-session waypoint from its native state and controls.
/// Precedence: terminal cancel/complete, pause, then the native mapping.
pub fn project_waypoint(
    native: &WaypointState,
    control: &TargetControl,
) -> Result<LifecycleState, LifecycleError> {
    let base = LifecycleState::from_waypoint_state(native);
    let conflict = |reason: &str| LifecycleError::InvalidControlHistory {
        seq: control.last_seq,
        reason: reason.to_string(),
    };
    if let Some(terminal) = control.terminal {
        if base == LifecycleState::Completed && terminal == LifecycleState::Cancelled {
            return Err(conflict(
                "cancelled control conflicts with completed native state",
            ));
        }
        return Ok(terminal);
    }
    if let Some(paused_from) = control.paused_from {
        if base == LifecycleState::Completed {
            return Err(conflict(
                "paused control conflicts with completed native state",
            ));
        }
        if paused_from != base {
            return Err(conflict("native state changed while paused"));
        }
        return Ok(LifecycleState::Paused);
    }
    Ok(base)
}

/// Facts a Mission projection is computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionFacts {
    /// Shared state of each ordered waypoint (Ticket-backed ones as read from
    /// the Ticket store).
    pub waypoints: Vec<LifecycleState>,
    /// A required entry cannot progress.
    pub required_entry_unavailable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MissionProjection {
    pub state: LifecycleState,
    pub result_review_needed: bool,
}

/// Mission projection. Precedence: terminal cancel/complete, pause, blocked
/// entry, then planned/active. Completion only comes from an explicit event
/// and is rejected unless every required waypoint outcome is completed; a
/// cancelled or blocked first unsatisfied required entry blocks the Mission.
pub fn project_mission(
    facts: &MissionFacts,
    control: &TargetControl,
) -> Result<MissionProjection, LifecycleError> {
    let done = |state| {
        Ok(MissionProjection {
            state,
            result_review_needed: false,
        })
    };
    let all_completed = !facts.waypoints.is_empty()
        && facts
            .waypoints
            .iter()
            .all(|s| *s == LifecycleState::Completed);
    if let Some(terminal) = control.terminal {
        if terminal == LifecycleState::Completed && !all_completed {
            return Err(LifecycleError::InvalidControlHistory {
                seq: control.last_seq,
                reason: "completion event while required waypoint outcomes are not all completed"
                    .into(),
            });
        }
        return done(terminal);
    }
    if control.paused_from.is_some() {
        return done(LifecycleState::Paused);
    }
    let entry_unsatisfied = facts
        .waypoints
        .iter()
        .find(|s| **s != LifecycleState::Completed)
        .is_some_and(|s| matches!(s, LifecycleState::Blocked | LifecycleState::Cancelled));
    if facts.required_entry_unavailable || entry_unsatisfied {
        return done(LifecycleState::Blocked);
    }
    if all_completed {
        return Ok(MissionProjection {
            state: LifecycleState::Active,
            result_review_needed: true,
        });
    }
    if facts
        .waypoints
        .iter()
        .any(|s| *s != LifecycleState::Planned)
    {
        return done(LifecycleState::Active);
    }
    done(LifecycleState::Planned)
}

// ---------------------------------------------------------------------------
// Capability contracts
// ---------------------------------------------------------------------------

/// Native transition request (`to_native_state` is domain-owned vocabulary).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionRequest {
    pub op_id: Uuid,
    pub to_native_state: String,
    pub author: Option<String>,
}

/// Request for a capability operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlRequest {
    pub op_id: Uuid,
    pub expected_control_sequence: u64,
    pub evidence: Option<String>,
}

pub trait LifecycleRead {
    fn read_lifecycle(
        &self,
        identity: &WaypointIdentity,
    ) -> Result<LifecycleSnapshot, LifecycleError>;
}

pub trait LifecycleTransition {
    fn transition(
        &self,
        identity: &WaypointIdentity,
        request: &TransitionRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError>;
}

macro_rules! capability_trait {
    ($(#[$doc:meta])* $name:ident { $($method:ident => $cap:ident),+ $(,)? }) => {
        $(#[$doc])*
        pub trait $name {
            /// Name used in unsupported-capability diagnostics.
            fn provider_name(&self) -> &str;
            $(
                fn $method(
                    &self,
                    _identity: &WaypointIdentity,
                    _request: &ControlRequest,
                ) -> Result<LifecycleSnapshot, LifecycleError> {
                    Err(LifecycleError::unsupported(Capability::$cap, self.provider_name()))
                }
            )+
        }
    };
}

capability_trait!(
    /// Plan revision; semantic edits require reapproval.
    PlanningCapability { revise_plan => Planning });
capability_trait!(ApprovalCapability { approve => Approval });
capability_trait!(PauseResumeCapability { pause => PauseResume, resume => PauseResume });
capability_trait!(CancellationCapability { cancel => Cancellation });
capability_trait!(CompletionCapability { complete => Completion, review_result => ResultReview });
capability_trait!(HandoffCapability { hand_off => Handoff });
capability_trait!(ReopenCapability { reopen => Reopen });
