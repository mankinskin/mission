use super::*;

pub(crate) fn check_generated(workspace_root: &Path, mission_id: Uuid) -> StatusSnapshot {
    inspect_mission(workspace_root, mission_id, false)
}

pub(crate) fn inspect_mission(
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
        let mut diagnostics = rendered
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
        if let Err(error) = store.verify_output_history(mission_id) {
            diagnostics.push(StatusDiagnostic {
                code: "mission-output-history-invalid".into(),
                source: generated
                    .parent()
                    .unwrap_or(&generated)
                    .display()
                    .to_string(),
                pointer: String::new(),
                waypoint_id: None,
                message: error.to_string(),
            });
        }
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
                output_record: None,
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
            output_record: None,
            external_status_snapshot,
        },
        Err(error) => failure(Some(mission_id), error),
    }
}

pub(crate) fn generated_directory(workspace_root: &Path, mission_id: Uuid) -> PathBuf {
    workspace_root
        .join(".workflow-tools")
        .join("mission")
        .join("missions")
        .join(mission_id.to_string())
        .join("generated")
}

pub(crate) fn finish(
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

pub(crate) fn success(mission_id: Option<Uuid>, revision: Option<u64>) -> StatusSnapshot {
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
        output_record: None,
        external_status_snapshot: BTreeMap::new(),
    }
}

pub(crate) fn failure(mission_id: Option<Uuid>, error: MissionError) -> StatusSnapshot {
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
        output_record: None,
        external_status_snapshot: BTreeMap::new(),
    }
}
