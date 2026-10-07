//! Internal mission roadmap bundle, durable store, and deterministic renderer.
//!
//! This crate deliberately has no transport dependencies.  The future public
//! `mission` crate is responsible for loading files and publishing projections.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const SCHEMA_VERSION: u32 = 1;
pub const RENDERER_VERSION: &str = "mission-api/1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub schema_version: u32,
    pub revision: u64,
    pub title: String,
    pub objective: String,
    pub waypoint_files: Vec<String>,
    pub execution_order: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub validation_gates: Vec<ValidationGate>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub deferred: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationGate {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaypointFragment {
    #[serde(alias = "id")]
    pub waypoint_id: String,
    pub title: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub mode: WaypointMode,
    pub scope: Scope,
    #[serde(default)]
    pub state: Option<WaypointState>,
    #[serde(default)]
    pub ticket_ref: Option<String>,
    pub session_package: String,
    pub prompt: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub requirement_ids: Vec<String>,
    #[serde(default)]
    pub non_goals: Vec<String>,
    #[serde(default)]
    pub validation: Vec<ValidationGate>,
    pub commit_checkpoint: CommitCheckpoint,
    pub part_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaypointMode {
    ExecutionSideEffect,
    Research,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Scope {
    SingleSession,
    TicketBacked { ticket_ref: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WaypointState {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitCheckpoint {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionBundle {
    pub manifest: BundleManifest,
    pub waypoints: Vec<WaypointFragment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub source: String,
    pub pointer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waypoint_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum MissionError {
    #[error("bundle validation failed ({0} diagnostic(s))")]
    Validation(usize, Vec<Diagnostic>),
    #[error("stale revision: expected current revision {expected}, found {actual}")]
    StaleRevision { expected: u64, actual: u64 },
    #[error("mission {0} was not found")]
    NotFound(Uuid),
    #[error("publication recovery is required for mission {mission_id}: {message}")]
    PublicationRecovery { mission_id: Uuid, message: String },
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl MissionError {
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Validation(_, diagnostics) => Some(diagnostics),
            _ => None,
        }
    }
}

/// Collect a manifest and exactly its declared, safe, manifest-relative fragments.
pub fn collect_bundle(manifest_path: &Path) -> Result<MissionBundle, MissionError> {
    let manifest_bytes = fs::read(manifest_path)?;
    let manifest: BundleManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        MissionError::Validation(
            1,
            vec![diag(
                "invalid-manifest-json",
                manifest_path,
                "",
                None,
                &format!("Manifest must be valid strict JSON: {error}"),
            )],
        )
    })?;
    let root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let mut diagnostics = validate_manifest_shape(&manifest, manifest_path);
    let mut waypoints = Vec::new();
    let mut seen_paths = BTreeSet::new();
    for (index, fragment) in manifest.waypoint_files.iter().enumerate() {
        let pointer = format!("/waypoint_files/{index}");
        if !seen_paths.insert(fragment) {
            diagnostics.push(diag(
                "duplicate-fragment-path",
                manifest_path,
                &pointer,
                None,
                "Each waypoint fragment path must be listed once.",
            ));
            continue;
        }
        let Some(path) = safe_fragment_path(&root, fragment) else {
            diagnostics.push(diag("unsafe-fragment-path", manifest_path, &pointer, None, "Use a relative path inside the bundle root; absolute, traversal, and symlink escapes are forbidden."));
            continue;
        };
        match fs::read(&path)
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(std::io::Error::other))
        {
            Ok(waypoint) => waypoints.push(waypoint),
            Err(error) => diagnostics.push(diag(
                "fragment-read-failed",
                &path,
                "",
                None,
                &format!("Waypoint fragment cannot be read as strict JSON: {error}"),
            )),
        }
    }
    if diagnostics.is_empty() {
        let bundle = MissionBundle {
            manifest,
            waypoints,
        };
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        Ok(bundle)
    } else {
        Err(MissionError::Validation(diagnostics.len(), diagnostics))
    }
}

pub fn validate_bundle(bundle: &MissionBundle) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = validate_manifest_shape(&bundle.manifest, Path::new("manifest.json"));
    let mut ids = BTreeSet::new();
    let mut waypoint_by_id = BTreeMap::new();
    for (index, waypoint) in bundle.waypoints.iter().enumerate() {
        let source = format!("waypoint[{index}]");
        if !valid_stable_id(&waypoint.waypoint_id) {
            diagnostics.push(diag(
                "invalid-waypoint-id",
                Path::new(&source),
                "/waypoint_id",
                Some(&waypoint.waypoint_id),
                "waypoint_id must be a lowercase slug containing only letters, digits, and hyphens.",
            ));
        } else if !ids.insert(&waypoint.waypoint_id) {
            diagnostics.push(diag(
                "duplicate-waypoint-id",
                Path::new(&source),
                "/waypoint_id",
                Some(&waypoint.waypoint_id),
                "Every waypoint_id must be unique.",
            ));
        }
        if waypoint.title.trim().is_empty()
            || waypoint.session_package.trim().is_empty()
            || waypoint.prompt.trim().is_empty()
            || waypoint.validation.is_empty()
            || waypoint
                .validation
                .iter()
                .any(|gate| gate.command.trim().is_empty())
            || waypoint.commit_checkpoint.message.trim().is_empty()
            || waypoint.part_markdown.trim().is_empty()
        {
            diagnostics.push(diag(
                "missing-required-content",
                Path::new(&source),
                "/prompt",
                Some(&waypoint.waypoint_id),
                "Waypoints require a title, session package, prompt, validation command, commit checkpoint, and Part content.",
            ));
        }
        let ticket_backed =
            matches!(waypoint.scope, Scope::TicketBacked { .. }) || waypoint.ticket_ref.is_some();
        if ticket_backed && waypoint.state.is_some() {
            diagnostics.push(diag(
                "ticket-state-owned-externally",
                Path::new(&source),
                "/state",
                Some(&waypoint.waypoint_id),
                "Ticket-backed waypoints must not carry local mutable state.",
            ));
        }
        if let Some(ticket_ref) = &waypoint.ticket_ref
            && !canonical_reference(ticket_ref, "ticket")
        {
            diagnostics.push(diag(
                "invalid-canonical-reference",
                Path::new(&source),
                "/ticket_ref",
                Some(&waypoint.waypoint_id),
                "Ticket references must use ce://<workspace>/ticket/<uuid>.",
            ));
        }
        if let Scope::TicketBacked { ticket_ref } = &waypoint.scope
            && !canonical_reference(ticket_ref, "ticket")
        {
            diagnostics.push(diag(
                "invalid-canonical-reference",
                Path::new(&source),
                "/scope/ticket_ref",
                Some(&waypoint.waypoint_id),
                "Ticket references must use ce://<workspace>/ticket/<uuid>.",
            ));
        }
        waypoint_by_id.insert(waypoint.waypoint_id.as_str(), waypoint);
        let mut dependencies = BTreeSet::new();
        for (dependency_index, dependency) in waypoint.depends_on.iter().enumerate() {
            if !dependencies.insert(dependency) {
                diagnostics.push(diag(
                    "duplicate-prerequisite",
                    Path::new(&source),
                    &format!("/depends_on/{dependency_index}"),
                    Some(&waypoint.waypoint_id),
                    "A waypoint may declare each prerequisite only once.",
                ));
            }
        }
    }

    let mut requirement_ids = BTreeSet::new();
    for (index, requirement) in bundle.manifest.requirements.iter().enumerate() {
        if !valid_stable_id(&requirement.id) {
            diagnostics.push(diag(
                "invalid-requirement-id",
                Path::new("manifest.json"),
                &format!("/requirements/{index}/id"),
                None,
                "Requirement IDs must be lowercase slugs containing only letters, digits, and hyphens.",
            ));
        } else if !requirement_ids.insert(&requirement.id) {
            diagnostics.push(diag(
                "duplicate-requirement-id",
                Path::new("manifest.json"),
                &format!("/requirements/{index}/id"),
                None,
                "Requirement IDs must be unique.",
            ));
        }
        if requirement.text.trim().is_empty() {
            diagnostics.push(diag(
                "missing-required-content",
                Path::new("manifest.json"),
                &format!("/requirements/{index}/text"),
                None,
                "Requirement text must not be empty.",
            ));
        }
    }
    let order = &bundle.manifest.execution_order;
    if order.len() != waypoint_by_id.len()
        || order.iter().collect::<BTreeSet<_>>().len() != order.len()
        || order
            .iter()
            .any(|id| !waypoint_by_id.contains_key(id.as_str()))
    {
        diagnostics.push(diag(
            "order-requires-each-waypoint-once",
            Path::new("manifest.json"),
            "/execution_order",
            None,
            "execution_order must contain every known waypoint_id exactly once.",
        ));
    }
    let positions: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for waypoint in &bundle.waypoints {
        for prerequisite in &waypoint.depends_on {
            match (
                positions.get(prerequisite.as_str()),
                positions.get(waypoint.waypoint_id.as_str()),
            ) {
                (Some(before), Some(after)) if before < after => {}
                (Some(_), Some(_)) => diagnostics.push(diag(
                    "prerequisite-must-precede-dependent",
                    Path::new("manifest.json"),
                    "/execution_order",
                    Some(&waypoint.waypoint_id),
                    "Every prerequisite must occur before its dependent in execution_order.",
                )),
                _ => diagnostics.push(diag(
                    "unknown-prerequisite",
                    Path::new("manifest.json"),
                    "/execution_order",
                    Some(&waypoint.waypoint_id),
                    "Every prerequisite must name a waypoint in this bundle.",
                )),
            }
        }
    }
    for (index, id) in order.iter().enumerate().skip(1) {
        if let Some(waypoint) = waypoint_by_id.get(id.as_str()) {
            let predecessor = &order[index - 1];
            if !waypoint
                .depends_on
                .iter()
                .any(|dependency| dependency == predecessor)
            {
                diagnostics.push(diag("missing-immediate-predecessor", Path::new("manifest.json"), "/execution_order", Some(id), "Each waypoint after the first must explicitly depend on its immediate predecessor checkpoint."));
            }
        }
    }
    let requirements: BTreeSet<&str> = bundle
        .manifest
        .requirements
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    for waypoint in &bundle.waypoints {
        for id in &waypoint.requirement_ids {
            if !requirements.contains(id.as_str()) {
                diagnostics.push(diag(
                    "unknown-requirement",
                    Path::new("manifest.json"),
                    "/requirements",
                    Some(&waypoint.waypoint_id),
                    "Waypoint requirement_ids must refer to a manifest requirement.",
                ));
            }
        }
    }
    for requirement in &bundle.manifest.requirements {
        if !requirement.deferred
            && !bundle
                .waypoints
                .iter()
                .any(|w| w.requirement_ids.contains(&requirement.id))
        {
            diagnostics.push(diag(
                "unowned-requirement",
                Path::new("manifest.json"),
                "/requirements",
                None,
                "Each non-deferred requirement must be owned by a waypoint.",
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

fn validate_manifest_shape(manifest: &BundleManifest, path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if manifest.schema_version != SCHEMA_VERSION {
        diagnostics.push(diag(
            "unsupported-schema-version",
            path,
            "/schema_version",
            None,
            "Only schema_version 1 is supported.",
        ));
    }
    if manifest.revision == 0
        || manifest.title.trim().is_empty()
        || manifest.objective.trim().is_empty()
    {
        diagnostics.push(diag(
            "missing-required-content",
            path,
            "",
            None,
            "revision, title, and objective must be nonempty valid values.",
        ));
    }
    diagnostics
}

fn valid_stable_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

fn safe_fragment_path(root: &Path, value: &str) -> Option<PathBuf> {
    let candidate = Path::new(value);
    if candidate.is_absolute()
        || candidate.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let canonical = root.join(candidate).canonicalize().ok()?;
    canonical.starts_with(root).then_some(canonical)
}

fn canonical_reference(value: &str, store: &str) -> bool {
    let Some(path) = value.strip_prefix("ce://") else {
        return false;
    };
    let mut segments = path.split('/');
    let (Some(workspace), Some(actual_store), Some(id), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return false;
    };
    !workspace.is_empty()
        && !workspace
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == b'\\')
        && actual_store == store
        && Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id)
}

fn diag(
    code: &'static str,
    source: &Path,
    pointer: &str,
    waypoint_id: Option<&str>,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        code,
        source: source.display().to_string(),
        pointer: pointer.to_string(),
        waypoint_id: waypoint_id.map(str::to_owned),
        message: message.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelMapping {
    pub revision: u64,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedMission {
    pub mission_id: Uuid,
    pub bundle: MissionBundle,
    pub label_mapping: LabelMapping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedRoadmap {
    pub roadmap: Vec<u8>,
    pub parts: BTreeMap<String, Vec<u8>>,
}

/// Render byte-stable Markdown without reading or mutating a mission store.
pub fn render(
    bundle: &MissionBundle,
    mission_id: Uuid,
) -> Result<RenderedRoadmap, Vec<Diagnostic>> {
    validate_bundle(bundle)?;
    let mut roadmap = format!(
        "# {}\n\n<!-- mission: {mission_id}; revision: {}; renderer: {RENDERER_VERSION} -->\n\n## Objective\n\n{}\n\n",
        escape(&bundle.manifest.title),
        bundle.manifest.revision,
        bundle.manifest.objective
    );
    render_string_section(&mut roadmap, "Artifacts", &bundle.manifest.artifacts);
    if !bundle.manifest.requirements.is_empty() {
        roadmap.push_str("## Requirements\n\n| ID | Requirement | Deferred |\n|---|---|---|\n");
        for requirement in &bundle.manifest.requirements {
            roadmap.push_str(&format!(
                "| `{}` | {} | {} |\n",
                escape(&requirement.id),
                escape(&requirement.text),
                requirement.deferred
            ));
        }
        roadmap.push('\n');
    }
    render_gate_section(
        &mut roadmap,
        "Validation Gates",
        &bundle.manifest.validation_gates,
    );
    render_string_section(&mut roadmap, "Notes", &bundle.manifest.notes);
    roadmap.push_str("## Roadmap Waypoints\n\n");
    let mut parts = BTreeMap::new();
    for (index, id) in bundle.manifest.execution_order.iter().enumerate() {
        let waypoint = bundle
            .waypoints
            .iter()
            .find(|waypoint| waypoint.waypoint_id == *id)
            .expect("validated order contains waypoint");
        let label = format!("W{}", index + 1);
        let filename = format!("{:02}-{}.md", index + 1, waypoint.waypoint_id);
        let dependencies = if waypoint.depends_on.is_empty() {
            "None".to_string()
        } else {
            waypoint.depends_on.join(", ")
        };
        let scope = match &waypoint.scope {
            Scope::SingleSession => "Single session".to_string(),
            Scope::TicketBacked { ticket_ref } => format!("Ticket-backed: `{ticket_ref}`"),
        };
        let state = waypoint.state.as_ref().map_or_else(
            || "Owned externally".to_string(),
            |state| match state {
                WaypointState::Pending => "pending".to_string(),
                WaypointState::InProgress => "in-progress".to_string(),
                WaypointState::Completed => "completed".to_string(),
                WaypointState::Blocked => "blocked".to_string(),
            },
        );
        roadmap.push_str(&format!(
            "### {label}. {} (`{}`)\n\n| Field | Value |\n|---|---|\n| **Part** | [{}]({}) |\n| **Mode** | {:?} |\n| **Scope** | {} |\n| **State** | {} |\n| **Prerequisites** | {} |\n| **Session package** | {} |\n| **Prompt** | {} |\n| **Artifacts** | {} |\n| **Requirements** | {} |\n| **Non-goals** | {} |\n| **Commit checkpoint** | {} |\n\n",
            escape(&waypoint.title),
            waypoint.waypoint_id,
            escape(&filename),
            filename,
            mode_label(&waypoint.mode),
            escape(&scope),
            escape(&state),
            escape(&dependencies),
            escape(&waypoint.session_package),
            escape(&waypoint.prompt),
            escape(&waypoint.artifacts.join(", ")),
            escape(&waypoint.requirement_ids.join(", ")),
            escape(&waypoint.non_goals.join(", ")),
            escape(&waypoint.commit_checkpoint.message)
        ));
        render_gate_section(
            &mut roadmap,
            &format!("{label} Validation"),
            &waypoint.validation,
        );
        let previous = index.checked_sub(1).map(|previous| {
            format!(
                "{:02}-{}.md",
                previous + 1,
                bundle.manifest.execution_order[previous]
            )
        });
        let next = bundle
            .manifest
            .execution_order
            .get(index + 1)
            .map(|next| format!("{:02}-{}.md", index + 2, next));
        let navigation = match (previous, next) {
            (Some(previous), Some(next)) => {
                format!("Previous: [{previous}]({previous}) | Next: [{next}]({next})")
            }
            (Some(previous), None) => format!("Previous: [{previous}]({previous})"),
            (None, Some(next)) => format!("Next: [{next}]({next})"),
            (None, None) => String::new(),
        };
        let mut part = format!(
            "# {label}. {} (`{}`)\n\n<!-- mission: {mission_id}; revision: {}; renderer: {RENDERER_VERSION} -->\n\n{navigation}\n\n",
            escape(&waypoint.title),
            waypoint.waypoint_id,
            bundle.manifest.revision,
        );
        part.push_str(&format!(
            "## Waypoint Details\n\n| Field | Value |\n|---|---|\n| **Mode** | {:?} |\n| **Scope** | {} |\n| **State** | {} |\n| **Prerequisites** | {} |\n| **Session package** | {} |\n| **Prompt** | {} |\n| **Artifacts** | {} |\n| **Requirements** | {} |\n| **Non-goals** | {} |\n| **Commit checkpoint** | {} |\n\n",
            mode_label(&waypoint.mode),
            escape(&scope),
            escape(&state),
            escape(&dependencies),
            escape(&waypoint.session_package),
            escape(&waypoint.prompt),
            escape(&waypoint.artifacts.join(", ")),
            escape(&waypoint.requirement_ids.join(", ")),
            escape(&waypoint.non_goals.join(", ")),
            escape(&waypoint.commit_checkpoint.message)
        ));
        part.push_str("## Validation\n\n");
        append_gates(&mut part, &waypoint.validation);
        part.push_str("\n## Part\n\n");
        part.push_str(&waypoint.part_markdown);
        if !part.ends_with('\n') {
            part.push('\n');
        }
        parts.insert(filename, part.into_bytes());
    }
    Ok(RenderedRoadmap {
        roadmap: roadmap.into_bytes(),
        parts,
    })
}

fn render_string_section(output: &mut String, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    output.push_str(&format!("## {title}\n\n"));
    for value in values {
        output.push_str(&format!("- {}\n", escape(value)));
    }
    output.push('\n');
}

fn render_gate_section(output: &mut String, title: &str, gates: &[ValidationGate]) {
    if gates.is_empty() {
        return;
    }
    output.push_str(&format!("## {title}\n\n"));
    append_gates(output, gates);
    output.push('\n');
}

fn append_gates(output: &mut String, gates: &[ValidationGate]) {
    for gate in gates {
        let cwd = gate
            .cwd
            .as_deref()
            .map_or_else(String::new, |cwd| format!(" (cwd: `{}`)", escape(cwd)));
        output.push_str(&format!("- `{}`{cwd}\n", escape(&gate.command)));
    }
}

fn mode_label(mode: &WaypointMode) -> &'static str {
    match mode {
        WaypointMode::ExecutionSideEffect => "execution-side-effect",
        WaypointMode::Research => "research",
    }
}

fn escape(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', "<br>")
}

pub struct MissionStore {
    root: PathBuf,
}

impl MissionStore {
    fn at_workspace(workspace_root: &Path) -> Self {
        let root = workspace_root
            .join(".workflow-tools")
            .join("mission")
            .join("missions");
        Self { root }
    }

    pub fn init(workspace_root: &Path) -> Result<Self, MissionError> {
        let store = Self::at_workspace(workspace_root);
        fs::create_dir_all(&store.root)?;
        Ok(store)
    }

    /// Open a store without creating directories or changing filesystem state.
    pub fn open(workspace_root: &Path) -> Self {
        Self::at_workspace(workspace_root)
    }

    pub fn import(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
    ) -> Result<AcceptedMission, MissionError> {
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let mission_dir = self.root.join(mission_id.to_string());
        let current = match self.get(mission_id) {
            Ok(current) => Some(current),
            Err(MissionError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let actual = current
            .as_ref()
            .map_or(0, |mission| mission.bundle.manifest.revision);
        if expected_current_revision != actual {
            return Err(MissionError::StaleRevision {
                expected: expected_current_revision,
                actual,
            });
        }
        if let Some(existing) = &current
            && bundle.manifest.revision <= existing.bundle.manifest.revision
        {
            return Err(MissionError::StaleRevision {
                expected: existing.bundle.manifest.revision + 1,
                actual: bundle.manifest.revision,
            });
        }
        let labels = bundle
            .manifest
            .execution_order
            .iter()
            .enumerate()
            .map(|(index, id)| (format!("W{}", index + 1), id.clone()))
            .collect();
        let accepted = AcceptedMission {
            mission_id,
            label_mapping: LabelMapping {
                revision: bundle.manifest.revision,
                labels,
            },
            bundle,
        };
        let temp = self.root.join(format!(".{}.tmp", mission_id));
        if temp.exists() {
            fs::remove_dir_all(&temp)?;
        }
        fs::create_dir_all(temp.join("history"))?;
        let bytes = serde_json::to_vec_pretty(&accepted)?;
        fs::write(temp.join("accepted.json"), &bytes)?;
        fs::write(
            temp.join("history").join(format!(
                "revision-{}.json",
                accepted.bundle.manifest.revision
            )),
            &bytes,
        )?;
        if let Some(existing) = current {
            let history = mission_dir.join("history");
            fs::create_dir_all(&history)?;
            let old = serde_json::to_vec_pretty(&existing)?;
            fs::write(
                history.join(format!(
                    "revision-{}.json",
                    existing.bundle.manifest.revision
                )),
                old,
            )?;
        }
        fs::create_dir_all(mission_dir.join("history"))?;
        for entry in fs::read_dir(temp.join("history"))? {
            let entry = entry?;
            fs::rename(
                entry.path(),
                mission_dir.join("history").join(entry.file_name()),
            )?;
        }
        let accepted_path = mission_dir.join("accepted.json");
        let backup_path = mission_dir.join(".accepted.backup");
        if accepted_path.exists() {
            if backup_path.exists() {
                fs::remove_file(&backup_path)?;
            }
            fs::rename(&accepted_path, &backup_path)?;
        }
        if let Err(error) = fs::rename(temp.join("accepted.json"), &accepted_path) {
            if backup_path.exists() {
                fs::rename(&backup_path, &accepted_path)?;
            }
            return Err(error.into());
        }
        if backup_path.exists() {
            fs::remove_file(backup_path)?;
        }
        fs::remove_dir_all(temp)?;
        self.get(mission_id)
    }

    pub fn get(&self, mission_id: Uuid) -> Result<AcceptedMission, MissionError> {
        let path = self.root.join(mission_id.to_string()).join("accepted.json");
        if !path.is_file() {
            return Err(MissionError::NotFound(mission_id));
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn revision_mapping(
        &self,
        mission_id: Uuid,
        revision: u64,
    ) -> Result<LabelMapping, MissionError> {
        let path = self
            .root
            .join(mission_id.to_string())
            .join("history")
            .join(format!("revision-{revision}.json"));
        if !path.is_file() {
            return Err(MissionError::NotFound(mission_id));
        }
        Ok(serde_json::from_slice::<AcceptedMission>(&fs::read(path)?)?.label_mapping)
    }

    /// Atomically replaces the accepted record and its generated projection.
    ///
    /// The entire mission directory is staged before its current directory is
    /// moved aside.  A failed replacement restores that directory; failure to
    /// restore is reported explicitly rather than silently selecting either
    /// version.
    pub fn publish(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
    ) -> Result<AcceptedMission, MissionError> {
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let current = match self.get(mission_id) {
            Ok(current) => Some(current),
            Err(MissionError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        let actual = current
            .as_ref()
            .map_or(0, |mission| mission.bundle.manifest.revision);
        if expected_current_revision != actual {
            return Err(MissionError::StaleRevision {
                expected: expected_current_revision,
                actual,
            });
        }
        if let Some(existing) = &current
            && bundle.manifest.revision <= existing.bundle.manifest.revision
        {
            return Err(MissionError::StaleRevision {
                expected: existing.bundle.manifest.revision + 1,
                actual: bundle.manifest.revision,
            });
        }
        let rendered = render(&bundle, mission_id)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let accepted = AcceptedMission {
            mission_id,
            label_mapping: LabelMapping {
                revision: bundle.manifest.revision,
                labels: bundle
                    .manifest
                    .execution_order
                    .iter()
                    .enumerate()
                    .map(|(index, id)| (format!("W{}", index + 1), id.clone()))
                    .collect(),
            },
            bundle,
        };
        let mission_dir = self.root.join(mission_id.to_string());
        let stage = self.root.join(format!(".{mission_id}.publish.tmp"));
        let backup = self.root.join(format!(".{mission_id}.publish.backup"));
        if stage.exists() || backup.exists() {
            return Err(MissionError::PublicationRecovery {
                mission_id,
                message: "a previous publication staging or backup directory exists".into(),
            });
        }
        if mission_dir.exists() {
            copy_directory(&mission_dir, &stage)?;
        } else {
            fs::create_dir_all(&stage)?;
        }
        let write_result = (|| -> Result<(), MissionError> {
            fs::create_dir_all(stage.join("history"))?;
            let bytes = serde_json::to_vec_pretty(&accepted)?;
            fs::write(stage.join("accepted.json"), &bytes)?;
            fs::write(
                stage.join("history").join(format!(
                    "revision-{}.json",
                    accepted.bundle.manifest.revision
                )),
                &bytes,
            )?;
            let generated = stage.join("generated");
            if generated.exists() {
                fs::remove_dir_all(&generated)?;
            }
            fs::create_dir_all(&generated)?;
            fs::write(generated.join("ROADMAP.md"), &rendered.roadmap)?;
            for (name, bytes) in &rendered.parts {
                fs::write(generated.join(name), bytes)?;
            }
            Ok(())
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_dir_all(&stage);
            return Err(error);
        }
        if mission_dir.exists() {
            fs::rename(&mission_dir, &backup)?;
        }
        if let Err(error) = fs::rename(&stage, &mission_dir) {
            let recovery = if backup.exists() {
                fs::rename(&backup, &mission_dir)
            } else {
                Ok(())
            };
            return match recovery {
                Ok(()) => Err(error.into()),
                Err(recovery_error) => Err(MissionError::PublicationRecovery {
                    mission_id,
                    message: format!(
                        "replacement failed ({error}); restoring the prior mission also failed ({recovery_error})"
                    ),
                }),
            };
        }
        if backup.exists() {
            fs::remove_dir_all(backup)?;
        }
        self.get(mission_id)
    }
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), MissionError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waypoint(id: &str, depends_on: Vec<String>) -> WaypointFragment {
        WaypointFragment {
            waypoint_id: id.into(),
            title: id.into(),
            depends_on,
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            session_package: id.into(),
            prompt: "implement".into(),
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: vec![],
            validation: vec![ValidationGate {
                command: "cargo test".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "checkpoint".into(),
            },
            part_markdown: "## Outcome".into(),
        }
    }
    fn roadmap(revision: u64) -> MissionBundle {
        MissionBundle {
            manifest: BundleManifest {
                schema_version: SCHEMA_VERSION,
                revision,
                title: "Mission".into(),
                objective: "Objective".into(),
                waypoint_files: vec![],
                execution_order: vec!["first".into(), "second".into()],
                artifacts: vec![],
                requirements: vec![],
                validation_gates: vec![],
                notes: vec![],
            },
            waypoints: vec![
                waypoint("first", vec![]),
                waypoint("second", vec!["first".into()]),
            ],
        }
    }
    #[test]
    fn store_readback_preserves_manifest_and_history_mapping() {
        let directory = tempfile::tempdir().unwrap();
        let store = MissionStore::init(directory.path()).unwrap();
        let mission_id = Uuid::new_v4();
        store.import(mission_id, roadmap(1), 0).unwrap();
        let mut second = roadmap(2);
        second.manifest.execution_order = vec!["second".into(), "first".into()];
        second.waypoints[0].depends_on = vec!["second".into()];
        second.waypoints[1].depends_on = vec![];
        store.import(mission_id, second, 1).unwrap();
        assert_eq!(store.get(mission_id).unwrap().bundle.manifest.revision, 2);
        assert_eq!(
            store.revision_mapping(mission_id, 1).unwrap().labels["W1"],
            "first"
        );
    }
    #[test]
    fn roadmap_render_is_pure_and_returns_generated_bytes() {
        let roadmap = roadmap(1);
        assert_eq!(
            render(&roadmap, Uuid::nil()).unwrap().roadmap,
            render(&roadmap, Uuid::nil()).unwrap().roadmap
        );
    }

    #[test]
    fn import_does_not_replace_a_corrupt_current_record() {
        let directory = tempfile::tempdir().unwrap();
        let store = MissionStore::init(directory.path()).unwrap();
        let mission_id = Uuid::new_v4();
        store.import(mission_id, roadmap(1), 0).unwrap();
        let accepted_path = store
            .root
            .join(mission_id.to_string())
            .join("accepted.json");
        fs::write(&accepted_path, "{").unwrap();

        assert!(matches!(
            store.import(mission_id, roadmap(2), 1),
            Err(MissionError::Json(_))
        ));
        assert_eq!(fs::read_to_string(accepted_path).unwrap(), "{");
    }
}
