//! Public Mission facade and shared transport operation semantics.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

pub use mission_api::*;
use serde::Serialize;
use ticket_api::storage::store::TicketStore;
use uuid::Uuid;

pub mod approval;
mod direct;
pub mod intake;
pub mod lifecycle;
#[cfg(feature = "mcp")]
pub mod named;
mod migration;

pub use direct::DirectWaypoint;
pub use lifecycle::WorkspaceLifecycle;
pub use migration::{DossierMigrationRecord, DossierMigrationReport};

pub const MISSION_WORKSPACE_ENV: &str = "MISSION_WORKSPACE";

/// Resolves a transport's launch root. Domain APIs receive only the returned
/// explicit root and never consult process environment.
pub fn resolve_workspace(
    explicit_workspace: Option<&Path>,
    configured_workspace: Option<&OsStr>,
    cwd: &Path,
) -> Result<(PathBuf, &'static str), String> {
    if !cwd.is_absolute() {
        return Err(format!("process cwd must be absolute: {}", cwd.display()));
    }
    let (selected, source) = if let Some(explicit) = explicit_workspace {
        if explicit.as_os_str().is_empty() {
            return Err("--workspace must not be empty".into());
        }
        (explicit, "explicit")
    } else if let Some(configured) = configured_workspace {
        if configured.is_empty() {
            return Err(format!("{MISSION_WORKSPACE_ENV} must not be empty"));
        }
        (Path::new(configured), "configured")
    } else {
        (cwd, "cwd")
    };
    let path = if selected.is_absolute() {
        selected.to_path_buf()
    } else {
        cwd.join(selected)
    };
    let path = normalize_absolute(&path)?;
    validate_workspace_path(&path)?;
    Ok((path, source))
}

fn normalize_absolute(path: &Path) -> Result<PathBuf, String> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !normalized.pop() {
                    return Err(format!(
                        "workspace path escapes its filesystem root: {}",
                        path.display()
                    ));
                }
            }
            std::path::Component::Normal(part) => normalized.push(part),
        }
    }
    if !normalized.is_absolute() {
        return Err(format!(
            "workspace path must resolve to an absolute path: {}",
            path.display()
        ));
    }
    Ok(normalized)
}

fn validate_workspace_path(path: &Path) -> Result<(), String> {
    let components: Vec<_> = path.components().collect();
    let mut current = PathBuf::new();
    for component in &components {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "workspace path must not contain symlinks: {}",
                    current.display()
                ));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(format!(
                    "workspace path ancestor is not a directory: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!(
                    "cannot inspect workspace path {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

/// Operations exposed identically by the CLI and MCP adapters.
#[derive(Clone, Debug)]
pub enum Operation {
    CreateMission {
        title: String,
        objective: String,
        waypoints: Vec<DirectWaypoint>,
    },
    CreateDraft {
        title: String,
        objective: String,
    },
    SaveDraft {
        mission_id: Uuid,
        draft: TypedDraft,
        expected_current_revision: u64,
    },
    PublishDraft {
        mission_id: Uuid,
        draft: TypedDraft,
        expected_current_revision: u64,
    },
    GetMissionSource {
        mission_id: Uuid,
    },
    RenderMission {
        mission_id: Uuid,
    },
    EditMissionObjective {
        mission_id: Uuid,
        objective: String,
        expected_current_revision: u64,
    },
    AddWaypoint {
        mission_id: Uuid,
        waypoint: DirectWaypoint,
        expected_current_revision: u64,
    },
    EditWaypoint {
        mission_id: Uuid,
        waypoint: DirectWaypoint,
        expected_current_revision: u64,
    },
    OrderWaypoints {
        mission_id: Uuid,
        execution_order: Vec<String>,
        waypoint_dependencies: BTreeMap<String, Vec<String>>,
        expected_current_revision: u64,
    },
    RecordProgress {
        mission_id: Uuid,
        waypoint_id: String,
        state: WaypointState,
        expected_current_revision: u64,
    },
    RecordValidationResult {
        mission_id: Uuid,
        result: OperatorResult,
        expected_current_revision: u64,
    },
    ValidateMission {
        mission_id: Uuid,
    },
    PublishMission {
        mission_id: Uuid,
        expected_current_revision: u64,
    },
    Get {
        mission_id: Uuid,
    },
    ValidatePreview {
        manifest_path: PathBuf,
    },
    Import {
        mission_id: Uuid,
        manifest_path: PathBuf,
        expected_current_revision: u64,
    },
    RenderPreview {
        mission_id: Uuid,
    },
    Publish {
        mission_id: Uuid,
        manifest_path: PathBuf,
        expected_current_revision: u64,
    },
    CheckGenerated {
        mission_id: Uuid,
    },
    GetOutputContract {
        mission_id: Uuid,
    },
    GetApprovalEvent {
        mission_id: Uuid,
        revision: u64,
    },
    RecordApprovalEvent {
        mission_id: Uuid,
        json: String,
    },
    PublishWithInputs {
        mission_id: Uuid,
        manifest_path: PathBuf,
        expected_current_revision: u64,
        inputs: MissionOutputInputs,
    },
    MigrateDossiers {
        dossier_path: Option<PathBuf>,
        dry_run: bool,
    },
    MigrateTypedStores {
        roots: Vec<PathBuf>,
    },
    RepairPreview {
        request: RepairRequest,
    },
    RepairApply {
        request: RepairRequest,
        proposal_id: String,
    },
    RepairJournal {
        journal_id: Uuid,
    },
    RepairResume {
        journal_id: Uuid,
    },
    RepairRollback {
        journal_id: Uuid,
    },
}

/// A stable operation result suitable for either transport.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StatusSnapshot {
    pub status: String,
    pub mission_id: Option<Uuid>,
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_state: Option<WaypointState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_state: Option<PublicationState>,
    pub diagnostics: Vec<StatusDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered: Option<RenderedPreview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accepted: Option<AcceptedMission>,
    /// API-owned typed source returned by the draft lifecycle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<TypedDraft>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_report: Option<DossierMigrationReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typed_migration: Option<MigrationJournal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair: Option<RepairResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_record: Option<OutputRecord>,
    #[serde(default)]
    pub external_status_snapshot: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "record", rename_all = "kebab-case")]
pub enum OutputRecord {
    Contract {
        contract: MissionOutputContract,
        binding: String,
    },
    ApprovalEvent(MissionApprovalEvent),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublicationState {
    NotPublished,
    Current,
    Drift,
}

/// Deterministic document bytes returned by render-preview.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RenderedPreview {
    pub roadmap: String,
    pub parts: std::collections::BTreeMap<String, String>,
}

/// A transport-neutral, structured diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StatusDiagnostic {
    pub code: String,
    pub source: String,
    pub pointer: String,
    pub waypoint_id: Option<String>,
    pub message: String,
}

/// Runs an operation. CLI and MCP adapters intentionally delegate here.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_state_without_a_mapping_returns_an_explicit_error() {
        let snapshot = BTreeMap::from([(
            "ce://default/ticket/00000000-0000-0000-0000-000000000001".into(),
            "paused".into(),
        )]);

        let error = snapshot_states(&snapshot).unwrap_err();

        assert_eq!(
            error.diagnostics().unwrap()[0].code,
            "external-status-unresolved"
        );
    }
}
mod operations;

pub use operations::{execute, execute_cli, execute_mcp};
pub(crate) use operations::{
    finish, parse_reference, snapshot_id, snapshot_states, ticket_snapshot,
};
