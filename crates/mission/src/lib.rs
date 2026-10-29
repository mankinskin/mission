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

mod direct;
mod migration;

pub use direct::DirectWaypoint;
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
    #[serde(default)]
    pub external_status_snapshot: BTreeMap<String, String>,
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
pub fn execute(workspace_root: &Path, operation: Operation) -> StatusSnapshot {
    match operation {
        Operation::CreateMission {
            title,
            objective,
            waypoints,
        } => direct::create(workspace_root, title, objective, waypoints),
        Operation::CreateDraft { title, objective } => {
            typed_result(MissionLifecycle::open(workspace_root).create_draft(title, objective))
        }
        Operation::SaveDraft {
            mission_id,
            draft,
            expected_current_revision,
        } => typed_result(MissionLifecycle::open(workspace_root).save_draft(
            mission_id,
            draft,
            expected_current_revision,
        )),
        Operation::PublishDraft {
            mission_id,
            draft,
            expected_current_revision,
        } => typed_result(MissionLifecycle::open(workspace_root).publish_draft(
            mission_id,
            draft,
            expected_current_revision,
        )),
        Operation::GetMissionSource { mission_id } => {
            typed_result(MissionLifecycle::open(workspace_root).get_mission_source(mission_id))
        }
        Operation::RenderMission { mission_id } => {
            let lifecycle = MissionLifecycle::open(workspace_root);
            match lifecycle.get_mission_source(mission_id).and_then(|source| {
                lifecycle
                    .render_mission(mission_id)
                    .map(|rendered| (source, rendered))
            }) {
                Ok((source, roadmap)) => StatusSnapshot {
                    status: "ok".into(),
                    mission_id: Some(mission_id),
                    revision: Some(source.revision),
                    mission_state: None,
                    publication_state: None,
                    diagnostics: vec![],
                    rendered: Some(RenderedPreview {
                        roadmap,
                        parts: BTreeMap::new(),
                    }),
                    accepted: None,
                    source: Some(source),
                    migration_report: None,
                    typed_migration: None,
                    repair: None,
                    external_status_snapshot: BTreeMap::new(),
                },
                Err(error) => failure(Some(mission_id), error),
            }
        }
        Operation::EditMissionObjective {
            mission_id,
            objective,
            expected_current_revision,
        } => direct::edit_objective(
            workspace_root,
            mission_id,
            objective,
            expected_current_revision,
        ),
        Operation::AddWaypoint {
            mission_id,
            waypoint,
            expected_current_revision,
        } => direct::add_waypoint(
            workspace_root,
            mission_id,
            waypoint,
            expected_current_revision,
        ),
        Operation::EditWaypoint {
            mission_id,
            waypoint,
            expected_current_revision,
        } => direct::edit_waypoint(
            workspace_root,
            mission_id,
            waypoint,
            expected_current_revision,
        ),
        Operation::OrderWaypoints {
            mission_id,
            execution_order,
            waypoint_dependencies,
            expected_current_revision,
        } => direct::order_waypoints(
            workspace_root,
            mission_id,
            execution_order,
            waypoint_dependencies,
            expected_current_revision,
        ),
        Operation::RecordProgress {
            mission_id,
            waypoint_id,
            state,
            expected_current_revision,
        } => direct::record_progress(
            workspace_root,
            mission_id,
            waypoint_id,
            state,
            expected_current_revision,
        ),
        Operation::RecordValidationResult {
            mission_id,
            result,
            expected_current_revision,
        } => direct::record_result(
            workspace_root,
            mission_id,
            result,
            expected_current_revision,
        ),
        Operation::ValidateMission { mission_id } => direct::validate(workspace_root, mission_id),
        Operation::PublishMission {
            mission_id,
            expected_current_revision,
        } => direct::publish(workspace_root, mission_id, expected_current_revision),
        Operation::Get { mission_id } => {
            let mut snapshot = inspect_mission(workspace_root, mission_id, true);
            if snapshot.status != "error" {
                snapshot.status = "ok".into();
                if snapshot.publication_state == Some(PublicationState::NotPublished) {
                    snapshot.diagnostics.clear();
                }
            }
            snapshot
        }
        Operation::ValidatePreview { manifest_path } => match collect_bundle(&manifest_path) {
            Ok(bundle) => success(None, Some(bundle.manifest.revision)),
            Err(error) => failure(None, error),
        },
        Operation::Import {
            mission_id,
            manifest_path,
            expected_current_revision,
        } => import_bundle(
            workspace_root,
            &manifest_path,
            mission_id,
            expected_current_revision,
        ),
        Operation::RenderPreview { mission_id } => {
            let store = MissionStore::open(workspace_root);
            match store.get(mission_id).and_then(|accepted| {
                let snapshot =
                    ticket_snapshot(workspace_root, &store, mission_id, &accepted.bundle)?;
                let states = snapshot_states(&snapshot)?;
                let snapshot_id = snapshot_id(&snapshot)?;
                let rendered =
                    store.render_with_status_snapshot(mission_id, &states, &snapshot_id)?;
                let mission_state =
                    store.derived_state_with_status_snapshot(mission_id, &states)?;
                let roadmap = String::from_utf8(rendered.roadmap).map_err(|error| {
                    MissionError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
                })?;
                let parts = rendered
                    .parts
                    .into_iter()
                    .map(|(name, bytes)| {
                        String::from_utf8(bytes)
                            .map(|contents| (name, contents))
                            .map_err(|error| {
                                MissionError::Io(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    error,
                                ))
                            })
                    })
                    .collect::<Result<_, _>>()?;
                Ok((
                    accepted,
                    snapshot,
                    mission_state,
                    RenderedPreview { roadmap, parts },
                ))
            }) {
                Ok((accepted, external_status_snapshot, mission_state, rendered)) => {
                    StatusSnapshot {
                        status: "ok".into(),
                        mission_id: Some(mission_id),
                        revision: Some(accepted.bundle.manifest.revision),
                        mission_state: Some(mission_state),
                        publication_state: None,
                        diagnostics: vec![],
                        rendered: Some(rendered),
                        accepted: None,
                        source: None,
                        migration_report: None,
                        typed_migration: None,
                        repair: None,
                        external_status_snapshot,
                    }
                }
                Err(error) => failure(Some(mission_id), error),
            }
        }
        Operation::Publish {
            mission_id,
            manifest_path,
            expected_current_revision,
        } => publish_bundle(
            workspace_root,
            mission_id,
            &manifest_path,
            expected_current_revision,
        ),
        Operation::CheckGenerated { mission_id } => check_generated(workspace_root, mission_id),
        Operation::MigrateDossiers {
            dossier_path,
            dry_run,
        } => match migration::migrate_dossiers(workspace_root, dossier_path.as_deref(), dry_run) {
            Ok(migration_report) => StatusSnapshot {
                status: "ok".into(),
                mission_id: None,
                revision: None,
                mission_state: None,
                publication_state: None,
                diagnostics: vec![],
                rendered: None,
                accepted: None,
                source: None,
                migration_report: Some(migration_report),
                typed_migration: None,
                repair: None,
                external_status_snapshot: BTreeMap::new(),
            },
            Err(error) => failure(None, error),
        },
        Operation::MigrateTypedStores { roots } => {
            let roots = if roots.is_empty() {
                vec![workspace_root.to_path_buf()]
            } else {
                roots
            };
            match MissionLifecycle::plan_migration(&roots).and_then(|mut journal| {
                MissionLifecycle::apply_migration(&mut journal)?;
                Ok(journal)
            }) {
                Ok(journal) => StatusSnapshot {
                    status: if journal
                        .records
                        .iter()
                        .all(|record| matches!(record.state, MigrationState::Applied))
                    {
                        "ok"
                    } else {
                        "error"
                    }
                    .into(),
                    mission_id: None,
                    revision: None,
                    mission_state: None,
                    publication_state: None,
                    diagnostics: vec![],
                    rendered: None,
                    accepted: None,
                    source: None,
                    migration_report: None,
                    typed_migration: Some(journal),
                    repair: None,
                    external_status_snapshot: BTreeMap::new(),
                },
                Err(error) => failure(None, error),
            }
        }
        Operation::RepairPreview { request } => repair_result(
            MissionLifecycle::open(workspace_root).repair_preview(workspace_root, request),
        ),
        Operation::RepairApply {
            request,
            proposal_id,
        } => repair_result(MissionLifecycle::open(workspace_root).repair_apply(
            workspace_root,
            request,
            &proposal_id,
        )),
        Operation::RepairJournal { journal_id } => repair_result(
            MissionLifecycle::open(workspace_root)
                .repair_journal(workspace_root, journal_id)
                .map(|journal| response_from_journal(&journal)),
        ),
        Operation::RepairResume { journal_id } => repair_result(
            MissionLifecycle::open(workspace_root).repair_resume(workspace_root, journal_id),
        ),
        Operation::RepairRollback { journal_id } => repair_result(
            MissionLifecycle::open(workspace_root).repair_rollback(workspace_root, journal_id),
        ),
    }
}

fn repair_result(result: Result<RepairResponse, MissionError>) -> StatusSnapshot {
    match result {
        Ok(repair) => StatusSnapshot {
            status: if repair.status == "blocked" || repair.status == "partial" {
                "error".into()
            } else {
                "ok".into()
            },
            mission_id: None,
            revision: None,
            mission_state: None,
            publication_state: None,
            diagnostics: repair
                .diagnostics
                .iter()
                .map(|message| StatusDiagnostic {
                    code: "repair-operation".into(),
                    source: "repair".into(),
                    pointer: String::new(),
                    waypoint_id: None,
                    message: message.clone(),
                })
                .collect(),
            rendered: None,
            accepted: None,
            source: None,
            migration_report: None,
            typed_migration: None,
            repair: Some(repair),
            external_status_snapshot: BTreeMap::new(),
        },
        Err(error) => failure(None, error),
    }
}

/// CLI adapter boundary; behavior is intentionally identical to MCP.
pub fn execute_cli(workspace_root: &Path, operation: Operation) -> StatusSnapshot {
    execute(workspace_root, operation)
}

/// MCP adapter boundary; behavior is intentionally identical to CLI.
pub fn execute_mcp(workspace_root: &Path, operation: Operation) -> StatusSnapshot {
    execute(workspace_root, operation)
}

fn typed_result(result: Result<TypedDraft, MissionError>) -> StatusSnapshot {
    match result {
        Ok(source) => StatusSnapshot {
            status: "ok".into(),
            mission_id: Some(source.mission_id),
            revision: Some(source.revision),
            mission_state: None,
            publication_state: None,
            diagnostics: vec![],
            rendered: None,
            accepted: None,
            source: Some(source),
            migration_report: None,
            typed_migration: None,
            repair: None,
            external_status_snapshot: BTreeMap::new(),
        },
        Err(error) => failure(None, error),
    }
}

fn import_bundle(
    workspace_root: &Path,
    manifest_path: &Path,
    mission_id: Uuid,
    expected_current_revision: u64,
) -> StatusSnapshot {
    let store = MissionStore::open(workspace_root);
    let result = collect_bundle(manifest_path).and_then(|bundle| {
        let snapshot = ticket_snapshot(workspace_root, &store, mission_id, &bundle)?;
        let states = snapshot_states(&snapshot)?;
        let accepted = store.import(mission_id, bundle, expected_current_revision)?;
        let mission_state = store.derived_state_with_status_snapshot(mission_id, &states)?;
        Ok((accepted, snapshot, mission_state))
    });
    match result {
        Ok((accepted, external_status_snapshot, mission_state)) => StatusSnapshot {
            status: "ok".into(),
            mission_id: Some(mission_id),
            revision: Some(accepted.bundle.manifest.revision),
            mission_state: Some(mission_state),
            publication_state: None,
            diagnostics: vec![],
            rendered: None,
            accepted: None,
            source: None,
            migration_report: None,
            typed_migration: None,
            repair: None,
            external_status_snapshot,
        },
        Err(error) => failure(Some(mission_id), error),
    }
}

fn publish_bundle(
    workspace_root: &Path,
    mission_id: Uuid,
    manifest_path: &Path,
    expected_current_revision: u64,
) -> StatusSnapshot {
    let store = MissionStore::open(workspace_root);
    let result = collect_bundle(manifest_path).and_then(|bundle| {
        let snapshot = ticket_snapshot(workspace_root, &store, mission_id, &bundle)?;
        let states = snapshot_states(&snapshot)?;
        let snapshot_id = snapshot_id(&snapshot)?;
        let accepted = store.publish_with_status_snapshot(
            mission_id,
            bundle,
            expected_current_revision,
            &states,
            &snapshot_id,
        )?;
        let mission_state = store.derived_state_with_status_snapshot(mission_id, &states)?;
        Ok((accepted, snapshot, mission_state))
    });
    match result {
        Ok((accepted, external_status_snapshot, mission_state)) => StatusSnapshot {
            status: "ok".into(),
            mission_id: Some(mission_id),
            revision: Some(accepted.bundle.manifest.revision),
            mission_state: Some(mission_state),
            publication_state: Some(PublicationState::Current),
            diagnostics: vec![],
            rendered: None,
            accepted: None,
            source: None,
            migration_report: None,
            typed_migration: None,
            repair: None,
            external_status_snapshot,
        },
        Err(error) => failure(Some(mission_id), error),
    }
}

pub(crate) fn ticket_snapshot(
    workspace_root: &Path,
    mission_store: &MissionStore,
    mission_id: Uuid,
    bundle: &MissionBundle,
) -> Result<BTreeMap<String, String>, MissionError> {
    let mut references = BTreeSet::new();
    collect_ticket_references(
        workspace_root,
        mission_store,
        mission_id,
        bundle,
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut references,
    )?;
    if references.is_empty() {
        return Ok(BTreeMap::new());
    }

    let store = TicketStore::open(workspace_root).map_err(|error| {
        external_status_error(
            "ce://default/ticket",
            &format!("Could not open the selected workspace's ticket store: {error}"),
        )
    })?;
    references
        .into_iter()
        .map(|reference| {
            let id = parse_reference(workspace_root, &reference, "ticket")?;
            let ticket = store.get(&id).map_err(|error| {
                external_status_error(
                    &reference,
                    &format!("Could not resolve ticket status from the selected store: {error}"),
                )
            })?;
            let state = ticket
                .extra
                .get("state")
                .and_then(|state| state.as_str())
                .ok_or_else(|| {
                    external_status_error(
                        &reference,
                        "The resolved ticket has no string-valued state field.",
                    )
                })?;
            Ok((reference, state.to_string()))
        })
        .collect()
}

fn collect_ticket_references(
    workspace_root: &Path,
    mission_store: &MissionStore,
    mission_id: Uuid,
    bundle: &MissionBundle,
    visiting: &mut std::collections::BTreeSet<Uuid>,
    visited: &mut std::collections::BTreeSet<Uuid>,
    references: &mut std::collections::BTreeSet<String>,
) -> Result<(), MissionError> {
    if visited.contains(&mission_id) {
        return Ok(());
    }
    if !visiting.insert(mission_id) {
        return Err(external_status_error(
            &format!("ce://default/mission/{mission_id}"),
            "Child mission references must not form a cycle.",
        ));
    }

    for waypoint in &bundle.waypoints {
        if let Some(reference) = waypoint.ticket_ref.as_ref().or(match &waypoint.scope {
            Scope::TicketBacked { ticket_ref } => Some(ticket_ref),
            Scope::SingleSession => None,
        }) {
            parse_reference(workspace_root, reference, "ticket")?;
            references.insert(reference.clone());
        }
        let Some(reference) = &waypoint.child_mission_ref else {
            continue;
        };
        let child_id = parse_reference(workspace_root, reference, "mission").map_err(|error| {
            validation_error(
                "cross-workspace-child-mission",
                reference,
                Some(&waypoint.waypoint_id),
                &error.to_string(),
            )
        })?;
        let child = mission_store.get(child_id).map_err(|error| match error {
            MissionError::NotFound(_) => validation_error(
                "unaccepted-child-mission",
                reference,
                Some(&waypoint.waypoint_id),
                "Child mission must be accepted in the selected workspace.",
            ),
            other => validation_error(
                "child-mission-resolution-failed",
                reference,
                Some(&waypoint.waypoint_id),
                &other.to_string(),
            ),
        })?;
        if child.bundle.waypoints.is_empty() {
            return Err(validation_error(
                "empty-child-mission",
                reference,
                Some(&waypoint.waypoint_id),
                "Child missions must contain at least one waypoint.",
            ));
        }
        collect_ticket_references(
            workspace_root,
            mission_store,
            child_id,
            &child.bundle,
            visiting,
            visited,
            references,
        )?;
    }

    visiting.remove(&mission_id);
    visited.insert(mission_id);
    Ok(())
}

fn parse_reference(
    workspace_root: &Path,
    reference: &str,
    kind: &str,
) -> Result<Uuid, MissionError> {
    let Some(rest) = reference.strip_prefix("ce://") else {
        return Err(external_status_error(
            reference,
            "Reference is not a canonical URN.",
        ));
    };
    let mut parts = rest.split('/');
    let workspace = parts.next().unwrap_or_default();
    let reference_kind = parts.next().unwrap_or_default();
    let id = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || reference_kind != kind
        || (workspace != "default" && workspace != selected_workspace(workspace_root))
    {
        return Err(external_status_error(
            reference,
            "Reference does not resolve within the explicitly selected workspace.",
        ));
    }
    Uuid::parse_str(id)
        .map_err(|error| external_status_error(reference, &format!("Invalid entity UUID: {error}")))
}

fn selected_workspace(workspace_root: &Path) -> &str {
    workspace_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default")
}

fn external_status_error(reference: &str, message: &str) -> MissionError {
    validation_error("external-status-unresolved", reference, None, message)
}

fn validation_error(
    code: &'static str,
    reference: &str,
    waypoint_id: Option<&str>,
    message: &str,
) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: reference.to_string(),
            pointer: "/external_status_snapshot".into(),
            waypoint_id: waypoint_id.map(str::to_string),
            message: message.to_string(),
        }],
    )
}

pub(crate) fn snapshot_states(
    snapshot: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, WaypointState>, MissionError> {
    snapshot
        .iter()
        .map(|(reference, state)| {
            let waypoint_state = match state.as_str() {
                "planning" | "ready" => WaypointState::Pending,
                "in-progress" | "in-implementation" | "in-review" => WaypointState::InProgress,
                "blocked" | "on-hold" | "cancelled" => WaypointState::Blocked,
                "completed" | "done" => WaypointState::Completed,
                _ => {
                    return Err(external_status_error(
                        reference,
                        &format!("Ticket state `{state}` has no Mission state mapping."),
                    ));
                }
            };
            Ok((reference.clone(), waypoint_state))
        })
        .collect()
}

pub(crate) fn snapshot_id(snapshot: &BTreeMap<String, String>) -> Result<String, MissionError> {
    Ok(format!(
        "ticket-status-v1:{}",
        serde_json::to_string(snapshot)?
    ))
}

fn check_generated(workspace_root: &Path, mission_id: Uuid) -> StatusSnapshot {
    inspect_mission(workspace_root, mission_id, false)
}

fn inspect_mission(
    workspace_root: &Path,
    mission_id: Uuid,
    include_accepted: bool,
) -> StatusSnapshot {
    let store = MissionStore::open(workspace_root);
    let result = store.get(mission_id).and_then(|accepted| {
        let snapshot = ticket_snapshot(workspace_root, &store, mission_id, &accepted.bundle)?;
        let states = snapshot_states(&snapshot)?;
        let snapshot_id = snapshot_id(&snapshot)?;
        let rendered = store.render_with_status_snapshot(mission_id, &states, &snapshot_id)?;
        let mission_state = store.derived_state_with_status_snapshot(mission_id, &states)?;
        let generated = generated_directory(workspace_root, mission_id);
        let diagnostics = rendered
            .check_generated(&generated)?
            .into_iter()
            .map(|diagnostic| StatusDiagnostic {
                code: diagnostic.code.into(),
                source: diagnostic.source,
                pointer: diagnostic.pointer,
                waypoint_id: diagnostic.waypoint_id,
                message: diagnostic.message,
            })
            .collect::<Vec<_>>();
        Ok((accepted, diagnostics, snapshot, mission_state))
    });
    match result {
        Ok((accepted, diagnostics, external_status_snapshot, mission_state))
            if diagnostics.is_empty() =>
        {
            StatusSnapshot {
                status: "ok".into(),
                mission_id: Some(mission_id),
                revision: Some(accepted.bundle.manifest.revision),
                mission_state: Some(mission_state),
                publication_state: Some(PublicationState::Current),
                diagnostics,
                rendered: None,
                accepted: include_accepted.then_some(accepted),
                source: None,
                migration_report: None,
                typed_migration: None,
                repair: None,
                external_status_snapshot,
            }
        }
        Ok((accepted, diagnostics, external_status_snapshot, mission_state)) => StatusSnapshot {
            status: "drift".into(),
            mission_id: Some(mission_id),
            revision: Some(accepted.bundle.manifest.revision),
            mission_state: Some(mission_state),
            publication_state: Some(
                if generated_directory(workspace_root, mission_id).is_dir() {
                    PublicationState::Drift
                } else {
                    PublicationState::NotPublished
                },
            ),
            diagnostics,
            rendered: None,
            accepted: include_accepted.then_some(accepted),
            source: None,
            migration_report: None,
            typed_migration: None,
            repair: None,
            external_status_snapshot,
        },
        Err(error) => failure(Some(mission_id), error),
    }
}

fn generated_directory(workspace_root: &Path, mission_id: Uuid) -> PathBuf {
    workspace_root
        .join(".workflow-tools")
        .join("mission")
        .join("missions")
        .join(mission_id.to_string())
        .join("generated")
}

fn finish(
    requested_id: Option<Uuid>,
    result: Result<AcceptedMission, MissionError>,
) -> StatusSnapshot {
    match result {
        Ok(accepted) => {
            let mission_id = requested_id.unwrap_or(accepted.mission_id);
            let revision = accepted.bundle.manifest.revision;
            StatusSnapshot {
                accepted: Some(accepted),
                ..success(Some(mission_id), Some(revision))
            }
        }
        Err(error) => failure(requested_id, error),
    }
}

fn success(mission_id: Option<Uuid>, revision: Option<u64>) -> StatusSnapshot {
    StatusSnapshot {
        status: "ok".into(),
        mission_id,
        revision,
        mission_state: None,
        publication_state: None,
        diagnostics: vec![],
        rendered: None,
        accepted: None,
        source: None,
        migration_report: None,
        typed_migration: None,
        repair: None,
        external_status_snapshot: BTreeMap::new(),
    }
}

fn failure(mission_id: Option<Uuid>, error: MissionError) -> StatusSnapshot {
    let diagnostics = error.diagnostics().map_or_else(
        || {
            vec![StatusDiagnostic {
                code: match error {
                    MissionError::StaleRevision { .. } => "stale-revision".into(),
                    MissionError::PublicationRecovery { .. } => {
                        "publication-recovery-required".into()
                    }
                    _ => "mission-operation-failed".into(),
                },
                source: String::new(),
                pointer: String::new(),
                waypoint_id: None,
                message: error.to_string(),
            }]
        },
        |diagnostics| {
            diagnostics
                .iter()
                .map(|diagnostic| StatusDiagnostic {
                    code: diagnostic.code.into(),
                    source: diagnostic.source.clone(),
                    pointer: diagnostic.pointer.clone(),
                    waypoint_id: diagnostic.waypoint_id.clone(),
                    message: diagnostic.message.clone(),
                })
                .collect()
        },
    );
    StatusSnapshot {
        status: "error".into(),
        mission_id,
        revision: None,
        mission_state: None,
        publication_state: None,
        diagnostics,
        rendered: None,
        accepted: None,
        source: None,
        migration_report: None,
        typed_migration: None,
        repair: None,
        external_status_snapshot: BTreeMap::new(),
    }
}

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
