use super::*;

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

pub(super) fn collect_ticket_references(
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

pub(crate) fn parse_reference(
    workspace_root: &Path,
    reference: &str,
    kind: &str,
) -> Result<Uuid, MissionError> {
    mission_api::local_entity_reference_id(workspace_root, reference, kind).map_err(|error| {
        let message = error
            .diagnostics()
            .and_then(|diagnostics| diagnostics.first())
            .map_or_else(
                || error.to_string(),
                |diagnostic| diagnostic.message.clone(),
            );
        external_status_error(reference, &message)
    })
}

pub(crate) fn external_status_error(reference: &str, message: &str) -> MissionError {
    validation_error("external-status-unresolved", reference, None, message)
}

pub(crate) fn validation_error(
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
