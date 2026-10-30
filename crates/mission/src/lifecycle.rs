//! Cross-store lifecycle adapters.
//!
//! Ticket state is read from, and transitioned through, the explicit workspace
//! Ticket store; this module never persists lifecycle state of its own.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use mission_api::{AcceptedMission, MissionStore, Scope, WaypointFragment, WaypointState};
use session_api::SessionStoreConfig;
use ticket_api::{
    error::StorageError,
    storage::{store::TicketStore, ticket_fs::TicketFs},
};
use uuid::Uuid;

pub use mission_api::lifecycle::*;

const PROVIDER: &str = "workspace lifecycle adapter";

/// Lifecycle read/transition adapter over one explicit workspace root.
pub struct WorkspaceLifecycle {
    workspace_root: PathBuf,
}

struct TicketRead {
    state: LifecycleState,
    native_state: String,
    revision: u64,
}

struct MissionRead {
    state: LifecycleState,
    result_review_needed: bool,
    accepted: AcceptedMission,
    controls: ControlProjection,
}

impl WorkspaceLifecycle {
    pub fn open(workspace_root: &Path) -> Self {
        Self {
            workspace_root: workspace_root.to_path_buf(),
        }
    }

    fn ticket_store(&self) -> Result<TicketStore, LifecycleError> {
        TicketStore::open(&self.workspace_root).map_err(|error| {
            LifecycleError::ForeignAuthorityAbsent(format!(
                "Ticket store of {} is unavailable: {error}",
                self.workspace_root.display()
            ))
        })
    }

    fn read_ticket(&self, ticket_id: Uuid) -> Result<TicketRead, LifecycleError> {
        let store = self.ticket_store()?;
        let absent = |error: StorageError| {
            LifecycleError::ForeignAuthorityAbsent(format!("Ticket {ticket_id}: {error}"))
        };
        let manifest = store.get(&ticket_id).map_err(absent)?;
        let indexed = store
            .get_indexed(&ticket_id)
            .map_err(absent)?
            .ok_or_else(|| absent(StorageError::NotFound(ticket_id)))?;
        let native_state = manifest
            .extra
            .get("state")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                LifecycleError::ForeignAuthorityAbsent(format!(
                    "Ticket {ticket_id} has no string state"
                ))
            })?
            .to_string();
        let history = TicketFs::read_history(&indexed.path).map_err(absent)?;
        Ok(TicketRead {
            state: LifecycleState::from_ticket_state(&native_state)?,
            native_state,
            // An unedited Ticket has no history revision yet: revision 0.
            revision: history.iter().map(|entry| entry.rev).max().unwrap_or(0),
        })
    }

    fn load_mission(
        &self,
        mission_id: Uuid,
    ) -> Result<(AcceptedMission, ControlProjection), LifecycleError> {
        let accepted = MissionStore::open(&self.workspace_root)
            .get(mission_id)
            .map_err(|error| LifecycleError::MissionUnavailable(error.to_string()))?;
        let journal = LifecycleJournal::read(&self.workspace_root, mission_id)?;
        // Only single-session waypoints can be control targets; Ticket-backed
        // and child-Mission waypoints keep their own authority.
        let targets = accepted
            .bundle
            .waypoints
            .iter()
            .filter(|waypoint| {
                Self::ticket_of(waypoint).is_none() && waypoint.child_mission_ref.is_none()
            })
            .map(|waypoint| waypoint.waypoint_id.clone())
            .collect();
        journal.validate_waypoint_targets(&targets)?;
        let controls = journal.projection()?;
        Ok((accepted, controls))
    }

    fn ticket_of(waypoint: &WaypointFragment) -> Option<&String> {
        waypoint.ticket_ref.as_ref().or(match &waypoint.scope {
            Scope::TicketBacked { ticket_ref } => Some(ticket_ref),
            Scope::SingleSession => None,
        })
    }

    fn ticket_id(&self, reference: &str) -> Result<Uuid, LifecycleError> {
        super::parse_reference(&self.workspace_root, reference, "ticket")
            .map_err(|error| LifecycleError::ForeignAuthorityAbsent(error.to_string()))
    }

    fn find_waypoint<'a>(
        accepted: &'a AcceptedMission,
        mission_id: Uuid,
        waypoint_id: &str,
    ) -> Result<&'a WaypointFragment, LifecycleError> {
        accepted
            .bundle
            .waypoints
            .iter()
            .find(|waypoint| waypoint.waypoint_id == waypoint_id)
            .ok_or_else(|| {
                LifecycleError::MissionUnavailable(format!(
                    "mission {mission_id} has no waypoint {waypoint_id}"
                ))
            })
    }

    fn waypoint_state(
        &self,
        waypoint: &WaypointFragment,
        controls: &ControlProjection,
        visiting: &mut BTreeSet<Uuid>,
    ) -> Result<LifecycleState, LifecycleError> {
        if let Some(reference) = Self::ticket_of(waypoint) {
            return Ok(self.read_ticket(self.ticket_id(reference)?)?.state);
        }
        if let Some(reference) = &waypoint.child_mission_ref {
            let child = super::parse_reference(&self.workspace_root, reference, "mission")
                .map_err(|error| LifecycleError::MissionUnavailable(error.to_string()))?;
            return Ok(self.read_mission(child, visiting)?.state);
        }
        let native = waypoint.state.clone().unwrap_or(WaypointState::Pending);
        let control = controls.target(&ControlTarget::Waypoint {
            waypoint_id: waypoint.waypoint_id.clone(),
        });
        project_waypoint(&native, &control)
    }

    fn read_mission(
        &self,
        mission_id: Uuid,
        visiting: &mut BTreeSet<Uuid>,
    ) -> Result<MissionRead, LifecycleError> {
        if !visiting.insert(mission_id) {
            return Err(LifecycleError::MissionUnavailable(format!(
                "child mission cycle at {mission_id}"
            )));
        }
        let (accepted, controls) = self.load_mission(mission_id)?;
        let mut states = Vec::new();
        for waypoint_id in &accepted.bundle.manifest.execution_order {
            let waypoint = Self::find_waypoint(&accepted, mission_id, waypoint_id)?;
            states.push(self.waypoint_state(waypoint, &controls, visiting)?);
        }
        let projection = project_mission(
            &MissionFacts {
                waypoints: states,
                required_entry_unavailable: false,
            },
            &controls.target(&ControlTarget::Mission),
        )?;
        visiting.remove(&mission_id);
        Ok(MissionRead {
            state: projection.state,
            result_review_needed: projection.result_review_needed,
            accepted,
            controls,
        })
    }

    fn label(state: &WaypointState) -> &'static str {
        match state {
            WaypointState::Pending => "pending",
            WaypointState::InProgress => "in-progress",
            WaypointState::Completed => "completed",
            WaypointState::Blocked => "blocked",
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn snapshot(
        &self,
        identity: &WaypointIdentity,
        state: LifecycleState,
        native_domain: NativeDomain,
        native_state: String,
        native_revision: u64,
        controls: &ControlProjection,
        result_review_needed: bool,
    ) -> LifecycleSnapshot {
        LifecycleSnapshot {
            identity: identity.clone(),
            state,
            native_domain,
            native_state,
            native_revision,
            owning_root: self.workspace_root.clone(),
            control_sequence: controls.last_seq,
            eligibility: ExecutionEligibility::effective(
                state,
                &controls.target(&ControlTarget::Mission),
            ),
            result_review_needed,
        }
    }

    fn ticket_snapshot(
        &self,
        identity: &WaypointIdentity,
        ticket_id: Uuid,
        controls: &ControlProjection,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let ticket = self.read_ticket(ticket_id)?;
        Ok(self.snapshot(
            identity,
            ticket.state,
            NativeDomain::Ticket,
            ticket.native_state,
            ticket.revision,
            controls,
            false,
        ))
    }

    fn resolve_ticket(&self, identity: &WaypointIdentity) -> Result<Option<Uuid>, LifecycleError> {
        match identity {
            WaypointIdentity::Ticket { ticket_id } => Ok(Some(*ticket_id)),
            WaypointIdentity::Mission { .. } => Ok(None),
            WaypointIdentity::Waypoint {
                mission_id,
                waypoint_id,
            } => {
                let (accepted, _) = self.load_mission(*mission_id)?;
                let waypoint = Self::find_waypoint(&accepted, *mission_id, waypoint_id)?;
                Self::ticket_of(waypoint)
                    .map(|reference| self.ticket_id(reference))
                    .transpose()
            }
        }
    }

    fn gate_error(error: crate::approval::ApprovalError) -> LifecycleError {
        LifecycleError::ApprovalGate {
            code: error.code(),
            message: error.message().to_string(),
        }
    }

    fn missions_linking(&self, ticket_id: Uuid) -> Result<BTreeSet<Uuid>, LifecycleError> {
        let unavailable = |message: String| LifecycleError::ApprovalGate {
            code: "approval_authority_unavailable",
            message,
        };
        let root = self.workspace_root.join(".workflow-tools/mission/missions");
        MissionStore::open(&self.workspace_root)
            .verify_store_chain()
            .map_err(|error| unavailable(format!("Mission store path unsafe: {error}")))?;
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(BTreeSet::new());
            }
            Err(error) => return Err(unavailable(format!("Mission store unreadable: {error}"))),
        };
        let store = MissionStore::open(&self.workspace_root);
        let mut linked = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|error| unavailable(error.to_string()))?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| unavailable("non-UTF-8 Mission entry".into()))?;
            if name.starts_with('.') {
                continue;
            }
            let Ok(mission_id) = Uuid::parse_str(name) else {
                continue;
            };
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(|error| {
                unavailable(format!("Mission entry {name} unreadable: {error}"))
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(unavailable(format!(
                    "Mission entry {name} is not a plain directory"
                )));
            }
            let accepted = store.get(mission_id).map_err(|error| {
                unavailable(format!("Mission {mission_id} cannot be inspected: {error}"))
            })?;
            for waypoint in &accepted.bundle.waypoints {
                if let Some(reference) = Self::ticket_of(waypoint) {
                    let linked_id = self.ticket_id(reference).map_err(|error| {
                        unavailable(format!(
                            "Mission {mission_id} has unresolvable Ticket link: {error}"
                        ))
                    })?;
                    if linked_id == ticket_id {
                        linked.insert(mission_id);
                    }
                }
            }
        }
        Ok(linked)
    }
}

impl LifecycleRead for WorkspaceLifecycle {
    fn read_lifecycle(
        &self,
        identity: &WaypointIdentity,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        match identity {
            WaypointIdentity::Ticket { ticket_id } => {
                self.ticket_snapshot(identity, *ticket_id, &ControlProjection::default())
            }
            WaypointIdentity::Mission { mission_id } => {
                let read = self.read_mission(*mission_id, &mut BTreeSet::new())?;
                let native = MissionStore::open(&self.workspace_root)
                    .derived_state(*mission_id)
                    .map_err(|error| LifecycleError::MissionUnavailable(error.to_string()))?;
                Ok(self.snapshot(
                    identity,
                    read.state,
                    NativeDomain::Mission,
                    Self::label(&native).to_string(),
                    read.accepted.bundle.manifest.revision,
                    &read.controls,
                    read.result_review_needed,
                ))
            }
            WaypointIdentity::Waypoint {
                mission_id,
                waypoint_id,
            } => {
                let (accepted, controls) = self.load_mission(*mission_id)?;
                let waypoint = Self::find_waypoint(&accepted, *mission_id, waypoint_id)?;
                if let Some(reference) = Self::ticket_of(waypoint) {
                    return self.ticket_snapshot(identity, self.ticket_id(reference)?, &controls);
                }
                let state =
                    self.waypoint_state(waypoint, &controls, &mut BTreeSet::from([*mission_id]))?;
                let native = waypoint.state.clone().unwrap_or(WaypointState::Pending);
                Ok(self.snapshot(
                    identity,
                    state,
                    NativeDomain::MissionWaypoint,
                    Self::label(&native).to_string(),
                    accepted.bundle.manifest.revision,
                    &controls,
                    false,
                ))
            }
        }
    }
}

impl LifecycleTransition for WorkspaceLifecycle {
    /// Transitions only Ticket-owned state, through the Ticket domain's own
    /// legal-state and history handling (single hop, no Mission-side write).
    fn transition(
        &self,
        identity: &WaypointIdentity,
        request: &TransitionRequest,
    ) -> Result<LifecycleSnapshot, LifecycleError> {
        let Some(ticket_id) = self.resolve_ticket(identity)? else {
            return Err(LifecycleError::unsupported(
                Capability::Transition,
                PROVIDER,
            ));
        };
        let session = SessionStoreConfig::new(self.workspace_root.join(".workflow-tools/session"));
        let approval = crate::approval::MissionApproval::open(&self.workspace_root, &session);
        let mut _entry_guards = Vec::new();
        let mut _link_lock = None;
        if matches!(
            LifecycleState::from_ticket_state(&request.to_native_state),
            Ok(LifecycleState::Active)
        ) {
            let lifecycle = mission_api::MissionLifecycle::open(&self.workspace_root);
            let discover = || -> Result<BTreeSet<Uuid>, LifecycleError> {
                let mut missions = self.missions_linking(ticket_id)?;
                if let WaypointIdentity::Waypoint { mission_id, .. } = identity {
                    missions.insert(*mission_id);
                }
                Ok(missions)
            };
            let mut settled = false;
            for _ in 0..3 {
                _link_lock = None;
                _entry_guards.clear();
                let missions = discover()?;
                for mission_id in &missions {
                    _entry_guards.push(
                        approval
                            .guard_entry(*mission_id, crate::approval::EntryPath::Activation)
                            .map_err(Self::gate_error)?,
                    );
                }
                _link_lock = Some(lifecycle.lock_ticket_links(ticket_id).map_err(|error| {
                    LifecycleError::ApprovalGate {
                        code: "approval_authority_unavailable",
                        message: error.to_string(),
                    }
                })?);
                if discover()? == missions {
                    settled = true;
                    break;
                }
            }
            if !settled {
                return Err(LifecycleError::ApprovalGate {
                    code: "approval_authority_unavailable",
                    message: "Mission links kept changing during entry".into(),
                });
            }
        }
        self.ticket_store()?
            .update_with_options(
                &ticket_id,
                Default::default(),
                None,
                Some(&request.to_native_state),
                None,
                None,
                request.author.as_deref(),
                true,
            )
            .map_err(|error| LifecycleError::TransitionRejected(error.to_string()))?;
        self.read_lifecycle(identity)
    }
}

macro_rules! unsupported_capabilities {
    ($($trait:ident),+) => {$(
        impl $trait for WorkspaceLifecycle {
            fn provider_name(&self) -> &str {
                PROVIDER
            }
        }
    )+};
}

// Declared but not yet implemented: every operation returns an explicit
// `unsupported_capability` error and changes neither store.
unsupported_capabilities!(
    PlanningCapability,
    ApprovalCapability,
    PauseResumeCapability,
    CancellationCapability,
    CompletionCapability,
    HandoffCapability,
    ReopenCapability
);
