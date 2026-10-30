use super::*;

pub(crate) fn import_bundle(
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
            output_record: None,
            external_status_snapshot,
        },
        Err(error) => failure(Some(mission_id), error),
    }
}

pub(crate) fn publish_bundle(
    workspace_root: &Path,
    mission_id: Uuid,
    manifest_path: &Path,
    expected_current_revision: u64,
) -> StatusSnapshot {
    publish_bundle_with_inputs(
        workspace_root,
        mission_id,
        manifest_path,
        expected_current_revision,
        None,
    )
}

pub(crate) fn publish_bundle_with_inputs(
    workspace_root: &Path,
    mission_id: Uuid,
    manifest_path: &Path,
    expected_current_revision: u64,
    inputs: Option<&MissionOutputInputs>,
) -> StatusSnapshot {
    let store = MissionStore::open(workspace_root);
    let result = collect_bundle(manifest_path).and_then(|bundle| {
        let snapshot = ticket_snapshot(workspace_root, &store, mission_id, &bundle)?;
        let states = snapshot_states(&snapshot)?;
        let snapshot_id = snapshot_id(&snapshot)?;
        let accepted = store.publish_with_inputs(
            mission_id,
            bundle,
            expected_current_revision,
            inputs.unwrap_or(&MissionOutputInputs::default()),
            Some((&states, &snapshot_id)),
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
            output_record: None,
            external_status_snapshot,
        },
        Err(error) => failure(Some(mission_id), error),
    }
}
