use super::*;

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<bool> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent"))?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("intake"),
        Uuid::new_v4()
    ));
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(error) = written {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    let linked = fs::hard_link(&tmp, path);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => {
            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn put_immutable(path: &Path, bytes: &[u8]) -> Result<(), Halt> {
    for _ in 0..2 {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_file() => {
                let existing = fs::read(path).map_err(|e| Halt::Failed(e.to_string()))?;
                return if existing == bytes {
                    Ok(())
                } else {
                    Err(incompatible(format!(
                        "{} already exists with different bytes; refusing to overwrite",
                        path.display()
                    ))
                    .into())
                };
            }
            Ok(_) => {
                return Err(
                    incompatible(format!("{} is not a regular file", path.display())).into(),
                );
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if write_new(path, bytes).map_err(|e| Halt::Failed(e.to_string()))? {
                    return Ok(());
                }
            }
            Err(error) => return Err(Halt::Failed(error.to_string())),
        }
    }
    Err(Halt::Failed(format!("could not create {}", path.display())))
}

pub(super) fn expected_record(
    store: &SessionStoreConfig,
    intake: &SessionIntake,
) -> ProvenanceRecord {
    ProvenanceRecord {
        schema_version: PROVENANCE_SCHEMA_VERSION,
        session_id: intake.record.session_id.clone(),
        request_id: intake.record.request_id.clone(),
        input_digest: intake.record.input_digest.clone(),
        byte_len: intake.record.byte_len,
        session_store_root: store.root.clone(),
        session_input_path: intake.input_path.clone(),
        input_file: COPY_FILE.into(),
    }
}

pub(super) fn write_provenance(
    ws: &Path,
    store: &SessionStoreConfig,
    mission_id: Uuid,
    intake: &SessionIntake,
    input: &[u8],
) -> Result<(), Halt> {
    let id = mission_id.to_string();
    check_chain(ws, &[&id, INPUT_DIR])?;
    let dir = MissionLifecycle::open(ws)
        .mission_directory(mission_id)
        .join(INPUT_DIR);
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(Halt::Failed(error.to_string())),
    }
    check_chain(ws, &[&id, INPUT_DIR])?;
    put_immutable(&dir.join(COPY_FILE), input)?;
    let record = serde_json::to_vec_pretty(&expected_record(store, intake))
        .map_err(|e| Halt::Failed(e.to_string()))?;
    put_immutable(&dir.join(PROVENANCE_FILE), &record)
}

pub(super) fn read_regular(path: &Path) -> Result<Option<Vec<u8>>, IntakeError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => {
            fs::read(path).map(Some).map_err(|e| IntakeError::Storage {
                reason: format!("{}: {e}", path.display()),
            })
        }
        Ok(_) => Err(incompatible(format!(
            "{} is not a regular file",
            path.display()
        ))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(IntakeError::Storage {
            reason: format!("{}: {error}", path.display()),
        }),
    }
}

pub(super) fn provenance_complete(
    mission_dir: &Path,
    store: &SessionStoreConfig,
    intake: &SessionIntake,
) -> Result<bool, IntakeError> {
    let dir = mission_dir.join(INPUT_DIR);
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
    let Some(copy) = read_regular(&dir.join(COPY_FILE))? else {
        return Ok(false);
    };
    let session_bytes = read_regular(&intake.input_path)?
        .ok_or_else(|| incompatible("Session input bytes are missing"))?;
    if copy != session_bytes {
        return Err(incompatible(
            "Mission provenance copy differs from the Session input",
        ));
    }
    let Some(record) = read_regular(&dir.join(PROVENANCE_FILE))? else {
        return Ok(false);
    };
    let parsed: ProvenanceRecord = serde_json::from_slice(&record)
        .map_err(|e| incompatible(format!("intake.json is invalid: {e}")))?;
    if parsed != expected_record(store, intake) {
        return Err(incompatible(
            "intake.json does not match this Session intake",
        ));
    }
    Ok(true)
}
