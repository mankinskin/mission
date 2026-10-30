use super::*;

pub(super) fn persisted_state(
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

pub(super) fn generated_state(
    mission_dir: &Path,
) -> Result<(PathBuf, GeneratedPlanState), IntakeError> {
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

pub(super) fn build_receipt(
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
