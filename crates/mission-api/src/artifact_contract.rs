use super::*;
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path},
};

pub const ARTIFACT_PROVENANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MissionFileRole {
    Authored,
    Generated,
    Helper,
}

fn validate_json_output(
    provenance: &MissionArtifactProvenance,
    relative_path: &str,
    bytes: &[u8],
) -> Result<(), MissionError> {
    let path = Path::new(relative_path);
    let invalid =
        |message: &str| validation_error("mission-artifact-schema-invalid", path, message);
    if relative_path == "accepted.json" {
        let value: AcceptedMission = serde_json::from_slice(bytes)
            .map_err(|_| invalid("Accepted data must satisfy the strict Mission schema."))?;
        if value.mission_id != provenance.mission_id {
            return Err(invalid(
                "Accepted data must belong to the declared Mission.",
            ));
        }
    } else if let Some(revision) = relative_path
        .strip_prefix("history/revision-")
        .and_then(|value| value.strip_suffix(".json"))
    {
        let value: AcceptedMission = serde_json::from_slice(bytes).map_err(|_| {
            invalid("Historical accepted data must satisfy the strict Mission schema.")
        })?;
        if value.mission_id.to_string() != provenance.mission_id.to_string()
            || revision.parse::<u64>().ok() != Some(value.bundle.manifest.revision)
        {
            return Err(invalid(
                "Historical accepted data must match its immutable path identity.",
            ));
        }
    } else if relative_path == "source.json"
        || relative_path.starts_with("history/source-revision-")
    {
        let value: StoredSource = serde_json::from_slice(bytes)
            .map_err(|_| invalid("Typed source data must satisfy its strict schema."))?;
        if value.mission_id != provenance.mission_id {
            return Err(invalid("Typed source must belong to the declared Mission."));
        }
        if let Some(revision) = relative_path
            .strip_prefix("history/source-revision-")
            .and_then(|value| value.strip_suffix(".json"))
            && revision.parse::<u64>().ok() != Some(value.revision)
        {
            return Err(invalid(
                "Historical typed source must match its immutable path identity.",
            ));
        }
    } else if let Some(name) = relative_path.strip_prefix("parts/")
        && let Some(id) = name.strip_suffix(".json")
    {
        let value: PartSource = serde_json::from_slice(bytes)
            .map_err(|_| invalid("Authored Part data must satisfy its strict schema."))?;
        if value.waypoint_id != id {
            return Err(invalid("Authored Part identity must match its file name."));
        }
    } else if relative_path.starts_with("history/provenance-revision-") {
        let value: MissionArtifactProvenance = serde_json::from_slice(bytes)
            .map_err(|_| invalid("Provenance data must satisfy its strict schema."))?;
        if value.mission_id != provenance.mission_id
            || value.revision
                != relative_path
                    .strip_prefix("history/provenance-revision-")
                    .and_then(|value| value.strip_suffix(".json"))
                    .and_then(|value| value.parse::<u64>().ok())
                    .unwrap_or_default()
        {
            return Err(invalid(
                "Provenance identity must match its immutable path.",
            ));
        }
    } else {
        let _: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| invalid("Declared JSON outputs must contain valid JSON."))?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MissionFileFormat {
    Json,
    Markdown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionFileProvenance {
    pub path: String,
    pub role: MissionFileRole,
    pub format: MissionFileFormat,
    pub schema_guarantees: Vec<String>,
    pub immutable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionArtifactProvenance {
    pub schema_version: u32,
    pub mission_id: Uuid,
    pub revision: u64,
    pub producer: String,
    pub purpose: String,
    pub invocation: String,
    pub grouping: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub files: Vec<MissionFileProvenance>,
}

impl MissionArtifactProvenance {
    pub fn validate(&self) -> Result<(), Vec<Diagnostic>> {
        let mut diagnostics = Vec::new();
        if self.schema_version != ARTIFACT_PROVENANCE_SCHEMA_VERSION {
            diagnostics.push(artifact_diag(
                "mission-provenance-schema-unsupported",
                "schema_version",
                "Only artifact provenance schema version 1 is supported.",
            ));
        }
        if self.revision == 0
            || self.producer.trim().is_empty()
            || self.purpose.trim().is_empty()
            || self.invocation.trim().is_empty()
            || self.grouping.trim().is_empty()
            || self.inputs.is_empty()
            || self.inputs.iter().any(|input| input.trim().is_empty())
        {
            diagnostics.push(artifact_diag(
                "mission-provenance-incomplete",
                "provenance",
                "Producer, purpose, invocation, grouping, and at least one input are required.",
            ));
        }

        let mut paths = BTreeSet::new();
        for file in &self.files {
            if !safe_manifest_path(&file.path) {
                diagnostics.push(artifact_diag(
                    "mission-artifact-path-invalid",
                    &file.path,
                    "Artifact paths must be relative, normalized paths inside the Mission directory.",
                ));
                continue;
            }
            if !paths.insert(file.path.as_str()) {
                diagnostics.push(artifact_diag(
                    "mission-artifact-path-duplicate",
                    &file.path,
                    "Each output path must be declared once.",
                ));
            }
            if file.schema_guarantees.is_empty()
                || file
                    .schema_guarantees
                    .iter()
                    .any(|guarantee| guarantee.trim().is_empty())
            {
                diagnostics.push(artifact_diag(
                    "mission-artifact-schema-missing",
                    &file.path,
                    "Every output must declare a nonblank format or schema guarantee.",
                ));
            }
            if let Some((role, format, guarantee, immutable)) =
                known_file_contract(&file.path, self.revision)
            {
                if file.role != role {
                    diagnostics.push(artifact_diag(
                        "mission-artifact-role-invalid",
                        &file.path,
                        "The declared file role does not match the Mission file contract.",
                    ));
                }
                if file.format != format {
                    diagnostics.push(artifact_diag(
                        "mission-artifact-format-unsupported",
                        &file.path,
                        "The declared format does not match the supported file contract.",
                    ));
                }
                if !file.schema_guarantees.iter().any(|item| item == guarantee) {
                    diagnostics.push(artifact_diag(
                        "mission-artifact-schema-invalid",
                        &file.path,
                        "The declared schema guarantee does not match the supported contract.",
                    ));
                }
                if file.immutable != immutable {
                    diagnostics.push(artifact_diag(
                        "mission-artifact-mutability-invalid",
                        &file.path,
                        "Historical evidence must be immutable and current files must remain writable.",
                    ));
                }
            } else if file.role != MissionFileRole::Helper || !helper_format_matches(file) {
                diagnostics.push(artifact_diag(
                    "mission-artifact-unknown-file",
                    &file.path,
                    "Unknown Mission files require an explicitly declared helper role and supported format.",
                ));
            }
        }

        let output_paths: BTreeSet<_> = self.outputs.iter().map(String::as_str).collect();
        if self.outputs.is_empty()
            || self.outputs.iter().any(|output| output.trim().is_empty())
            || output_paths.len() != self.outputs.len()
            || output_paths != paths
        {
            diagnostics.push(artifact_diag(
                "mission-provenance-output-mismatch",
                "outputs",
                "The output list must name every declared file exactly once.",
            ));
        }

        let expected_group = format!("mission:{}/revision:{}", self.mission_id, self.revision);
        if self.grouping != expected_group {
            diagnostics.push(artifact_diag(
                "mission-provenance-group-invalid",
                "grouping",
                "Grouping must identify the owning Mission and revision.",
            ));
        }

        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }

    pub(super) fn for_write(
        accepted: &AcceptedMission,
        source: Option<&TypedDraft>,
        rendered: Option<&RenderedRoadmap>,
    ) -> Self {
        let revision = accepted.bundle.manifest.revision;
        let mut files = vec![
            file_record(
                "accepted.json",
                MissionFileRole::Generated,
                MissionFileFormat::Json,
                "mission-accepted/1",
                false,
            ),
            file_record(
                format!("history/revision-{revision}.json"),
                MissionFileRole::Generated,
                MissionFileFormat::Json,
                "mission-accepted/1",
                true,
            ),
        ];
        if let Some(source) = source {
            files.push(file_record(
                "source.json",
                MissionFileRole::Authored,
                MissionFileFormat::Json,
                "mission-typed-source/2",
                false,
            ));
            files.push(file_record(
                format!("history/source-revision-{}.json", source.revision),
                MissionFileRole::Authored,
                MissionFileFormat::Json,
                "mission-typed-source/2",
                true,
            ));
            for waypoint in &source.waypoints {
                files.push(file_record(
                    format!("parts/{}.json", waypoint.waypoint_id),
                    MissionFileRole::Authored,
                    MissionFileFormat::Json,
                    "mission-part-source/1",
                    false,
                ));
            }
        }
        if let Some(rendered) = rendered {
            for (name, _) in rendered.files() {
                files.push(file_record(
                    format!("generated/{name}"),
                    MissionFileRole::Generated,
                    MissionFileFormat::Markdown,
                    "utf-8-markdown/1",
                    false,
                ));
            }
            if source.is_some() {
                files.push(file_record(
                    "mission.md",
                    MissionFileRole::Generated,
                    MissionFileFormat::Markdown,
                    "utf-8-markdown/1",
                    false,
                ));
            }
        }
        let provenance_path = format!("history/provenance-revision-{revision}.json");
        files.push(file_record(
            &provenance_path,
            MissionFileRole::Generated,
            MissionFileFormat::Json,
            "mission-artifact-provenance/1",
            true,
        ));
        let outputs = files.iter().map(|file| file.path.clone()).collect();
        Self {
            schema_version: ARTIFACT_PROVENANCE_SCHEMA_VERSION,
            mission_id: accepted.mission_id,
            revision,
            producer: "workflow-tools/mission-api".into(),
            purpose: "Persist a Mission revision and its declared projections".into(),
            invocation: "MissionStore::write_authority".into(),
            grouping: format!("mission:{}/revision:{revision}", accepted.mission_id),
            inputs: if source.is_some() {
                vec!["source.json".into()]
            } else {
                vec!["in-memory:MissionBundle".into()]
            },
            outputs,
            files,
        }
    }
}

pub(super) fn validate_file_outputs(
    provenance: &MissionArtifactProvenance,
    mission_directory: &Path,
    immutable_only: bool,
) -> Result<(), MissionError> {
    let root = mission_directory.canonicalize()?;
    for file in &provenance.files {
        if immutable_only && !file.immutable {
            continue;
        }
        let path = mission_directory.join(&file.path);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(validation_error(
                    "mission-artifact-missing",
                    &path,
                    "A declared immutable output is missing.",
                ));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(validation_error(
                "mission-artifact-destination-invalid",
                &path,
                "Declared outputs must be regular files.",
            ));
        }
        let canonical = path.canonicalize()?;
        if !canonical.starts_with(&root) {
            return Err(validation_error(
                "mission-artifact-destination-invalid",
                &path,
                "Declared outputs must remain inside their Mission directory.",
            ));
        }
        let bytes = fs::read(&path)?;
        match file.format {
            MissionFileFormat::Json => {
                validate_json_output(provenance, &file.path, &bytes)?;
            }
            MissionFileFormat::Markdown => {
                if std::str::from_utf8(&bytes).is_err() {
                    return Err(validation_error(
                        "mission-artifact-format-invalid",
                        &path,
                        "Markdown outputs must be valid UTF-8.",
                    ));
                }
            }
        }
    }
    Ok(())
}

impl MissionStore {
    pub fn get_artifact_provenance(
        &self,
        mission_id: Uuid,
        revision: u64,
    ) -> Result<MissionArtifactProvenance, MissionError> {
        let path = self
            .root
            .join(mission_id.to_string())
            .join("history")
            .join(format!("provenance-revision-{revision}.json"));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(MissionError::NotFound(mission_id));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(validation_error(
                "mission-provenance-destination-invalid",
                &path,
                "Provenance records must be regular files.",
            ));
        }
        let provenance: MissionArtifactProvenance = serde_json::from_slice(&fs::read(path)?)?;
        if provenance.mission_id != mission_id || provenance.revision != revision {
            return Err(validation_error(
                "mission-provenance-identity-mismatch",
                &self.root,
                "Provenance identity must match its Mission and revision path.",
            ));
        }
        provenance
            .validate()
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let mission_directory = self.root.join(mission_id.to_string());
        validate_file_outputs(&provenance, &mission_directory, true)?;
        Ok(provenance)
    }

    pub(super) fn validate_artifact_references(
        &self,
        mission_id: Uuid,
        bundle: &MissionBundle,
    ) -> Result<(), MissionError> {
        let workspace = self.root.ancestors().nth(3).unwrap_or(&self.root);
        let generated = Path::new(".workflow-tools")
            .join("mission")
            .join("missions")
            .join(mission_id.to_string())
            .join("generated");
        let consumers = [generated.join("README.md"), generated.join("ROADMAP.md")];
        let references = bundle
            .manifest
            .artifacts
            .iter()
            .chain(
                bundle
                    .waypoints
                    .iter()
                    .flat_map(|waypoint| &waypoint.artifacts),
            )
            .chain(
                bundle
                    .manifest
                    .operator_results
                    .iter()
                    .map(|result| &result.evidence_pointer),
            );
        let mut diagnostics = Vec::new();
        for reference in references {
            for consumer in &consumers {
                if let Err(diagnostic) = resolve_artifact_reference(workspace, consumer, reference)
                {
                    diagnostics.push(diagnostic);
                }
            }
        }
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(MissionError::Validation(diagnostics.len(), diagnostics))
        }
    }
}

pub fn resolve_artifact_reference(
    workspace_root: &Path,
    consumer_path: &Path,
    reference: &str,
) -> Result<String, Diagnostic> {
    if reference.starts_with("ce://") {
        return validate_canonical_uri(reference).then(|| reference.to_owned()).ok_or_else(|| {
            reference_diag(
                "mission-reference-unknown",
                consumer_path,
                reference,
                "Canonical entity references must have workspace, store and identifier segments.",
            )
        });
    }
    if reference.is_empty() || reference.contains(':') || reference.contains('\\') {
        return Err(reference_diag(
            "mission-reference-unknown",
            consumer_path,
            reference,
            "Only workspace-relative files and canonical ce:// entity references are supported.",
        ));
    }

    let root = workspace_root.canonicalize().map_err(|_| {
        reference_diag(
            "mission-reference-wrong-root",
            workspace_root,
            reference,
            "The owning workspace root must exist and resolve to a directory.",
        )
    })?;
    let consumer = relative_to_root(&root, consumer_path).ok_or_else(|| {
        reference_diag(
            "mission-reference-wrong-root",
            consumer_path,
            reference,
            "The consumer destination must remain inside its owning workspace.",
        )
    })?;
    if !safe_relative_path(&consumer) {
        return Err(reference_diag(
            "mission-reference-escaping",
            consumer_path,
            reference,
            "The consumer destination must not escape its owning workspace.",
        ));
    }
    ensure_existing_ancestors_inside(&root, &consumer).map_err(|code| {
        reference_diag(
            code,
            consumer_path,
            reference,
            "The consumer destination resolves outside its owning workspace.",
        )
    })?;

    let target = Path::new(reference);
    let relative_target = if target.is_absolute() {
        match target.strip_prefix(&root) {
            Ok(relative) => relative.to_path_buf(),
            Err(_) => {
                return Err(reference_diag(
                    "mission-reference-wrong-root",
                    target,
                    reference,
                    "Absolute artifact references must belong to the owning workspace.",
                ));
            }
        }
    } else {
        target.to_path_buf()
    };
    if !safe_relative_path(&relative_target) {
        return Err(reference_diag(
            "mission-reference-escaping",
            target,
            reference,
            "Artifact references must not escape the owning workspace.",
        ));
    }
    let resolved = root.join(&relative_target);
    let canonical = resolved.canonicalize().map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            "mission-reference-missing"
        } else {
            "mission-reference-unknown"
        };
        reference_diag(
            code,
            &resolved,
            reference,
            "The referenced artifact must exist and be accessible.",
        )
    })?;
    if !canonical.starts_with(&root) {
        return Err(reference_diag(
            "mission-reference-escaping",
            &resolved,
            reference,
            "Artifact symlinks must resolve inside the owning workspace.",
        ));
    }
    if !canonical.is_file() {
        return Err(reference_diag(
            "mission-reference-unknown",
            &canonical,
            reference,
            "Artifact references must identify regular files.",
        ));
    }
    let target_relative = canonical.strip_prefix(&root).map_err(|_| {
        reference_diag(
            "mission-reference-wrong-root",
            &canonical,
            reference,
            "Artifact reference resolved outside the owning workspace.",
        )
    })?;
    let parent = consumer.parent().unwrap_or_else(|| Path::new(""));
    let relative = relative_path(parent, target_relative);
    Ok(encode_reference_path(&relative))
}

pub(super) fn validate_canonical_uri(reference: &str) -> bool {
    let Some(rest) = reference.strip_prefix("ce://") else {
        return false;
    };
    let mut parts = rest.split('/');
    let (Some(workspace), Some(store), Some(id), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let known_store = matches!(
        store,
        "ticket"
            | "spec"
            | "test"
            | "session"
            | "mission"
            | "feedback"
            | "log"
            | "rule"
            | "file"
            | "commit"
    );
    known_store && valid_uri_segment(workspace) && valid_uri_segment(id)
}

fn valid_uri_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

fn safe_manifest_path(value: &str) -> bool {
    safe_relative_path(Path::new(value))
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn relative_to_root(root: &Path, path: &Path) -> Option<std::path::PathBuf> {
    if path.is_absolute() {
        path.strip_prefix(root).ok().map(Path::to_path_buf)
    } else {
        Some(path.to_path_buf())
    }
}

fn ensure_existing_ancestors_inside(root: &Path, relative: &Path) -> Result<(), &'static str> {
    let mut current = root.join(relative);
    while !current.exists() {
        if fs::symlink_metadata(&current).is_ok() {
            return Err("mission-reference-wrong-root");
        }
        if !current.pop() {
            return Err("mission-reference-wrong-root");
        }
    }
    let canonical = current
        .canonicalize()
        .map_err(|_| "mission-reference-wrong-root")?;
    if canonical.starts_with(root) {
        Ok(())
    } else {
        Err("mission-reference-wrong-root")
    }
}

fn relative_path(from: &Path, to: &Path) -> std::path::PathBuf {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = std::path::PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for component in &to[common..] {
        result.push(component.as_os_str());
    }
    result
}

fn encode_reference_path(path: &Path) -> String {
    path.to_string_lossy()
        .bytes()
        .fold(String::new(), |mut output, byte| {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
                output.push(char::from(byte));
            } else {
                output.push_str(&format!("%{byte:02X}"));
            }
            output
        })
}

fn known_file_contract(
    path: &str,
    revision: u64,
) -> Option<(MissionFileRole, MissionFileFormat, &'static str, bool)> {
    let fixed = match path {
        "accepted.json" => Some((
            MissionFileRole::Generated,
            MissionFileFormat::Json,
            "mission-accepted/1",
            false,
        )),
        "source.json" => Some((
            MissionFileRole::Authored,
            MissionFileFormat::Json,
            "mission-typed-source/2",
            false,
        )),
        "mission.md" => Some((
            MissionFileRole::Generated,
            MissionFileFormat::Markdown,
            "utf-8-markdown/1",
            false,
        )),
        _ => None,
    };
    if fixed.is_some() {
        return fixed;
    }
    if path == format!("history/revision-{revision}.json") {
        return Some((
            MissionFileRole::Generated,
            MissionFileFormat::Json,
            "mission-accepted/1",
            true,
        ));
    }
    if path == format!("history/source-revision-{revision}.json") {
        return Some((
            MissionFileRole::Authored,
            MissionFileFormat::Json,
            "mission-typed-source/2",
            true,
        ));
    }
    if path == format!("history/provenance-revision-{revision}.json") {
        return Some((
            MissionFileRole::Generated,
            MissionFileFormat::Json,
            "mission-artifact-provenance/1",
            true,
        ));
    }
    if path
        .strip_prefix("generated/")
        .is_some_and(|name| matches!(name, "README.md" | "ROADMAP.md") || valid_part_filename(name))
    {
        return Some((
            MissionFileRole::Generated,
            MissionFileFormat::Markdown,
            "utf-8-markdown/1",
            false,
        ));
    }
    if let Some(name) = path.strip_prefix("parts/")
        && name.strip_suffix(".json").is_some_and(safe_file_stem)
    {
        return Some((
            MissionFileRole::Authored,
            MissionFileFormat::Json,
            "mission-part-source/1",
            false,
        ));
    }
    None
}

fn helper_format_matches(file: &MissionFileProvenance) -> bool {
    match Path::new(&file.path)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some("json") => file.format == MissionFileFormat::Json,
        Some("md") => file.format == MissionFileFormat::Markdown,
        _ => false,
    }
}

fn valid_part_filename(name: &str) -> bool {
    let Some((label, id)) = name.split_once('-') else {
        return false;
    };
    label.len() >= 2
        && label.bytes().all(|byte| byte.is_ascii_digit())
        && id.strip_suffix(".md").is_some_and(safe_file_stem)
}

fn safe_file_stem(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
}

fn file_record(
    path: impl Into<String>,
    role: MissionFileRole,
    format: MissionFileFormat,
    guarantee: &str,
    immutable: bool,
) -> MissionFileProvenance {
    MissionFileProvenance {
        path: path.into(),
        role,
        format,
        schema_guarantees: vec![guarantee.into()],
        immutable,
    }
}

fn artifact_diag(code: &'static str, pointer: &str, message: &str) -> Diagnostic {
    diag(
        code,
        Path::new("artifact-provenance.json"),
        pointer,
        None,
        message,
    )
}

fn reference_diag(code: &'static str, source: &Path, pointer: &str, message: &str) -> Diagnostic {
    diag(code, source, pointer, None, message)
}

fn validation_error(code: &'static str, path: &Path, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![artifact_diag(code, &path.display().to_string(), message)],
    )
}
