use super::*;

pub(super) const CREATE_INTENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateIntent {
    pub(super) schema_version: u32,
    pub(super) mission_id: Uuid,
    pub(super) revision: u64,
    pub(super) request_sha256: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum IntentState {
    Absent,
    Matches,
    Conflicts,
}

pub(super) fn create_request_digest(
    accepted: &AcceptedMission,
    source: &TypedDraft,
) -> Result<String, MissionError> {
    let accepted = serde_json::to_vec_pretty(accepted)?;
    let source = serde_json::to_vec_pretty(&stored_source(source))?;
    Ok(format!(
        "{}+{}",
        super::output_history::digest(&accepted),
        super::output_history::digest(&source)
    ))
}

pub(super) fn read_create_intent(
    path: &Path,
    expected: &CreateIntent,
) -> Result<IntentState, MissionError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(IntentState::Absent);
        }
        Err(error) => return Err(error.into()),
        Ok(meta) if !meta.file_type().is_file() => return Ok(IntentState::Conflicts),
        Ok(_) => {}
    }
    let bytes = fs::read(path)?;
    Ok(match serde_json::from_slice::<CreateIntent>(&bytes) {
        Ok(found) if found == *expected => IntentState::Matches,
        _ => IntentState::Conflicts,
    })
}

pub(super) fn write_create_intent(
    root: &Path,
    path: &Path,
    expected: &CreateIntent,
) -> Result<(), MissionError> {
    use std::io::Write;
    let tmp = root.join(format!(
        ".{}.typed.intent.tmp-{}",
        expected.mission_id,
        Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(expected)?;
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()
    })();
    if let Err(error) = written {
        if let Err(cleanup) = fs::remove_file(&tmp) {
            eprintln!("mission-api: retained {}: {cleanup}", tmp.display());
        }
        return Err(error.into());
    }
    let linked = fs::hard_link(&tmp, path);
    if let Err(cleanup) = fs::remove_file(&tmp) {
        eprintln!("mission-api: retained {}: {cleanup}", tmp.display());
    }
    linked?;
    fs::File::open(root)?.sync_all()?;
    Ok(())
}

pub(super) fn forget_create_intent(path: Option<&Path>) {
    if let Some(path) = path
        && let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!(
            "mission-api: retained create intent {}: {error}",
            path.display()
        );
    }
}

pub(super) enum StageMatch {
    Same,
    OwnPartial,
    Foreign,
}
enum TreeEntry {
    Dir,
    File(Vec<u8>),
    Other,
}

fn tree_entries(
    root: &Path,
) -> Result<std::collections::BTreeMap<PathBuf, TreeEntry>, MissionError> {
    let mut out = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| typed_error("unsafe-part-path", "stage entry escaped its root."))?
                .to_path_buf();
            let meta = fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                out.insert(relative, TreeEntry::Other);
            } else if meta.is_dir() {
                out.insert(relative, TreeEntry::Dir);
                pending.push(path);
            } else if meta.is_file() {
                out.insert(relative, TreeEntry::File(fs::read(&path)?));
            } else {
                out.insert(relative, TreeEntry::Other);
            }
        }
    }
    Ok(out)
}

pub(super) fn compare_stage(stage: &Path, expected: &Path) -> Result<StageMatch, MissionError> {
    if !fs::symlink_metadata(stage)?.file_type().is_dir() {
        return Ok(StageMatch::Foreign);
    }
    let found = tree_entries(stage)?;
    let wanted = tree_entries(expected)?;
    let mut partial = found.len() != wanted.len();
    for (relative, entry) in &found {
        match (entry, wanted.get(relative)) {
            (TreeEntry::Dir, Some(TreeEntry::Dir)) => {}
            (TreeEntry::File(have), Some(TreeEntry::File(want))) => {
                if have != want {
                    if want.starts_with(have) {
                        partial = true;
                    } else {
                        return Ok(StageMatch::Foreign);
                    }
                }
            }
            _ => return Ok(StageMatch::Foreign),
        }
    }
    Ok(if partial {
        StageMatch::OwnPartial
    } else {
        StageMatch::Same
    })
}

pub(crate) fn ensure_safe_path(root: &Path, path: &Path) -> Result<(), MissionError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        typed_error(
            "unsafe-part-path",
            "managed paths must remain below the Mission root.",
        )
    })?;
    let mut current = root.to_path_buf();
    let root_metadata = fs::symlink_metadata(&current).map_err(MissionError::Io)?;
    if root_metadata.file_type().is_symlink() {
        return Err(typed_error(
            "unsafe-part-path",
            "managed source paths and their ancestors must not be symlinks.",
        ));
    }
    for component in relative.components() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(MissionError::Io)?;
        if metadata.file_type().is_symlink() {
            return Err(typed_error(
                "unsafe-part-path",
                "managed source paths and their ancestors must not be symlinks.",
            ));
        }
    }
    Ok(())
}
