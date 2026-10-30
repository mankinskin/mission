use super::*;

fn placeholder(intake: &SessionIntake) -> (String, String) {
    (
        format!("Planning Mission for intake {}", intake.record.request_id),
        format!(
            "Intake-derived placeholder, pending later planning. Session {} request {} input sha256:{}. No planning intent has been authored yet.",
            intake.record.session_id, intake.record.request_id, intake.record.input_digest
        ),
    )
}

pub(super) fn owned_draft(
    ws: &Path,
    store: &SessionStoreConfig,
    mission_id: Uuid,
    intake: &SessionIntake,
) -> Result<bool, IntakeError> {
    let id = mission_id.to_string();
    check_chain(ws, &[&id, INPUT_DIR])?;
    let lifecycle = MissionLifecycle::open(ws);
    let dir = lifecycle.mission_directory(mission_id);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_dir() => {}
        Ok(_) => {
            return Err(incompatible(format!(
                "{} is not a directory",
                dir.display()
            )));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(IntakeError::Storage {
                reason: format!("{}: {error}", dir.display()),
            });
        }
    }
    if let Some(bytes) = read_regular(&dir.join(INPUT_DIR).join(PROVENANCE_FILE))? {
        let parsed: ProvenanceRecord = serde_json::from_slice(&bytes)
            .map_err(|e| incompatible(format!("intake.json is invalid: {e}")))?;
        if parsed == expected_record(store, intake) {
            return Ok(true);
        }
        return Err(incompatible(
            "intake.json does not match this Session intake",
        ));
    }
    let (title, objective) = placeholder(intake);
    match lifecycle.get_mission_source(mission_id) {
        Ok(draft) if draft.title == title && draft.objective == objective => Ok(true),
        Ok(_) => Err(incompatible(format!(
            "Mission {mission_id} exists but is not the draft created for this intake; refusing adoption"
        ))),
        Err(MissionError::NotFound(_)) => Err(incompatible(format!(
            "Mission directory {mission_id} exists without a source; refusing adoption"
        ))),
        Err(error) => Err(IntakeError::Storage {
            reason: error.to_string(),
        }),
    }
}

pub(super) fn ensure_mission(
    ws: &Path,
    mission_id: Uuid,
    store: &SessionStoreConfig,
    intake: &SessionIntake,
) -> Result<(), Halt> {
    if owned_draft(ws, store, mission_id, intake)? {
        return Ok(());
    }
    let (title, objective) = placeholder(intake);
    match MissionLifecycle::open(ws).create_draft_with_id(mission_id, title, objective) {
        Ok(_) => Ok(()),
        Err(MissionError::StaleRevision { .. }) => {
            if owned_draft(ws, store, mission_id, intake)? {
                Ok(())
            } else {
                Err(incompatible(format!(
                    "Mission {mission_id} appeared concurrently and is not this intake's"
                ))
                .into())
            }
        }
        Err(error) => Err(Halt::Failed(error.to_string())),
    }
}
