use super::*;
use sha2::{Digest, Sha256};
use std::{io::Write, time::SystemTime};

/// Additional request material; continuations are independent, create-new inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MissionOutputInputs {
    pub merged_request: Option<String>,
    #[serde(default)]
    pub continuations: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputFile {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub role: String,
    pub format: String,
}

/// Pre-reaction inventory. Its digest is a binding identity, not authentication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionOutputContract {
    pub schema_version: u32,
    pub mission_id: Uuid,
    pub revision: u64,
    pub revision_label: String,
    pub renderer: String,
    pub producer: String,
    pub purpose: String,
    pub files: Vec<OutputFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalDecision {
    Approve,
    ChangesRequested,
}

/// Caller-supplied reaction. Time and event identity are assigned on recording.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalEventRequest {
    pub revision: u64,
    pub decision: ApprovalDecision,
    /// `sha256:<digest>` of the exact contract JSON bytes, not a signature.
    pub signature: String,
    pub actor: String,
    pub actor_source: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionApprovalEvent {
    pub schema_version: u32,
    pub event_id: Uuid,
    pub mission_id: Uuid,
    pub recorded_at_unix_ms: u64,
    #[serde(flatten)]
    pub reaction: ApprovalEventRequest,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn timestamp() -> Result<u64, MissionError> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .ok_or_else(|| invalid("System clock cannot supply a recording timestamp."))
}

fn invalid(message: &str) -> MissionError {
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

pub(super) fn write_inputs(stage: &Path, inputs: &MissionOutputInputs) -> Result<(), MissionError> {
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

pub(super) fn capture(stage: &Path, accepted: &AcceptedMission) -> Result<(), MissionError> {
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
        renderer: RENDERER_VERSION.into(),
        producer: "mission-api".into(),
        purpose: "whole-state pre-approval inventory".into(),
        files: files
            .iter()
            .map(|(name, bytes)| file_record(name, bytes))
            .collect(),
    };
    Ok(serde_json::to_vec_pretty(&contract)?)
}

pub(super) fn projected_state(
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

fn validate_contract(
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

pub(super) fn history(root: Option<&Path>, revision: u64) -> Result<Vec<u8>, MissionError> {
    let mut text = String::from(
        "# Mission output history\n\nRevision labels identify whole states, not individual files. Missing states are never reconstructed from today's renderer. Human reactions are separate records, not publication locks or acceptance inferred from publication.\n\n",
    );
    // Render ranges rather than iterating unbounded missing revision numbers.
    let mut available = BTreeSet::new();
    if let Some(root) = root {
        let states = root.join("history/states");
        if states.exists() {
            typed_source::ensure_safe_path(root, &states)?;
            for entry in fs::read_dir(states)? {
                let entry = entry?;
                let number = entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.strip_prefix("revision-"))
                    .and_then(|number| number.parse::<u64>().ok())
                    .ok_or_else(|| {
                        invalid("Archived states require canonical revision-N labels.")
                    })?;
                if number == 0
                    || entry.file_name().to_str() != Some(format!("revision-{number}").as_str())
                    || !entry.file_type()?.is_dir()
                {
                    return Err(invalid(
                        "Archived state labels must be unique canonical directories, not aliases or links.",
                    ));
                }
                if number < revision && entry.path().join("output-contract.json").is_file() {
                    validate_contract(&entry.path(), self_id(root)?, number)?;
                    available.insert(number);
                }
            }
        }
    }
    available.insert(revision);
    let mut next = 1;
    for number in available {
        if number > next {
            if number == next + 1 {
                text.push_str(&format!(
                    "- revision-{next}: unrecoverable gap (no complete archived state).\n"
                ));
            } else {
                text.push_str(&format!("- revision-{next}..revision-{}: unrecoverable gap (no complete archived states).\n", number - 1));
            }
        }
        text.push_str(&format!("- [revision-{number}](../history/states/revision-{number}/output-contract.json): available complete state{}.\n",
            if number == revision { " (current)" } else { "" }));
        next = number.saturating_add(1);
    }
    text.push_str("\nReactions, when recorded: `../history/events/revision-N.json`. Missing reactions mean unknown; approvals are never reused for a later revision.\n");
    Ok(text.into_bytes())
}

fn self_id(root: &Path) -> Result<Uuid, MissionError> {
    let accepted: AcceptedMission = serde_json::from_slice(&fs::read(root.join("accepted.json"))?)?;
    Ok(accepted.mission_id)
}

impl MissionStore {
    /// Binding for the exact current JSON bytes; not a cryptographic signature.
    pub fn output_contract_binding(&self, mission_id: Uuid) -> Result<String, MissionError> {
        self.output_contract(mission_id)?;
        Ok(format!(
            "sha256:{}",
            digest(&fs::read(
                self.root
                    .join(mission_id.to_string())
                    .join("output-contract.json"),
            )?)
        ))
    }

    /// Read and validate all bytes of the current output contract.
    pub fn output_contract(&self, mission_id: Uuid) -> Result<MissionOutputContract, MissionError> {
        typed_source::ensure_safe_path(&self.root, &self.root.join(mission_id.to_string()))?;
        let accepted = self.get(mission_id)?;
        validate_contract(
            &self.root.join(mission_id.to_string()),
            mission_id,
            accepted.bundle.manifest.revision,
        )
    }

    /// Validate current and every available archived state; never repair or fabricate.
    pub fn verify_output_history(&self, mission_id: Uuid) -> Result<(), MissionError> {
        let directory = self.root.join(mission_id.to_string());
        let current = self.output_contract(mission_id)?;
        let _ = history(Some(&directory), current.revision.saturating_add(1))?;
        let archive = directory
            .join("history/states")
            .join(&current.revision_label);
        validate_contract(&archive, mission_id, current.revision)?;
        if fs::read(archive.join("output-contract.json"))?
            != fs::read(directory.join("output-contract.json"))?
        {
            return Err(invalid("Current and archived contracts differ."));
        }
        self.validate_preserved_events(mission_id)?;
        Ok(())
    }

    pub(super) fn validate_preserved_events(&self, mission_id: Uuid) -> Result<(), MissionError> {
        let directory = self.root.join(mission_id.to_string());
        let events = directory.join("history/events");
        if events.exists() {
            typed_source::ensure_safe_path(&directory, &events)?;
            for entry in fs::read_dir(events)? {
                let entry = entry?;
                typed_source::ensure_safe_path(&directory, &entry.path())?;
                let event: MissionApprovalEvent = serde_json::from_slice(&fs::read(entry.path())?)?;
                if entry.file_name().to_str()
                    != Some(format!("revision-{}.json", event.reaction.revision).as_str())
                {
                    return Err(invalid("Reaction path identity mismatch."));
                }
                self.validate_event(&event, &directory)?;
            }
        }
        Ok(())
    }

    fn validate_event(
        &self,
        event: &MissionApprovalEvent,
        directory: &Path,
    ) -> Result<(), MissionError> {
        if event.schema_version != 1
            || event.event_id.is_nil()
            || event.mission_id != self_id(directory)?
            || event.recorded_at_unix_ms == 0
            || event.reaction.actor.trim().is_empty()
            || event.reaction.actor_source.trim().is_empty()
        {
            return Err(invalid(
                "Reaction identity, actor and recording metadata are required.",
            ));
        }
        let archive = directory
            .join("history/states")
            .join(format!("revision-{}", event.reaction.revision));
        typed_source::ensure_safe_path(directory, &archive)?;
        validate_contract(&archive, event.mission_id, event.reaction.revision)?;
        if event.reaction.signature
            != format!(
                "sha256:{}",
                digest(&fs::read(archive.join("output-contract.json"))?)
            )
        {
            return Err(invalid("Reaction contract binding mismatch."));
        }
        Ok(())
    }

    /// Strict JSON input; one create-new reaction per current revision.
    /// Recording a reaction never changes Mission authority or grants execution.
    pub fn record_approval_event_json(
        &self,
        mission_id: Uuid,
        json: &[u8],
    ) -> Result<MissionApprovalEvent, MissionError> {
        let request: ApprovalEventRequest = serde_json::from_slice(json)?;
        self.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.root, mission_id)?;
        let contract = self.output_contract(mission_id)?;
        let directory = self.root.join(mission_id.to_string());
        self.verify_output_history(mission_id)?;
        if request.revision != contract.revision {
            return Err(invalid(
                "Reaction must bind to the current revision; historical approval reuse is forbidden.",
            ));
        }
        let event = MissionApprovalEvent {
            schema_version: 1,
            event_id: Uuid::new_v4(),
            mission_id,
            recorded_at_unix_ms: timestamp()?,
            reaction: request,
        };
        self.validate_event(&event, &directory)?;
        let events = directory.join("history/events");
        fs::create_dir_all(&events)?;
        typed_source::ensure_safe_path(&directory, &events)?;
        let destination = events.join(format!("revision-{}.json", contract.revision));
        let pending = events.join(format!(".{}.pending", event.event_id));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pending)?;
            file.write_all(&serde_json::to_vec_pretty(&event)?)?;
            file.sync_all()?;
            // Link is atomic and create-new, unlike rename which can replace a decision.
            fs::hard_link(&pending, destination)?;
            Ok::<_, MissionError>(())
        })();
        if pending.exists() {
            fs::remove_file(pending)?;
        }
        result?;
        fs::File::open(events)?.sync_all()?;
        Ok(event)
    }

    /// Read a recorded reaction and validate its exact immutable contract binding.
    pub fn approval_event(
        &self,
        mission_id: Uuid,
        revision: u64,
    ) -> Result<MissionApprovalEvent, MissionError> {
        let directory = self.root.join(mission_id.to_string());
        let path = directory
            .join("history/events")
            .join(format!("revision-{revision}.json"));
        typed_source::ensure_safe_path(&self.root, &path)?;
        let event: MissionApprovalEvent = serde_json::from_slice(&fs::read(path)?)?;
        if event.reaction.revision != revision || event.mission_id != mission_id {
            return Err(invalid("Reaction identity does not match its path."));
        }
        self.validate_event(&event, &directory)?;
        Ok(event)
    }
}
