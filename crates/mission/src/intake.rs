//! Immutable Session input and recoverable managed planning-Mission creation.
//! Stops between steps expose persisted subsets, never an executable Mission.

use mission_api::{MissionError, MissionLifecycle};
use serde::{Deserialize, Serialize};
use session_api::{SessionError, SessionIntake, SessionIntakeLinkState, SessionStoreConfig};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const PROVENANCE_SCHEMA_VERSION: u32 = 1;
const INPUT_DIR: &str = "input";
const COPY_FILE: &str = "submitted-input.bin";
const PROVENANCE_FILE: &str = "intake.json";
const STORE_SEGMENTS: [&str; 3] = [".workflow-tools", "mission", "missions"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntakeStep {
    InputSubmitted,
    MissionReserved,
    MissionCreated,
    ProvenanceCopied,
    SessionLinked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeneratedPlanState {
    NotYetGenerated,
    Generated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedPlanEntry {
    pub path: PathBuf,
    pub state: GeneratedPlanState,
}

pub struct ManagedIntakeRequest<'a> {
    pub session_store: &'a SessionStoreConfig,
    pub workspace_root: &'a Path,
    pub session_id: &'a str,
    pub request_id: &'a str,
    pub input: &'a [u8],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagedIntake {
    pub session_id: String,
    pub request_id: String,
    pub mission_id: Uuid,
    pub revision: u64,
    pub workspace_root: PathBuf,
    pub mission_dir: PathBuf,
    pub input_path: PathBuf,
    pub session_input_path: PathBuf,
    pub input_digest: String,
    pub source_path: PathBuf,
    pub generated_plan_entry: GeneratedPlanEntry,
    pub executable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReceipt {
    pub session_id: String,
    pub request_id: String,
    pub mission_id: Option<Uuid>,
    pub persisted: Vec<IntakeStep>,
    pub cause: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntakeError {
    SessionNotFound {
        session_id: String,
    },
    IntakeMissing {
        session_id: String,
        request_id: String,
    },
    SessionFinished {
        session_id: String,
    },
    WorkspaceUnavailable {
        reason: String,
    },
    IncompatibleStore {
        reason: String,
    },
    IdempotencyConflict {
        request_id: String,
        existing_digest: String,
        submitted_digest: String,
    },
    InvalidRequest {
        reason: String,
    },
    LinkageConflict {
        reason: String,
    },
    Busy {
        reason: String,
    },
    Pending(PendingReceipt),
    Storage {
        reason: String,
    },
}

impl IntakeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::SessionNotFound { .. } => "session_not_found",
            Self::IntakeMissing { .. } => "intake_missing",
            Self::SessionFinished { .. } => "session_finished",
            Self::WorkspaceUnavailable { .. } => "workspace_unavailable",
            Self::IncompatibleStore { .. } => "incompatible_store",
            Self::IdempotencyConflict { .. } => "idempotency_conflict",
            Self::InvalidRequest { .. } => "invalid_request",
            Self::LinkageConflict { .. } => "linkage_conflict",
            Self::Busy { .. } => "busy",
            Self::Pending(_) => "pending",
            Self::Storage { .. } => "storage_error",
        }
    }
}

impl std::fmt::Display for IntakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = self.code();
        match self {
            Self::SessionNotFound { session_id } => write!(f, "{code}: session {session_id}"),
            Self::IntakeMissing {
                session_id,
                request_id,
            } => write!(f, "{code}: session {session_id} request {request_id}"),
            Self::SessionFinished { session_id } => write!(f, "{code}: session {session_id}"),
            Self::IdempotencyConflict {
                request_id,
                existing_digest,
                submitted_digest,
            } => write!(
                f,
                "{code}: request {request_id} has sha256 {existing_digest}, got {submitted_digest}"
            ),
            Self::Pending(receipt) => write!(
                f,
                "{code}: persisted {:?}: {}",
                receipt.persisted, receipt.cause
            ),
            Self::WorkspaceUnavailable { reason }
            | Self::IncompatibleStore { reason }
            | Self::InvalidRequest { reason }
            | Self::LinkageConflict { reason }
            | Self::Busy { reason }
            | Self::Storage { reason } => write!(f, "{code}: {reason}"),
        }
    }
}

impl std::error::Error for IntakeError {}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ProvenanceRecord {
    schema_version: u32,
    session_id: String,
    request_id: String,
    input_digest: String,
    byte_len: u64,
    session_store_root: PathBuf,
    session_input_path: PathBuf,
    input_file: String,
}

enum Halt {
    Injected(IntakeStep),
    Failed(String),
    Final(IntakeError),
}

impl From<IntakeError> for Halt {
    fn from(error: IntakeError) -> Self {
        Self::Final(error)
    }
}

/// Submit or resume a single planning Mission from immutable Session input.
mod errors;
mod mission;
mod paths;
mod provenance;
mod receipt;
mod workflow;

use errors::*;
use mission::*;
use paths::*;
use provenance::*;
use receipt::*;
pub use workflow::{find, submit_and_create, submit_and_create_with_fault};
