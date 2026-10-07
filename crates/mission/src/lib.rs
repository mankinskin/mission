//! Public Mission facade and shared transport operation semantics.

use std::{
    fs,
    path::{Path, PathBuf},
};

pub use mission_api::*;
use serde::Serialize;
use uuid::Uuid;

/// Operations exposed identically by the CLI and MCP adapters.
#[derive(Clone, Debug)]
pub enum Operation {
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
}

/// A stable operation result suitable for either transport.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StatusSnapshot {
    pub status: String,
    pub mission_id: Option<Uuid>,
    pub revision: Option<u64>,
    pub diagnostics: Vec<StatusDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered: Option<RenderedPreview>,
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
        Operation::ValidatePreview { manifest_path } => match collect_bundle(&manifest_path) {
            Ok(bundle) => success(None, Some(bundle.manifest.revision)),
            Err(error) => failure(None, error),
        },
        Operation::Import {
            mission_id,
            manifest_path,
            expected_current_revision,
        } => with_bundle(&manifest_path, mission_id, |bundle| {
            MissionStore::init(workspace_root)?.import(
                mission_id,
                bundle,
                expected_current_revision,
            )
        }),
        Operation::RenderPreview { mission_id } => match MissionStore::open(workspace_root)
            .get(mission_id)
            .and_then(|accepted| {
                let rendered = render(&accepted.bundle, mission_id).map_err(|diagnostics| {
                    MissionError::Validation(diagnostics.len(), diagnostics)
                })?;
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
                Ok((accepted, RenderedPreview { roadmap, parts }))
            }) {
            Ok((accepted, rendered)) => StatusSnapshot {
                status: "ok".into(),
                mission_id: Some(mission_id),
                revision: Some(accepted.bundle.manifest.revision),
                diagnostics: vec![],
                rendered: Some(rendered),
            },
            Err(error) => failure(Some(mission_id), error),
        },
        Operation::Publish {
            mission_id,
            manifest_path,
            expected_current_revision,
        } => with_bundle(&manifest_path, mission_id, |bundle| {
            MissionStore::init(workspace_root)?.publish(
                mission_id,
                bundle,
                expected_current_revision,
            )
        }),
        Operation::CheckGenerated { mission_id } => check_generated(workspace_root, mission_id),
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

fn with_bundle(
    manifest_path: &Path,
    mission_id: Uuid,
    operation: impl FnOnce(MissionBundle) -> Result<AcceptedMission, MissionError>,
) -> StatusSnapshot {
    match collect_bundle(manifest_path).and_then(operation) {
        Ok(accepted) => success(Some(mission_id), Some(accepted.bundle.manifest.revision)),
        Err(error) => failure(Some(mission_id), error),
    }
}

fn check_generated(workspace_root: &Path, mission_id: Uuid) -> StatusSnapshot {
    let result = MissionStore::open(workspace_root)
        .get(mission_id)
        .and_then(|accepted| {
            let rendered = render(&accepted.bundle, mission_id)
                .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
            let generated = generated_directory(workspace_root, mission_id);
            let mut expected = rendered.parts;
            expected.insert("ROADMAP.md".into(), rendered.roadmap);
            let mut diagnostics = Vec::new();
            for (name, bytes) in &expected {
                let path = generated.join(name);
                match fs::read(&path) {
                    Ok(actual) if actual == *bytes => {}
                    Ok(_) => diagnostics.push(drift(&path)),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        diagnostics.push(drift(&path));
                    }
                    Err(error) => return Err(MissionError::Io(error)),
                }
            }
            if generated.is_dir() {
                for entry in fs::read_dir(&generated)? {
                    let entry = entry?;
                    if !expected.contains_key(&entry.file_name().to_string_lossy().to_string()) {
                        diagnostics.push(drift(&entry.path()));
                    }
                }
            }
            Ok((accepted, diagnostics))
        });
    match result {
        Ok((accepted, diagnostics)) if diagnostics.is_empty() => {
            success(Some(mission_id), Some(accepted.bundle.manifest.revision))
        }
        Ok((accepted, diagnostics)) => StatusSnapshot {
            status: "drift".into(),
            mission_id: Some(mission_id),
            revision: Some(accepted.bundle.manifest.revision),
            diagnostics,
            rendered: None,
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

fn drift(path: &Path) -> StatusDiagnostic {
    StatusDiagnostic {
        code: "generated-drift".into(),
        source: path.display().to_string(),
        pointer: String::new(),
        waypoint_id: None,
        message: "Generated document differs from the accepted mission projection.".into(),
    }
}

fn success(mission_id: Option<Uuid>, revision: Option<u64>) -> StatusSnapshot {
    StatusSnapshot {
        status: "ok".into(),
        mission_id,
        revision,
        diagnostics: vec![],
        rendered: None,
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
        diagnostics,
        rendered: None,
    }
}
