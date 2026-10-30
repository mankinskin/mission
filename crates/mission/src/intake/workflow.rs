use super::*;

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
