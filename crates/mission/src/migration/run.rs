use super::*;

pub fn migrate_dossiers(
    workspace_root: &Path,
    dossier_path: Option<&Path>,
    dry_run: bool,
) -> Result<DossierMigrationReport, MissionError> {
    let workspace_root = workspace_root.canonicalize()?;
    let transcripts_root = workspace_root.join("transcripts");
    let mut dossiers = if let Some(dossier_path) = dossier_path {
        let selected = if dossier_path.is_absolute() {
            dossier_path.to_path_buf()
        } else {
            workspace_root.join(dossier_path)
        };
        let selected = selected.canonicalize()?;
        if !selected.starts_with(&transcripts_root) || !selected.is_dir() {
            return Err(invalid_migration_input(
                selected,
                "Select a dossier directory inside the workspace transcripts/ folder.",
            ));
        }
        vec![selected]
    } else if transcripts_root.is_dir() {
        let mut directories = Vec::new();
        for entry in fs::read_dir(&transcripts_root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                directories.push(entry.path());
            }
        }
        directories
    } else {
        Vec::new()
    };
    dossiers.sort();

    let store = MissionStore::open(&workspace_root);
    let mut report = DossierMigrationReport {
        dry_run,
        ..DossierMigrationReport::default()
    };
    for dossier in dossiers {
        let source_path = dossier
            .strip_prefix(&workspace_root)
            .unwrap_or(&dossier)
            .to_string_lossy()
            .replace('\\', "/");
        let mission_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("workflow-tools-mission-dossier-v1:{source_path}").as_bytes(),
        );
        let outcome = migrate_one(
            &workspace_root,
            &dossier,
            &source_path,
            mission_id,
            &store,
            dry_run,
        );
        let (outcome, reason) = match outcome {
            Ok((outcome, reason)) => (outcome, reason),
            Err(reason) => ("blocked", Some(reason)),
        };
        match outcome {
            "migrated" => report.migrated += 1,
            "would-migrate" => report.would_migrate += 1,
            "skipped" => report.skipped += 1,
            "blocked" => report.blocked += 1,
            _ => {}
        }
        report.records.push(DossierMigrationRecord {
            source_path,
            mission_id: Some(mission_id),
            outcome: outcome.into(),
            reason,
        });
    }
    Ok(report)
}

pub(super) fn migrate_one(
    workspace_root: &Path,
    dossier: &Path,
    source_path: &str,
    mission_id: Uuid,
    store: &MissionStore,
    dry_run: bool,
) -> Result<(&'static str, Option<String>), String> {
    let roadmap_path = dossier.join("ROADMAP.md");
    if !roadmap_path.is_file() {
        return Ok(("skipped", Some("No current ROADMAP.md was found.".into())));
    }

    let mut bundle = parse_roadmap(workspace_root, dossier, &roadmap_path, source_path, 1)?;
    let inputs = dossier_inputs(dossier)?;
    for historical_path in historical_roadmap_paths(dossier)? {
        let relative_path = historical_path
            .strip_prefix(workspace_root)
            .unwrap_or(&historical_path)
            .to_string_lossy()
            .replace('\\', "/");
        bundle
            .manifest
            .notes
            .push(format!("legacy-history-excluded: {relative_path}"));
    }
    let marker = format!("legacy-source: {source_path}");
    let current = match store.get(mission_id) {
        Ok(accepted) => Some(accepted),
        Err(MissionError::NotFound(_)) => None,
        Err(error) => return Err(format!("Could not read the existing mission: {error}")),
    };

    let actual_revision = if let Some(accepted) = current.as_ref() {
        if !accepted
            .bundle
            .manifest
            .notes
            .iter()
            .any(|note| note == &marker)
        {
            return Err(format!(
                "Deterministic mission ID {mission_id} is already used by a record with different provenance."
            ));
        }
        let actual_revision = accepted.bundle.manifest.revision;
        if actual_revision != 1 {
            return Err(format!(
                "Stored revision {actual_revision} does not match the current-roadmap-only migration."
            ));
        }
        if accepted.bundle != bundle {
            return Err(format!(
                "Stored revision {actual_revision} differs from its source snapshot; refusing to overwrite it."
            ));
        }
        actual_revision
    } else {
        0
    };

    let snapshot = crate::ticket_snapshot(workspace_root, store, mission_id, &bundle)
        .map_err(|error| format!("Could not resolve external status: {error}"))?;
    let states = crate::snapshot_states(&snapshot)
        .map_err(|error| format!("Could not map external status: {error}"))?;
    let snapshot_id = crate::snapshot_id(&snapshot)
        .map_err(|error| format!("Could not identify external status snapshot: {error}"))?;

    if dry_run {
        return if actual_revision == 1 {
            Ok(("skipped", Some("This source is already migrated.".into())))
        } else {
            Ok(("would-migrate", None))
        };
    }

    if actual_revision == 1 {
        return Ok(("skipped", Some("This source is already migrated.".into())));
    }

    store
        .publish_with_inputs(
            mission_id,
            bundle,
            0,
            &inputs,
            Some((&states, &snapshot_id)),
        )
        .map_err(|error| format!("Could not publish the migrated mission: {error}"))?;
    Ok(("migrated", None))
}

pub(super) fn dossier_inputs(dossier: &Path) -> Result<MissionOutputInputs, String> {
    fn read(path: &Path) -> Result<String, String> {
        let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "Request input must be a regular file: {}",
                path.display()
            ));
        }
        fs::read_to_string(path).map_err(|error| error.to_string())
    }
    fn collect(
        root: &Path,
        directory: &Path,
        inputs: &mut BTreeMap<String, String>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if kind.is_symlink() {
                return Err(format!(
                    "Request inputs must not contain links: {}",
                    path.display()
                ));
            }
            if kind.is_dir() {
                collect(root, &path, inputs)?;
            } else {
                let name = path
                    .strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                inputs.insert(name, read(&path)?);
            }
        }
        Ok(())
    }
    let mut inputs = MissionOutputInputs::default();
    if dossier.join("REQUEST.md").exists() {
        inputs.merged_request = Some(read(&dossier.join("REQUEST.md"))?);
    }
    let root = dossier.join("inputs");
    if root.exists() {
        if fs::symlink_metadata(&root)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Dossier inputs/ must not be a symbolic link.".into());
        }
        collect(&root, &root, &mut inputs.continuations)?;
    }
    Ok(inputs)
}

pub(super) fn historical_roadmap_paths(dossier: &Path) -> Result<Vec<PathBuf>, String> {
    let mut historical = Vec::new();
    for entry in fs::read_dir(dossier).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && name
                .strip_prefix("ROADMAP.v")
                .and_then(|name| name.strip_suffix(".md"))
                .is_some_and(|number| {
                    !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
                })
        {
            historical.push(entry.path());
        }
    }
    historical.sort();
    Ok(historical)
}
