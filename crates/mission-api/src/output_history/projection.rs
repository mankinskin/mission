use super::*;

pub(super) fn invalid(message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![contract_diag("mission-output-history-invalid", "", message)],
    )
}

/// Enumerate actual bytes, rejecting links and unsupported filesystem entries.
fn inventory(root: &Path, include_intent: bool) -> Result<BTreeMap<String, Vec<u8>>, MissionError> {
    typed_source::ensure_safe_path(root, root)?;
    fn visit(
        root: &Path,
        directory: &Path,
        include_intent: bool,
        files: &mut BTreeMap<String, Vec<u8>>,
    ) -> Result<(), MissionError> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| invalid("Invalid output root."))?;
            if relative == Path::new("history") || relative == Path::new("output-contract.json") {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(invalid("Output states must not contain symbolic links."));
            }
            if !include_intent && relative == Path::new("intent") && metadata.is_dir() {
                continue;
            }
            if metadata.is_dir() {
                visit(root, &path, include_intent, files)?;
            } else if metadata.is_file() {
                let name = relative
                    .to_str()
                    .ok_or_else(|| invalid("Output paths must be UTF-8."))?
                    .replace('\\', "/");
                files.insert(name, fs::read(path)?);
            } else {
                return Err(invalid("Output states must contain only regular files."));
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, root, include_intent, &mut files)?;
    Ok(files)
}

fn file_record(path: &str, bytes: &[u8]) -> OutputFile {
    OutputFile {
        path: path.into(),
        size_bytes: bytes.len() as u64,
        sha256: digest(bytes),
        role: if path.starts_with("inputs/")
            || path.starts_with("parts/")
            || matches!(path, "REQUEST.md" | "source.json")
        {
            "authored"
        } else {
            "generated"
        }
        .into(),
        format: if path.ends_with(".json") {
            "json"
        } else if path.ends_with(".md") {
            "markdown"
        } else {
            "bytes"
        }
        .into(),
    }
}

pub(crate) fn write_inputs(stage: &Path, inputs: &MissionOutputInputs) -> Result<(), MissionError> {
    for (name, contents) in &inputs.continuations {
        if name.is_empty()
            || name.contains('\\')
            || Path::new(name)
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(invalid(
                "Continuation names must be safe relative paths under inputs/.",
            ));
        }
        let destination = stage.join("inputs").join(name);
        if destination.exists() {
            return Err(invalid(
                "Continuation input names are create-new; never replace an earlier input.",
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, contents)?;
    }
    if let Some(request) = &inputs.merged_request {
        fs::write(stage.join("REQUEST.md"), request)?;
    }
    Ok(())
}

pub(crate) fn capture(stage: &Path, accepted: &AcceptedMission) -> Result<(), MissionError> {
    let revision = accepted.bundle.manifest.revision;
    let _ = history(Some(stage), revision)?;
    let label = format!("revision-{revision}");
    let archive = stage.join("history/states").join(&label);
    fs::create_dir_all(stage.join("history/states"))?;
    // create_dir, not create_dir_all: even an empty occupied label is a collision.
    fs::create_dir(&archive)?;
    let files = inventory(stage, false)?;
    let bytes = contract_bytes(accepted, &files)?;
    for (name, contents) in files {
        let target = archive.join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, contents)?;
    }
    fs::write(archive.join("output-contract.json"), &bytes)?;
    fs::write(stage.join("output-contract.json"), bytes)?;
    validate_contract(stage, accepted.mission_id, revision)?;
    validate_contract(&archive, accepted.mission_id, revision)?;
    Ok(())
}

fn contract_bytes(
    accepted: &AcceptedMission,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, MissionError> {
    let revision = accepted.bundle.manifest.revision;
    let contract = MissionOutputContract {
        schema_version: 1,
        mission_id: accepted.mission_id,
        revision,
        revision_label: format!("revision-{revision}"),
        renderer: outcome_plan::renderer_version(&accepted.bundle).into(),
        producer: "mission-api".into(),
        purpose: "whole-state pre-approval inventory".into(),
        files: files
            .iter()
            .map(|(name, bytes)| file_record(name, bytes))
            .collect(),
    };
    Ok(serde_json::to_vec_pretty(&contract)?)
}

pub(crate) fn projected_state(
    directory: &Path,
    accepted: &AcceptedMission,
    source: Option<&TypedDraft>,
    rendered: &RenderedRoadmap,
    mission_markdown: Option<String>,
) -> Result<BTreeMap<String, Vec<u8>>, MissionError> {
    let mut files = inventory(directory, false)?;
    files.retain(|name, _| {
        !name.starts_with("generated/")
            && !name.starts_with("parts/")
            && !matches!(name.as_str(), "mission.md" | "source.json")
    });
    files.insert("accepted.json".into(), serde_json::to_vec_pretty(accepted)?);
    if let Some(source) = source {
        files.insert(
            "source.json".into(),
            serde_json::to_vec_pretty(&stored_source(source))?,
        );
        for waypoint in &source.waypoints {
            files.insert(
                format!("parts/{}.json", waypoint.waypoint_id),
                serde_json::to_vec_pretty(&waypoint.part)?,
            );
        }
    }
    let ledger = plan_operations::ledger_from_bytes(
        files.get(plan_operations::EVIDENCE_FILE).map(Vec::as_slice),
    )?;
    files.remove(plan_operations::REVIEW_PLAN_FILE);
    if let Some(source) = source {
        let plan = plan_operations::build_review_plan(accepted, source, &ledger)?;
        files.insert(
            plan_operations::REVIEW_PLAN_FILE.into(),
            serde_json::to_vec_pretty(&plan)?,
        );
    }
    for (name, bytes) in rendered.files() {
        files.insert(format!("generated/{name}"), bytes.to_vec());
    }
    if let Some(markdown) = mission_markdown {
        files.insert("mission.md".into(), markdown.into_bytes());
    }
    files.insert(
        "artifact-provenance.json".into(),
        serde_json::to_vec_pretty(&MissionArtifactProvenance::for_write(
            accepted,
            source,
            Some(rendered),
        ))?,
    );
    files.insert(
        "output-contract.json".into(),
        contract_bytes(accepted, &files)?,
    );
    Ok(files)
}

pub(super) fn validate_contract(
    directory: &Path,
    mission_id: Uuid,
    revision: u64,
) -> Result<MissionOutputContract, MissionError> {
    validate_contract_inner(directory, mission_id, revision, false)
}

pub(super) fn validate_live_contract(
    directory: &Path,
    mission_id: Uuid,
    revision: u64,
) -> Result<MissionOutputContract, MissionError> {
    validate_contract_inner(directory, mission_id, revision, true)
}

fn validate_contract_inner(
    directory: &Path,
    mission_id: Uuid,
    revision: u64,
    allow_journal_appends: bool,
) -> Result<MissionOutputContract, MissionError> {
    typed_source::ensure_safe_path(directory, &directory.join("output-contract.json"))?;
    let contract: MissionOutputContract =
        serde_json::from_slice(&fs::read(directory.join("output-contract.json"))?)?;
    if contract.schema_version != 1
        || contract.mission_id != mission_id
        || contract.revision != revision
        || contract.revision_label != format!("revision-{revision}")
        || contract.renderer.trim().is_empty()
        || contract.producer != "mission-api"
        || contract.purpose != "whole-state pre-approval inventory"
    {
        return Err(invalid(
            "Contract metadata does not match the requested state.",
        ));
    }
    let includes_intent = contract
        .files
        .iter()
        .any(|file| file.path == "intent" || file.path.starts_with("intent/"));
    let mut files = inventory(directory, includes_intent)?;
    if allow_journal_appends {
        validate_native_journal(&files, mission_id)?;
        validate_control_appends(directory, &contract, &mut files)?;
    }
    let actual: Vec<_> = files
        .iter()
        .filter(|(name, _)| {
            !allow_journal_appends
                || !(name.starts_with("approvals/journal/")
                    || name.starts_with("approvals/controls/"))
                || contract.files.iter().any(|file| file.path == **name)
        })
        .map(|(name, bytes)| file_record(name, bytes))
        .collect();
    if contract.files != actual {
        return Err(invalid(
            "Complete BOM, file digest, size, path or metadata mismatch.",
        ));
    }
    let accepted: AcceptedMission = serde_json::from_slice(
        files
            .get("accepted.json")
            .ok_or_else(|| invalid("Contract is missing accepted.json."))?,
    )?;
    if accepted.mission_id != mission_id || accepted.bundle.manifest.revision != revision {
        return Err(invalid("Contract authority identity mismatch."));
    }
    Ok(contract)
}

fn validate_control_appends(
    directory: &Path,
    contract: &MissionOutputContract,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), MissionError> {
    let controls: BTreeMap<_, _> = files
        .iter()
        .filter_map(|(path, bytes)| {
            path.strip_prefix("approvals/controls/")
                .map(|name| (name.to_owned(), bytes.clone()))
        })
        .collect();
    crate::lifecycle::replay_controls(controls.keys().cloned().collect(), |name| {
        controls
            .get(name)
            .cloned()
            .ok_or_else(|| crate::LifecycleError::JournalCorrupt(name.into()))
    })
    .map_err(|error| invalid(&format!("Invalid native control journal: {error}")))?;
    let path = crate::lifecycle::LIFECYCLE_JOURNAL_FILE;
    if let Some(bytes) = files.get(path) {
        let journal = crate::LifecycleJournal::from_json(
            std::str::from_utf8(bytes).map_err(|_| invalid("Control journal must be UTF-8."))?,
        )
        .map_err(|error| invalid(&error.to_string()))?;
        if let Some(record) = contract.files.iter().find(|file| file.path == path) {
            let archived = directory
                .join("history/states")
                .join(&contract.revision_label)
                .join(path);
            typed_source::ensure_safe_path(directory, &archived)?;
            let baseline = fs::read(&archived)?;
            if file_record(path, &baseline) != *record {
                return Err(invalid(
                    "Archived control journal does not match the output contract.",
                ));
            }
            let previous = crate::LifecycleJournal::from_json(
                std::str::from_utf8(&baseline)
                    .map_err(|_| invalid("Archived control journal must be UTF-8."))?,
            )
            .map_err(|error| invalid(&error.to_string()))?;
            if !journal.events.starts_with(&previous.events) {
                return Err(invalid(
                    "Control journal changed its snapshotted event prefix.",
                ));
            }
            files.insert(path.into(), baseline);
        } else {
            files.remove(path);
        }
    }
    Ok(())
}

fn validate_native_journal(
    files: &BTreeMap<String, Vec<u8>>,
    mission_id: Uuid,
) -> Result<(), MissionError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct JournalRecord {
        schema_version: u32,
        mission_id: Uuid,
        seq: u64,
        entry: serde_json::Value,
    }
    for (index, (path, bytes)) in files
        .iter()
        .filter(|(path, _)| path.starts_with("approvals/journal/"))
        .enumerate()
    {
        let seq = index as u64 + 1;
        let record: JournalRecord = serde_json::from_slice(bytes).map_err(|error| {
            invalid(&format!("Invalid native approval journal {path}: {error}"))
        })?;
        if *path != format!("approvals/journal/{seq:08}.json")
            || record.schema_version != 1
            || record.mission_id != mission_id
            || record.seq != seq
            || !matches!(
                record.entry.get("type").and_then(serde_json::Value::as_str),
                Some("review" | "challenge" | "grant" | "decision" | "invalidated")
            )
        {
            return Err(invalid(&format!(
                "Invalid native approval journal identity: {path}"
            )));
        }
    }
    Ok(())
}
