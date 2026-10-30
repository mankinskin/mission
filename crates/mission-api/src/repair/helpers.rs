use super::*;

pub(super) fn merge_accepted_ownership(source: &mut TypedDraft, accepted: &TypedDraft) -> bool {
    let references: BTreeMap<_, _> = accepted
        .waypoints
        .iter()
        .map(|waypoint| {
            (
                waypoint.waypoint_id.as_str(),
                waypoint.ownership_refs.clone(),
            )
        })
        .collect();
    if source.waypoints.len() != references.len()
        || source
            .waypoints
            .iter()
            .any(|waypoint| !references.contains_key(waypoint.waypoint_id.as_str()))
    {
        return false;
    }
    for waypoint in &mut source.waypoints {
        if let Some(ownership) = references.get(waypoint.waypoint_id.as_str()) {
            waypoint.ownership_refs = ownership.clone();
        }
    }
    true
}

pub(super) fn matches_hash(expected: &str, observed: &Option<String>) -> bool {
    observed.as_deref() == Some(expected)
}

pub(super) fn fingerprint_files(
    directory: &Path,
) -> Result<BTreeMap<String, String>, MissionError> {
    let mut files = BTreeMap::new();
    collect_files(directory, directory, &mut files)?;
    Ok(files)
}

pub(super) fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), MissionError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(repair_error(
                "repair-payload-symlink",
                "Mission payload contains a symlink.",
            ));
        }
        let path = entry.path();
        if kind.is_dir() {
            collect_files(root, &path, files)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).map_err(|_| {
                repair_error("repair-payload-path-invalid", "payload escaped its root.")
            })?;
            let mut components = Vec::new();
            for component in relative.components() {
                match component {
                    Component::Normal(value) => components.push(
                        value
                            .to_str()
                            .ok_or_else(|| {
                                repair_error(
                                    "repair-payload-path-invalid",
                                    "payload path is not valid UTF-8.",
                                )
                            })?
                            .to_owned(),
                    ),
                    _ => {
                        return Err(repair_error(
                            "repair-payload-path-invalid",
                            "payload contains an unsafe relative path.",
                        ));
                    }
                }
            }
            files.insert(components.join("/"), sha256(&fs::read(&path)?));
        } else {
            return Err(repair_error(
                "repair-payload-type-invalid",
                "Mission payload contains a non-file, non-directory entry.",
            ));
        }
    }
    Ok(())
}

pub(super) fn payload_fingerprint(files: &BTreeMap<String, String>) -> String {
    let mut digest = Sha256::new();
    for (path, hash) in files {
        digest.update(path.as_bytes());
        digest.update([0]);
        digest.update(hash.as_bytes());
        digest.update([b'\n']);
    }
    format!("{:x}", digest.finalize())
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn projected_fingerprint(
    before: &BTreeMap<String, String>,
    accepted: &AcceptedMission,
    source: &TypedDraft,
    rendered: &RenderedRoadmap,
    lifecycle: &MissionLifecycle,
) -> Result<BTreeMap<String, String>, MissionError> {
    let mut after = before.clone();
    after.retain(|path, _| {
        !path.starts_with("parts/") && !path.starts_with("generated/") && path != "mission.md"
    });
    let accepted_bytes = serde_json::to_vec_pretty(accepted)?;
    let source_bytes = serde_json::to_vec_pretty(&stored_source(source))?;
    let revision = accepted.bundle.manifest.revision;
    after.insert("accepted.json".into(), sha256(&accepted_bytes));
    after.insert(
        format!("history/revision-{revision}.json"),
        sha256(&accepted_bytes),
    );
    after.insert("source.json".into(), sha256(&source_bytes));
    after.insert(
        format!("history/source-revision-{revision}.json"),
        sha256(&source_bytes),
    );
    for waypoint in &source.waypoints {
        after.insert(
            format!("parts/{}.json", waypoint.waypoint_id),
            sha256(&serde_json::to_vec_pretty(&waypoint.part)?),
        );
    }
    for (name, bytes) in rendered.files() {
        after.insert(format!("generated/{name}"), sha256(bytes));
    }
    after.insert(
        "mission.md".into(),
        sha256(lifecycle.render_typed(source).as_bytes()),
    );
    let provenance = MissionArtifactProvenance::for_write(accepted, Some(source), Some(rendered));
    after.insert(
        format!("history/provenance-revision-{revision}.json"),
        sha256(&serde_json::to_vec_pretty(&provenance)?),
    );
    extend_output_state(&mut after, accepted, Some(source), rendered, lifecycle)?;
    Ok(after)
}

pub(super) fn projected_rollback_fingerprint(
    before: &BTreeMap<String, String>,
    accepted: &AcceptedMission,
    source: Option<&TypedDraft>,
    rendered: &RenderedRoadmap,
    lifecycle: &MissionLifecycle,
) -> Result<BTreeMap<String, String>, MissionError> {
    let mut after = before.clone();
    after.retain(|path, _| {
        !path.starts_with("parts/")
            && !path.starts_with("generated/")
            && path != "mission.md"
            && path != "source.json"
            && path != "review-plan.json"
    });
    let accepted_bytes = serde_json::to_vec_pretty(accepted)?;
    let revision = accepted.bundle.manifest.revision;
    after.insert("accepted.json".into(), sha256(&accepted_bytes));
    after.insert(
        format!("history/revision-{revision}.json"),
        sha256(&accepted_bytes),
    );
    if let Some(source) = source {
        let source_bytes = serde_json::to_vec_pretty(&stored_source(source))?;
        after.insert("source.json".into(), sha256(&source_bytes));
        after.insert(
            format!("history/source-revision-{revision}.json"),
            sha256(&source_bytes),
        );
        for waypoint in &source.waypoints {
            after.insert(
                format!("parts/{}.json", waypoint.waypoint_id),
                sha256(&serde_json::to_vec_pretty(&waypoint.part)?),
            );
        }
        after.insert(
            "mission.md".into(),
            sha256(lifecycle.render_typed(source).as_bytes()),
        );
    }
    for (name, bytes) in rendered.files() {
        after.insert(format!("generated/{name}"), sha256(bytes));
    }
    let provenance = MissionArtifactProvenance::for_write(accepted, source, Some(rendered));
    after.insert(
        format!("history/provenance-revision-{revision}.json"),
        sha256(&serde_json::to_vec_pretty(&provenance)?),
    );
    extend_output_state(&mut after, accepted, source, rendered, lifecycle)?;
    Ok(after)
}

pub(super) fn extend_output_state(
    after: &mut BTreeMap<String, String>,
    accepted: &AcceptedMission,
    source: Option<&TypedDraft>,
    rendered: &RenderedRoadmap,
    lifecycle: &MissionLifecycle,
) -> Result<(), MissionError> {
    let directory = lifecycle.store.root.join(accepted.mission_id.to_string());
    let projected = super::output_history::projected_state(
        &directory,
        accepted,
        source,
        rendered,
        source.map(|source| lifecycle.render_typed(source)),
    )?;
    for (name, bytes) in projected {
        let digest = sha256(&bytes);
        after.insert(name.clone(), digest.clone());
        after.insert(
            format!(
                "history/states/revision-{}/{name}",
                accepted.bundle.manifest.revision
            ),
            digest,
        );
    }
    Ok(())
}

pub(super) fn copy_tree(source: &Path, destination: &Path) -> Result<(), MissionError> {
    reject_symlink_ancestors(destination)?;
    if destination.exists() {
        return Err(repair_error(
            "repair-snapshot-exists",
            "refusing to overwrite an existing recovery snapshot.",
        ));
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let kind = entry.file_type()?;
        let target = destination.join(entry.file_name());
        if kind.is_symlink() {
            return Err(repair_error(
                "repair-snapshot-symlink",
                "Mission payload contains a symlink.",
            ));
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            let mut input = File::open(entry.path())?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
        } else {
            return Err(repair_error(
                "repair-snapshot-type-invalid",
                "Mission payload contains an unsupported filesystem entry.",
            ));
        }
    }
    File::open(destination)?.sync_all()?;
    if let Some(parent) = destination.parent() {
        sync_directory_chain(parent)?;
    }
    Ok(())
}

pub(super) fn sync_directory_chain(directory: &Path) -> Result<(), MissionError> {
    let mut current = Some(directory);
    while let Some(path) = current {
        File::open(path)?.sync_all()?;
        current = path.parent();
    }
    Ok(())
}

pub(super) fn load_source_from_directory(
    mission_dir: &Path,
    mission_id: Uuid,
) -> Result<TypedDraft, MissionError> {
    let source_path = mission_dir.join("source.json");
    let stored: StoredSource = serde_json::from_slice(&fs::read(source_path)?)?;
    if stored.mission_id != mission_id {
        return Err(repair_error(
            "repair-snapshot-identity-mismatch",
            "snapshot typed source identity differs from its Mission ID.",
        ));
    }
    let mut waypoints = Vec::with_capacity(stored.waypoints.len());
    for waypoint in stored.waypoints {
        if waypoint.part_file != format!("parts/{}.json", waypoint.waypoint_id) {
            return Err(repair_error(
                "repair-snapshot-part-path-invalid",
                "snapshot typed Part path is not API-managed.",
            ));
        }
        let part: PartSource =
            serde_json::from_slice(&fs::read(mission_dir.join(waypoint.part_file))?)?;
        waypoints.push(TypedWaypoint {
            outcome_plan: waypoint.outcome_plan,
            waypoint_id: waypoint.waypoint_id,
            title: waypoint.title,
            prompt: waypoint.prompt,
            part,
            depends_on: waypoint.depends_on,
            validation: waypoint.validation,
            commit_checkpoint: waypoint.commit_checkpoint,
            non_goals: waypoint.non_goals,
            ownership_refs: waypoint.ownership_refs,
            relationships: waypoint.relationships,
        });
    }
    Ok(TypedDraft {
        mission_id: stored.mission_id,
        schema_version: stored.schema_version,
        revision: stored.revision,
        title: stored.title,
        objective: stored.objective,
        waypoints,
    })
}

pub(super) fn repair_error(code: &'static str, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "repair".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.into(),
        }],
    )
}

pub(super) fn repair_diagnostic(error: &MissionError) -> String {
    match error {
        MissionError::Validation(_, diagnostics) => diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("; "),
        _ => error.to_string(),
    }
}

pub(super) fn contains_symlink(path: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => return true,
            Ok(_) => {}
            Err(_) => return true,
        }
    }
    false
}

pub(super) fn reject_symlink_ancestors(path: &Path) -> Result<(), MissionError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(repair_error(
                    "repair-path-symlink",
                    "journal and snapshot paths must not contain symlinks.",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
