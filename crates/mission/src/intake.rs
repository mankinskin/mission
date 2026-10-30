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
pub fn submit_and_create(request: &ManagedIntakeRequest<'_>) -> Result<ManagedIntake, IntakeError> {
    submit_and_create_with_fault(request, None)
}

/// Verification hook simulating interruption between durable steps.
#[doc(hidden)]
pub fn submit_and_create_with_fault(
    request: &ManagedIntakeRequest<'_>,
    stop_after: Option<IntakeStep>,
) -> Result<ManagedIntake, IntakeError> {
    match run(request, stop_after) {
        Ok(receipt) => Ok(receipt),
        Err(Halt::Final(error)) => Err(error),
        Err(Halt::Injected(step)) => Err(pending(request, format!("stopped after {step:?}"))),
        Err(Halt::Failed(cause)) => Err(pending(request, cause)),
    }
}

/// Resolve complete linkage; incomplete operations return their persisted subset.
pub fn find(
    store: &SessionStoreConfig,
    workspace_root: &Path,
    session_id: &str,
    request_id: &str,
) -> Result<ManagedIntake, IntakeError> {
    preflight(workspace_root)?;
    let intake = store
        .read_intake(session_id, request_id)
        .map_err(to_final)?;
    if let Some(linkage) = &intake.linkage {
        check_binding(&linkage.mission_workspace_root, workspace_root)?;
        if linkage.state == SessionIntakeLinkState::Linked {
            return build_receipt(store, workspace_root, &intake, linkage.mission_id);
        }
    }
    let (mission_id, persisted) = persisted_state(store, workspace_root, session_id, request_id)?
        .ok_or_else(|| IntakeError::IntakeMissing {
        session_id: session_id.to_string(),
        request_id: request_id.to_string(),
    })?;
    Err(IntakeError::Pending(PendingReceipt {
        session_id: session_id.to_string(),
        request_id: request_id.to_string(),
        mission_id,
        persisted,
        cause: "Session-to-Mission linkage is incomplete; retry with the same request_id and bytes"
            .into(),
    }))
}

fn run(
    request: &ManagedIntakeRequest<'_>,
    stop_after: Option<IntakeStep>,
) -> Result<ManagedIntake, Halt> {
    let ws = request.workspace_root;
    let store = request.session_store;
    let (sid, rid) = (request.session_id, request.request_id);
    preflight(ws)?;
    let mut op = store
        .begin_intake_operation(sid, rid, request.input)
        .map_err(halt)?;
    let mut intake = op.intake().clone();
    if stop_after == Some(IntakeStep::InputSubmitted) {
        return Err(Halt::Injected(IntakeStep::InputSubmitted));
    }
    if let Some(existing) = &intake.linkage {
        check_binding(&existing.mission_workspace_root, ws)?;
    }
    let linkage = match intake.linkage.clone() {
        Some(linkage) => linkage,
        None => {
            op.reserve_mission(Uuid::new_v4(), ws).map_err(halt)?;
            intake = op.intake().clone();
            let linkage = intake
                .linkage
                .clone()
                .ok_or_else(|| Halt::Failed("reservation was not persisted".into()))?;
            check_binding(&linkage.mission_workspace_root, ws)?;
            linkage
        }
    };
    if stop_after == Some(IntakeStep::MissionReserved) {
        return Err(Halt::Injected(IntakeStep::MissionReserved));
    }
    let mission_id = linkage.mission_id;
    if linkage.state != SessionIntakeLinkState::Linked {
        ensure_mission(ws, mission_id, store, &intake)?;
        if stop_after == Some(IntakeStep::MissionCreated) {
            return Err(Halt::Injected(IntakeStep::MissionCreated));
        }
        write_provenance(ws, store, mission_id, &intake, request.input)?;
        if stop_after == Some(IntakeStep::ProvenanceCopied) {
            return Err(Halt::Injected(IntakeStep::ProvenanceCopied));
        }
        op.complete_link(mission_id).map_err(halt)?;
    }
    Ok(build_receipt(store, ws, &intake, mission_id)?)
}

fn classify(error: SessionError) -> Result<IntakeError, SessionError> {
    Ok(match error {
        SessionError::IntakeSessionNotFound { session_id } => {
            IntakeError::SessionNotFound { session_id }
        }
        SessionError::IntakeMissing {
            session_id,
            request_id,
        } => IntakeError::IntakeMissing {
            session_id,
            request_id,
        },
        SessionError::WorkspaceFinished { session_id } => {
            IntakeError::SessionFinished { session_id }
        }
        SessionError::IntakeIdempotencyConflict {
            request_id,
            existing_digest,
            submitted_digest,
            ..
        } => IntakeError::IdempotencyConflict {
            request_id,
            existing_digest,
            submitted_digest,
        },
        SessionError::InvalidIntakeRequestId(value) => IntakeError::InvalidRequest {
            reason: format!("request_id `{value}` is not a safe identifier"),
        },
        SessionError::InvalidSessionId(value) => IntakeError::InvalidRequest {
            reason: format!("session id `{value}` is not a Session UUID"),
        },
        SessionError::RuntimeMutationConflict { session_id } => IntakeError::Busy {
            reason: format!("another operation holds the Session lock for {session_id}"),
        },
        SessionError::IntakeLinkageConflict { reason, .. } => {
            IntakeError::LinkageConflict { reason }
        }
        SessionError::IntakeCorrupt { path, reason } => IntakeError::IncompatibleStore {
            reason: format!("{}: {reason}", path.display()),
        },
        SessionError::InvalidStorePath(path) => IntakeError::IncompatibleStore {
            reason: format!("unsafe or symlinked Session path {}", path.display()),
        },
        other => return Err(other),
    })
}

fn halt(error: SessionError) -> Halt {
    match classify(error) {
        Ok(classified) => Halt::Final(classified),
        Err(other) => Halt::Failed(other.to_string()),
    }
}

fn to_final(error: SessionError) -> IntakeError {
    classify(error).unwrap_or_else(|other| IntakeError::Storage {
        reason: other.to_string(),
    })
}

fn pending(request: &ManagedIntakeRequest<'_>, cause: String) -> IntakeError {
    match persisted_state(
        request.session_store,
        request.workspace_root,
        request.session_id,
        request.request_id,
    ) {
        Ok(Some((mission_id, persisted))) => IntakeError::Pending(PendingReceipt {
            session_id: request.session_id.to_string(),
            request_id: request.request_id.to_string(),
            mission_id,
            persisted,
            cause,
        }),
        Ok(None) => IntakeError::Storage { reason: cause },
        Err(error) => IntakeError::Storage {
            reason: format!("{cause}; persisted state could not be inspected: {error}"),
        },
    }
}

fn unavailable(reason: impl Into<String>) -> IntakeError {
    IntakeError::WorkspaceUnavailable {
        reason: reason.into(),
    }
}

fn incompatible(reason: impl Into<String>) -> IntakeError {
    IntakeError::IncompatibleStore {
        reason: reason.into(),
    }
}

fn preflight(root: &Path) -> Result<(), IntakeError> {
    if !root.is_absolute() || root.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(unavailable(
            "workspace root must be an absolute path without `..`",
        ));
    }
    super::validate_workspace_path(root).map_err(unavailable)?;
    match fs::metadata(root) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            return Err(unavailable(format!(
                "{} is not a directory",
                root.display()
            )));
        }
        Err(error) => return Err(unavailable(format!("{}: {error}", root.display()))),
    }
    check_chain(root, &[])
}

fn check_chain(root: &Path, extra: &[&str]) -> Result<(), IntakeError> {
    let mut current = root.to_path_buf();
    for segment in STORE_SEGMENTS.iter().copied().chain(extra.iter().copied()) {
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(unavailable(format!(
                    "{} is a symlink; managed Mission paths must not escape through symlinks",
                    current.display()
                )));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(incompatible(format!(
                    "{} exists but is not a directory; incompatible Mission store",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(unavailable(format!("{}: {error}", current.display()))),
        }
    }
    Ok(())
}

fn check_binding(bound: &Path, requested: &Path) -> Result<(), IntakeError> {
    if bound == requested {
        Ok(())
    } else {
        Err(unavailable(format!(
            "request is already bound to Mission workspace root {}; refusing to use {}",
            bound.display(),
            requested.display()
        )))
    }
}

fn placeholder(intake: &SessionIntake) -> (String, String) {
    (
        format!("Planning Mission for intake {}", intake.record.request_id),
        format!(
            "Intake-derived placeholder, pending later planning. Session {} request {} input sha256:{}. No planning intent has been authored yet.",
            intake.record.session_id, intake.record.request_id, intake.record.input_digest
        ),
    )
}

fn owned_draft(
    ws: &Path,
    store: &SessionStoreConfig,
    mission_id: Uuid,
    intake: &SessionIntake,
) -> Result<bool, IntakeError> {
    let id = mission_id.to_string();
    check_chain(ws, &[&id, INPUT_DIR])?;
    let lifecycle = MissionLifecycle::open(ws);
    let dir = lifecycle.mission_directory(mission_id);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => {
            return Err(incompatible(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(IntakeError::Storage {
                reason: format!("{}: {error}", dir.display()),
            });
        }
    }
    if let Some(bytes) = read_regular(&dir.join(INPUT_DIR).join(PROVENANCE_FILE))? {
        let parsed: ProvenanceRecord = serde_json::from_slice(&bytes)
            .map_err(|e| incompatible(format!("intake.json is invalid: {e}")))?;
        if parsed == expected_record(store, intake) {
            return Ok(true);
        }
        return Err(incompatible(
            "intake.json does not match this Session intake",
        ));
    }
    let (title, objective) = placeholder(intake);
    match lifecycle.get_mission_source(mission_id) {
        Ok(draft) if draft.title == title && draft.objective == objective => Ok(true),
        Ok(_) => Err(incompatible(format!(
            "Mission {mission_id} exists but is not the draft created for this intake; refusing adoption"
        ))),
        Err(MissionError::NotFound(_)) => Err(incompatible(format!(
            "Mission directory {mission_id} exists without a source; refusing adoption"
        ))),
        Err(error) => Err(IntakeError::Storage {
            reason: error.to_string(),
        }),
    }
}

fn ensure_mission(
    ws: &Path,
    mission_id: Uuid,
    store: &SessionStoreConfig,
    intake: &SessionIntake,
) -> Result<(), Halt> {
    if owned_draft(ws, store, mission_id, intake)? {
        return Ok(());
    }
    let (title, objective) = placeholder(intake);
    match MissionLifecycle::open(ws).create_draft_with_id(mission_id, title, objective) {
        Ok(_) => Ok(()),
        Err(MissionError::StaleRevision { .. }) => {
            if owned_draft(ws, store, mission_id, intake)? {
                Ok(())
            } else {
                Err(incompatible(format!(
                    "Mission {mission_id} appeared concurrently and is not this intake's"
                ))
                .into())
            }
        }
        Err(error) => Err(Halt::Failed(error.to_string())),
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<bool> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent"))?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("intake"),
        Uuid::new_v4()
    ));
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    let linked = fs::hard_link(&tmp, path);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => {
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error),
    }
}

fn put_immutable(path: &Path, bytes: &[u8]) -> Result<(), Halt> {
    for _ in 0..2 {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_file() => {
                let existing = fs::read(path).map_err(|e| Halt::Failed(e.to_string()))?;
                return if existing == bytes {
                    Ok(())
                } else {
                    Err(incompatible(format!(
                        "{} already exists with different bytes; refusing to overwrite",
                        path.display()
                    ))
                    .into())
                };
            }
            Ok(_) => {
                return Err(
                    incompatible(format!("{} is not a regular file", path.display())).into(),
                );
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if write_new(path, bytes).map_err(|e| Halt::Failed(e.to_string()))? {
                    return Ok(());
                }
            }
            Err(error) => return Err(Halt::Failed(error.to_string())),
        }
    }
    Err(Halt::Failed(format!("could not create {}", path.display())))
}

fn expected_record(store: &SessionStoreConfig, intake: &SessionIntake) -> ProvenanceRecord {
    ProvenanceRecord {
        schema_version: PROVENANCE_SCHEMA_VERSION,
        session_id: intake.record.session_id.clone(),
        request_id: intake.record.request_id.clone(),
        input_digest: intake.record.input_digest.clone(),
        byte_len: intake.record.byte_len,
        session_store_root: store.root.clone(),
        session_input_path: intake.input_path.clone(),
        input_file: COPY_FILE.into(),
    }
}

fn write_provenance(
    ws: &Path,
    store: &SessionStoreConfig,
    mission_id: Uuid,
    intake: &SessionIntake,
    input: &[u8],
) -> Result<(), Halt> {
    let id = mission_id.to_string();
    check_chain(ws, &[&id, INPUT_DIR])?;
    let dir = MissionLifecycle::open(ws)
        .mission_directory(mission_id)
        .join(INPUT_DIR);
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(Halt::Failed(error.to_string())),
    }
    check_chain(ws, &[&id, INPUT_DIR])?;
    put_immutable(&dir.join(COPY_FILE), input)?;
    let record = serde_json::to_vec_pretty(&expected_record(store, intake))
        .map_err(|e| Halt::Failed(e.to_string()))?;
    put_immutable(&dir.join(PROVENANCE_FILE), &record)
}

fn read_regular(path: &Path) -> Result<Option<Vec<u8>>, IntakeError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => {
            fs::read(path).map(Some).map_err(|e| IntakeError::Storage {
                reason: format!("{}: {e}", path.display()),
            })
        }
        Ok(_) => Err(incompatible(format!(
            "{} is not a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(IntakeError::Storage {
            reason: format!("{}: {error}", path.display()),
        }),
    }
}

fn provenance_complete(
    mission_dir: &Path,
    store: &SessionStoreConfig,
    intake: &SessionIntake,
) -> Result<bool, IntakeError> {
    let dir = mission_dir.join(INPUT_DIR);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => {
            return Err(incompatible(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(IntakeError::Storage {
                reason: format!("{}: {error}", dir.display()),
            });
        }
    }
    let Some(copy) = read_regular(&dir.join(COPY_FILE))? else {
        return Ok(false);
    };
    let session_bytes = read_regular(&intake.input_path)?
        .ok_or_else(|| incompatible("Session input bytes are missing"))?;
    if copy != session_bytes {
        return Err(incompatible(
            "Mission provenance copy differs from the Session input",
        ));
    }
    let Some(record) = read_regular(&dir.join(PROVENANCE_FILE))? else {
        return Ok(false);
    };
    let parsed: ProvenanceRecord = serde_json::from_slice(&record)
        .map_err(|e| incompatible(format!("intake.json is invalid: {e}")))?;
    if parsed != expected_record(store, intake) {
        return Err(incompatible(
            "intake.json does not match this Session intake",
        ));
    }
    Ok(true)
}

fn persisted_state(
    store: &SessionStoreConfig,
    ws: &Path,
    session_id: &str,
    request_id: &str,
) -> Result<Option<(Option<Uuid>, Vec<IntakeStep>)>, IntakeError> {
    let intake = match store.read_intake(session_id, request_id) {
        Ok(intake) => intake,
        Err(SessionError::IntakeMissing { .. }) => return Ok(None),
        Err(error) => return Err(to_final(error)),
    };
    let mut steps = vec![IntakeStep::InputSubmitted];
    let mut mission = None;
    if let Some(linkage) = &intake.linkage {
        steps.push(IntakeStep::MissionReserved);
        mission = Some(linkage.mission_id);
        if linkage.mission_workspace_root == ws {
            if owned_draft(ws, store, linkage.mission_id, &intake)? {
                steps.push(IntakeStep::MissionCreated);
                if provenance_complete(
                    &MissionLifecycle::open(ws).mission_directory(linkage.mission_id),
                    store,
                    &intake,
                )? {
                    steps.push(IntakeStep::ProvenanceCopied);
                    if linkage.state == SessionIntakeLinkState::Linked {
                        steps.push(IntakeStep::SessionLinked);
                    }
                }
            }
        }
    }
    Ok(Some((mission, steps)))
}

fn generated_state(mission_dir: &Path) -> Result<(PathBuf, GeneratedPlanState), IntakeError> {
    let dir = mission_dir.join("generated");
    let path = dir.join("README.md");
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => {
            return Err(incompatible(format!(
                "{} is not a real directory",
                dir.display()
            )));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok((path, GeneratedPlanState::NotYetGenerated));
        }
        Err(error) => {
            return Err(IntakeError::Storage {
                reason: format!("{}: {error}", dir.display()),
            });
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_file() => Ok((path, GeneratedPlanState::Generated)),
        Ok(_) => Err(incompatible(format!(
            "{} is not a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Ok((path, GeneratedPlanState::NotYetGenerated))
        }
        Err(error) => Err(IntakeError::Storage {
            reason: format!("{}: {error}", path.display()),
        }),
    }
}

fn build_receipt(
    store: &SessionStoreConfig,
    ws: &Path,
    intake: &SessionIntake,
    mission_id: Uuid,
) -> Result<ManagedIntake, IntakeError> {
    let id = mission_id.to_string();
    check_chain(ws, &[&id, INPUT_DIR])?;
    let lifecycle = MissionLifecycle::open(ws);
    let mission_dir = lifecycle.mission_directory(mission_id);
    let draft = lifecycle
        .get_mission_source(mission_id)
        .map_err(|error| match error {
            MissionError::NotFound(_) => incompatible(format!(
                "linked Mission {mission_id} is missing; not recreating it"
            )),
            other => IntakeError::Storage {
                reason: other.to_string(),
            },
        })?;
    if !provenance_complete(&mission_dir, store, intake)? {
        return Err(incompatible(format!(
            "linked Mission {mission_id} has no complete provenance copy"
        )));
    }
    let (generated, state) = generated_state(&mission_dir)?;
    Ok(ManagedIntake {
        session_id: intake.record.session_id.clone(),
        request_id: intake.record.request_id.clone(),
        mission_id,
        revision: draft.revision,
        workspace_root: ws.to_path_buf(),
        input_path: mission_dir.join(INPUT_DIR).join(COPY_FILE),
        session_input_path: intake.input_path.clone(),
        input_digest: intake.record.input_digest.clone(),
        source_path: mission_dir.join("source.json"),
        generated_plan_entry: GeneratedPlanEntry {
            path: generated,
            state,
        },
        mission_dir,
        executable: false,
    })
}
