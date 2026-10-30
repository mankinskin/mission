use super::*;

pub(super) fn entering_in_progress(
    current: Option<&AcceptedMission>,
    bundle: &MissionBundle,
) -> Option<String> {
    bundle
        .waypoints
        .iter()
        .find(|waypoint| {
            matches!(waypoint.state, Some(WaypointState::InProgress))
                && !current.is_some_and(|existing| {
                    existing.bundle.waypoints.iter().any(|old| {
                        old.waypoint_id == waypoint.waypoint_id
                            && matches!(old.state, Some(WaypointState::InProgress))
                    })
                })
        })
        .map(|waypoint| waypoint.waypoint_id.clone())
}

pub(crate) fn ensure_store_chain(root: &Path) -> Result<(), MissionError> {
    let managed = Path::new(".workflow-tools/mission/missions");
    if !root.ends_with(managed) {
        return Ok(());
    }
    let Some(workspace) = root.ancestors().nth(3) else {
        return Ok(());
    };
    let mut current = workspace.to_path_buf();
    for component in managed.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                return Err(typed_error(
                    "unsafe-part-path",
                    "Mission store components below the workspace must be plain directories.",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(crate) fn stored_source(draft: &TypedDraft) -> StoredSource {
    StoredSource {
        mission_id: draft.mission_id,
        schema_version: draft.schema_version,
        revision: draft.revision,
        title: draft.title.clone(),
        objective: draft.objective.clone(),
        waypoints: draft
            .waypoints
            .iter()
            .map(|waypoint| StoredWaypoint {
                outcome_plan: waypoint.outcome_plan.clone(),
                waypoint_id: waypoint.waypoint_id.clone(),
                title: waypoint.title.clone(),
                prompt: waypoint.prompt.clone(),
                part_file: format!("parts/{}.json", waypoint.waypoint_id),
                depends_on: waypoint.depends_on.clone(),
                validation: waypoint.validation.clone(),
                commit_checkpoint: waypoint.commit_checkpoint.clone(),
                non_goals: waypoint.non_goals.clone(),
                ownership_refs: waypoint.ownership_refs.clone(),
                relationships: waypoint.relationships.clone(),
            })
            .collect(),
    }
}
