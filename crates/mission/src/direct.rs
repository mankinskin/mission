use super::*;
use serde::Deserialize;

/// Complete local waypoint content, supplied inline rather than in a bundle file.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectWaypoint {
    pub waypoint_id: String,
    pub title: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub prompt: String,
    pub validation: Vec<ValidationGate>,
    pub commit_checkpoint: CommitCheckpoint,
    #[serde(default)]
    pub non_goals: Vec<String>,
}

pub(super) fn create(
    workspace: &Path,
    title: String,
    objective: String,
    waypoints: Vec<DirectWaypoint>,
) -> StatusSnapshot {
    let mission_id = Uuid::new_v4();
    let bundle = MissionBundle {
        manifest: BundleManifest {
            schema_version: SCHEMA_VERSION,
            revision: 1,
            title,
            objective,
            waypoint_files: vec![],
            execution_order: waypoints
                .iter()
                .map(|item| item.waypoint_id.clone())
                .collect(),
            artifacts: vec![],
            requirements: vec![],
            validation_gates: vec![],
            notes: vec![],
            operator_results: vec![],
        },
        waypoints: waypoints.into_iter().map(fragment).collect(),
    };
    finish(
        mission_id,
        MissionStore::open(workspace).import(mission_id, bundle, 0),
    )
}

fn fragment(input: DirectWaypoint) -> WaypointFragment {
    WaypointFragment {
        session_package: input.waypoint_id.clone(),
        waypoint_id: input.waypoint_id,
        title: input.title,
        depends_on: input.depends_on,
        mode: WaypointMode::ExecutionSideEffect,
        scope: Scope::SingleSession,
        state: Some(WaypointState::Pending),
        ticket_ref: None,
        child_mission_ref: None,
        part_markdown: input.prompt.clone(),
        prompt: input.prompt,
        artifacts: vec![],
        requirement_ids: vec![],
        non_goals: input.non_goals,
        validation: input.validation,
        commit_checkpoint: input.commit_checkpoint,
    }
}

pub(super) fn add_waypoint(
    workspace: &Path,
    id: Uuid,
    waypoint: DirectWaypoint,
    expected: u64,
) -> StatusSnapshot {
    update(workspace, id, expected, |bundle| {
        bundle
            .manifest
            .execution_order
            .push(waypoint.waypoint_id.clone());
        bundle.waypoints.push(fragment(waypoint));
        Ok(())
    })
}

pub(super) fn edit_waypoint(
    workspace: &Path,
    id: Uuid,
    waypoint: DirectWaypoint,
    expected: u64,
) -> StatusSnapshot {
    update(workspace, id, expected, |bundle| {
        let item = local_waypoint(bundle, &waypoint.waypoint_id)?;
        item.title = waypoint.title;
        item.depends_on = waypoint.depends_on;
        item.part_markdown = waypoint.prompt.clone();
        item.prompt = waypoint.prompt;
        item.validation = waypoint.validation;
        item.commit_checkpoint = waypoint.commit_checkpoint;
        item.non_goals = waypoint.non_goals;
        Ok(())
    })
}

pub(super) fn record_progress(
    workspace: &Path,
    id: Uuid,
    waypoint_id: String,
    state: WaypointState,
    expected: u64,
) -> StatusSnapshot {
    update(workspace, id, expected, |bundle| {
        local_waypoint(bundle, &waypoint_id)?.state = Some(state);
        Ok(())
    })
}

pub(super) fn set_dependencies(
    bundle: &mut MissionBundle,
    dependencies: BTreeMap<String, Vec<String>>,
) -> Result<(), MissionError> {
    for (waypoint_id, depends_on) in dependencies {
        local_waypoint(bundle, &waypoint_id)?.depends_on = depends_on;
    }
    Ok(())
}

fn local_waypoint<'a>(
    bundle: &'a mut MissionBundle,
    waypoint_id: &str,
) -> Result<&'a mut WaypointFragment, MissionError> {
    let waypoint = bundle
        .waypoints
        .iter_mut()
        .find(|item| item.waypoint_id == waypoint_id)
        .ok_or_else(|| {
            invalid(
                waypoint_id,
                "unknown-waypoint",
                "Use a waypoint ID returned by get.",
            )
        })?;
    if waypoint.ticket_ref.is_some()
        || waypoint.child_mission_ref.is_some()
        || matches!(waypoint.scope, Scope::TicketBacked { .. })
    {
        return Err(invalid(
            waypoint_id,
            "externally-owned-waypoint",
            "Change the authoritative Ticket or child Mission, not local waypoint content or state.",
        ));
    }
    Ok(waypoint)
}

fn invalid(waypoint_id: &str, code: &'static str, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "accepted.json".into(),
            pointer: "/waypoints".into(),
            waypoint_id: Some(waypoint_id.into()),
            message: message.into(),
        }],
    )
}

fn next_bundle(
    store: &MissionStore,
    id: Uuid,
    expected: u64,
) -> Result<MissionBundle, MissionError> {
    let mut bundle = store.get(id)?.bundle;
    if bundle.manifest.revision != expected {
        return Err(MissionError::StaleRevision {
            expected,
            actual: bundle.manifest.revision,
        });
    }
    bundle.manifest.revision = expected.checked_add(1).ok_or_else(|| {
        MissionError::Io(std::io::Error::other(
            "Mission revision exhausted; cannot allocate another revision.",
        ))
    })?;
    Ok(bundle)
}

pub(super) fn update(
    workspace: &Path,
    id: Uuid,
    expected: u64,
    update: impl FnOnce(&mut MissionBundle) -> Result<(), MissionError>,
) -> StatusSnapshot {
    let store = MissionStore::open(workspace);
    let result = next_bundle(&store, id, expected).and_then(|mut bundle| {
        update(&mut bundle)?;
        store.import(id, bundle, expected)
    });
    finish(id, result)
}

pub(super) fn publish(workspace: &Path, id: Uuid, expected: u64) -> StatusSnapshot {
    let store = MissionStore::open(workspace);
    let result = next_bundle(&store, id, expected).and_then(|bundle| {
        let snapshot = ticket_snapshot(workspace, &store, id, &bundle)?;
        let states = snapshot_states(&snapshot)?;
        store.publish_with_status_snapshot(id, bundle, expected, &states, &snapshot_id(&snapshot)?)
    });
    finish(id, result)
}

pub(super) fn finish(id: Uuid, result: Result<AcceptedMission, MissionError>) -> StatusSnapshot {
    match result {
        Ok(accepted) => {
            let revision = accepted.bundle.manifest.revision;
            StatusSnapshot {
                accepted: Some(accepted),
                ..success(Some(id), Some(revision))
            }
        }
        Err(error) => failure(Some(id), error),
    }
}
