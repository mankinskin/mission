use super::*;

pub(super) fn validate_json_output(
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
            if !super::references::safe_manifest_path(&file.path) {
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

    pub(crate) fn for_write(
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
    if path.strip_prefix("generated/").is_some_and(|name| {
        matches!(
            name,
            "README.md" | "ROADMAP.md" | "HISTORY.md" | "waypoints/README.md"
        ) || name
            .strip_prefix("waypoints/")
            .is_some_and(valid_part_filename)
            || valid_part_filename(name)
    }) {
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
