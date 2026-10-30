use super::*;

pub(super) fn invalid(message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![contract_diag("mission-output-history-invalid", "", message)],
    )
}

/// Enumerate actual bytes, rejecting links and unsupported filesystem entries.
fn inventory(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, MissionError> {
    typed_source::ensure_safe_path(root, root)?;
    fn visit(
        root: &Path,
        directory: &Path,
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
            if metadata.is_dir() {
                visit(root, &path, files)?;
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
    visit(root, root, &mut files)?;
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
    let files = inventory(stage)?;
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
    let mut files = inventory(directory)?;
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
    let files = inventory(directory)?;
    let actual: Vec<_> = files
        .iter()
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
