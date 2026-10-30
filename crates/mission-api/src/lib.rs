//! Internal mission roadmap bundle, durable store, and deterministic renderer.
//!
//! This crate deliberately has no transport dependencies.  The future public
//! `mission` crate is responsible for loading files and publishing projections.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod operator_results;
mod outcome_plan;
pub use outcome_plan::*;
mod output_history;
pub use output_history::*;
mod artifact_contract;
pub mod lifecycle;
mod plan_operations;
mod repair;
mod review_entry;
mod schema_renderers;
mod typed_migration;
mod typed_source;
mod waypoint_relationships;
pub use artifact_contract::{
    ARTIFACT_PROVENANCE_SCHEMA_VERSION, MissionArtifactProvenance, MissionFileFormat,
    MissionFileProvenance, MissionFileRole, resolve_artifact_reference,
};
pub use lifecycle::*;
pub use plan_operations::{
    PlanOperation, PlanOperationOutcome, PlanOperationRequest, PlanRequirement, ReviewEvidence,
    ReviewPlan, ReviewWaypoint,
};
pub use repair::*;
pub use schema_renderers::*;
pub use typed_migration::*;
pub use typed_source::*;
pub use waypoint_relationships::*;

/// Legacy native authoring version; v2 requires explicit authored outcome plans.
pub const SCHEMA_VERSION: u32 = 1;
/// Legacy renderer retained byte-for-byte; native v2 uses mission-api/6.
pub const RENDERER_VERSION: &str = "mission-api/5";

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
    /// Operator-supplied evidence. Recording this data never executes it.
    #[serde(default)]
    pub operator_results: Vec<OperatorResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorResult {
    pub waypoint_id: String,
    pub outcome: String,
    pub operator_command: String,
    pub evidence_pointer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub deferred: bool,
    /// References only; specification criteria remain authoritative in Spec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub canonical_criteria: Vec<CriterionReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ValidationGate {
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaypointFragment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_plan: Option<WaypointOutcomePlan>,
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
    /// Canonical reference to an accepted child mission.  This is deliberately
    /// separate from ticket ownership and from the execution scope.
    #[serde(default)]
    pub child_mission_ref: Option<String>,
    #[serde(default, skip_serializing_if = "WaypointRelationships::is_empty")]
    pub relationships: WaypointRelationships,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommitCheckpoint {
    pub message: String,
}

/// Complete local waypoint content supplied by the transport facade.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectWaypoint {
    pub waypoint_id: String,
    pub title: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub prompt: String,
    pub validation: Vec<ValidationGate>,
    pub commit_checkpoint: CommitCheckpoint,
    #[serde(default)]
    pub non_goals: Vec<String>,
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
    diagnostics.extend(schema_renderers::validate_requirement_shapes(
        &bundle.manifest.requirements,
    ));
    let mut ids = BTreeSet::new();
    let mut waypoint_by_id = BTreeMap::new();
    for (index, waypoint) in bundle.waypoints.iter().enumerate() {
        if let Err(diagnostic) = outcome_plan::validate_outcome_plan(
            waypoint.outcome_plan.as_ref(),
            bundle.manifest.schema_version == OUTCOME_BUNDLE_SCHEMA_VERSION,
            &waypoint.waypoint_id,
        ) {
            diagnostics.push(diagnostic);
        }
        if let Err(diagnostic) = waypoint.relationships.validate() {
            diagnostics.push(diagnostic);
        }
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
        {
            diagnostics.push(diag(
                "missing-required-content",
                Path::new(&source),
                "/prompt",
                Some(&waypoint.waypoint_id),
                "Waypoints require a title, session package, prompt, validation command, and commit checkpoint.",
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
        if let (
            Some(ticket_ref),
            Scope::TicketBacked {
                ticket_ref: scoped_ref,
            },
        ) = (&waypoint.ticket_ref, &waypoint.scope)
            && ticket_ref != scoped_ref
        {
            diagnostics.push(diag(
                "ticket-reference-mismatch",
                Path::new(&source),
                "/ticket_ref",
                Some(&waypoint.waypoint_id),
                "ticket_ref must match the reference declared by ticket-backed scope.",
            ));
        }
        if let Some(child_mission_ref) = &waypoint.child_mission_ref {
            if !canonical_reference(child_mission_ref, "mission") {
                diagnostics.push(diag(
                    "invalid-child-mission-reference",
                    Path::new(&source),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child mission references must use ce://<workspace>/mission/<uuid>.",
                ));
            }
            if waypoint.state.is_some() {
                diagnostics.push(diag(
                    "child-mission-state-derived",
                    Path::new(&source),
                    "/state",
                    Some(&waypoint.waypoint_id),
                    "Child mission waypoint state is derived from the accepted child mission.",
                ));
            }
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

    diagnostics.extend(operator_results::validate(bundle));
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
    if order.is_empty() {
        diagnostics.push(diag(
            "empty-mission",
            Path::new("manifest.json"),
            "/execution_order",
            None,
            "An accepted mission must contain at least one waypoint.",
        ));
    }
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
    if !matches!(
        manifest.schema_version,
        SCHEMA_VERSION | OUTCOME_BUNDLE_SCHEMA_VERSION
    ) {
        diagnostics.push(diag(
            "unsupported-schema-version",
            path,
            "/schema_version",
            None,
            "Only native schema_version 1 (legacy) and 2 (planned outcomes) are supported.",
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

fn mission_reference(value: &str) -> Option<(&str, Uuid)> {
    let path = value.strip_prefix("ce://")?;
    let mut segments = path.split('/');
    let (workspace, store, id, None) = (
        segments.next()?,
        segments.next()?,
        segments.next()?,
        segments.next(),
    ) else {
        return None;
    };
    (store == "mission").then_some((workspace, Uuid::parse_str(id).ok()?))
}

fn mission_reference_id(value: &str) -> Option<Uuid> {
    mission_reference(value).map(|(_, id)| id)
}

fn first_incomplete_state(bundle: &MissionBundle) -> WaypointState {
    bundle
        .manifest
        .execution_order
        .iter()
        .find_map(|id| {
            bundle
                .waypoints
                .iter()
                .find(|waypoint| waypoint.waypoint_id == *id)
                .map(|waypoint| waypoint.state.clone().unwrap_or(WaypointState::Pending))
                .filter(|state| *state != WaypointState::Completed)
        })
        .unwrap_or(WaypointState::Completed)
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedRoadmap {
    pub readme: Vec<u8>,
    pub roadmap: Vec<u8>,
    pub parts: BTreeMap<String, Vec<u8>>,
}

impl RenderedRoadmap {
    /// The complete generated inventory shared by publication and verification.
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        [
            ("README.md", self.readme.as_slice()),
            ("ROADMAP.md", self.roadmap.as_slice()),
        ]
        .into_iter()
        .chain(
            self.parts
                .iter()
                .map(|(name, bytes)| (name.as_str(), bytes.as_slice())),
        )
    }

    /// Compare the complete inventory and review-entry destinations without writes.
    pub fn check_generated(&self, generated: &Path) -> Result<Vec<Diagnostic>, MissionError> {
        review_entry::check_generated(self, generated)
    }
}

/// Render byte-stable Markdown without reading or mutating a mission store.
pub fn render(
    bundle: &MissionBundle,
    mission_id: Uuid,
) -> Result<RenderedRoadmap, Vec<Diagnostic>> {
    render_with_snapshot(bundle, mission_id, None)
}

/// Render a previously materialized status snapshot without reading a store.
pub fn render_with_snapshot(
    bundle: &MissionBundle,
    mission_id: Uuid,
    snapshot_id: Option<&str>,
) -> Result<RenderedRoadmap, Vec<Diagnostic>> {
    render_with_consumer(bundle, mission_id, snapshot_id, None, None)
}

fn render_with_consumer(
    bundle: &MissionBundle,
    mission_id: Uuid,
    snapshot_id: Option<&str>,
    workspace: Option<&Path>,
    generated_directory: Option<&Path>,
) -> Result<RenderedRoadmap, Vec<Diagnostic>> {
    if snapshot_id.is_some() {
        let mut validation_bundle = bundle.clone();
        for waypoint in &mut validation_bundle.waypoints {
            if waypoint.ticket_ref.is_some()
                || waypoint.child_mission_ref.is_some()
                || matches!(waypoint.scope, Scope::TicketBacked { .. })
            {
                waypoint.state = None;
            }
        }
        validate_bundle(&validation_bundle)?;
    } else {
        validate_bundle(bundle)?;
    }
    let relationships = if let Some(workspace) = workspace {
        waypoint_relationships::resolve_bundle(workspace, bundle).map_err(|error| match error {
            MissionError::Validation(_, diagnostics) => diagnostics,
            other => vec![contract_diag(
                "mission-waypoint-link-unavailable",
                "",
                &other.to_string(),
            )],
        })?
    } else {
        if bundle
            .waypoints
            .iter()
            .any(|waypoint| !waypoint.relationships.is_empty())
        {
            return Err(vec![contract_diag(
                "mission-waypoint-link-root-required",
                "/relationships",
                "Use render_in_workspace with an explicit owning workspace to resolve waypoint relationships.",
            )]);
        }
        BTreeMap::new()
    };
    let mut rendered = render_materialized_with_consumer(
        bundle,
        mission_id,
        snapshot_id,
        workspace,
        generated_directory,
    );
    for (index, id) in bundle.manifest.execution_order.iter().enumerate() {
        if let Some(content) = relationships.get(id) {
            rendered
                .roadmap
                .extend_from_slice(format!("\n## Linked entities: `{id}`\n\n{content}").as_bytes());
            let filename = format!("waypoints/{:02}-{id}.md", index + 1);
            if let Some(part) = rendered.parts.get_mut(&filename) {
                part.extend_from_slice(format!("\n## Linked entities\n\n{content}").as_bytes());
            }
        }
    }
    Ok(rendered)
}

fn render_materialized_with_consumer(
    bundle: &MissionBundle,
    mission_id: Uuid,
    snapshot_id: Option<&str>,
    workspace: Option<&Path>,
    generated_directory: Option<&Path>,
) -> RenderedRoadmap {
    let renderer = outcome_plan::renderer_version(bundle);
    let snapshot_id = snapshot_id.filter(|id| *id != "ticket-status-v1:{}");
    let snapshot_provenance = format!(
        "; external-status-snapshot: {}",
        snapshot_id.unwrap_or("ticket-status-v1:{}")
    );
    let mut roadmap = format!(
        "# {}\n\n<!-- mission: {mission_id}; revision: {}; renderer: {renderer}{snapshot_provenance} -->\n\n## Objective\n\n{}\n\n",
        escape(&bundle.manifest.title),
        bundle.manifest.revision,
        schema_renderers::markdown_text(&bundle.manifest.objective)
    );
    render_artifact_section(
        &mut roadmap,
        &bundle.manifest.artifacts,
        workspace,
        generated_directory
            .map(|path| path.join("ROADMAP.md"))
            .as_deref(),
    );
    if !bundle.manifest.requirements.is_empty() {
        roadmap.push_str("## Requirements\n\n| ID | Requirement | Deferred | Canonical criteria |\n|---|---|---|---|\n");
        for requirement in &bundle.manifest.requirements {
            roadmap.push_str(&render_requirement_row(requirement));
        }
        roadmap.push('\n');
    }
    render_gate_section(
        &mut roadmap,
        "Validation Gates",
        &bundle.manifest.validation_gates,
    );
    render_string_section(&mut roadmap, "Notes", &bundle.manifest.notes);
    operator_results::render(&mut roadmap, &bundle.manifest.operator_results, None);
    roadmap.push_str("## Roadmap Waypoints\n\n");
    let mut parts = BTreeMap::new();
    for (index, id) in bundle.manifest.execution_order.iter().enumerate() {
        let waypoint = bundle
            .waypoints
            .iter()
            .find(|waypoint| waypoint.waypoint_id == *id)
            .expect("validated order contains waypoint");
        let label = format!("W{}", index + 1);
        let filename = format!("waypoints/{:02}-{}.md", index + 1, waypoint.waypoint_id);
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
        let child_mission = waypoint.child_mission_ref.as_deref().unwrap_or("None");
        roadmap.push_str(&format!(
            "### {label}. {} (`{}`)\n\n| Field | Value |\n|---|---|\n| **Part** | [{}]({}) |\n| **Mode** | {:?} |\n| **Scope** | {} |\n| **Child mission** | {} |\n| **State** | {} |\n| **Prerequisites** | {} |\n| **Session package** | {} |\n| **Prompt** | {} |\n| **Artifacts** | {} |\n| **Requirements** | {} |\n| **Non-goals** | {} |\n| **Commit checkpoint** | {} |\n\n",
            escape(&waypoint.title),
            waypoint.waypoint_id,
            escape(&filename),
            filename,
            mode_label(&waypoint.mode),
            escape(&scope),
            escape(child_mission),
            escape(&state),
            escape(&dependencies),
            escape(&waypoint.session_package),
            escape(&waypoint.prompt),
            render_artifacts(
                &waypoint.artifacts,
                workspace,
                generated_directory
                    .map(|path| path.join("ROADMAP.md"))
                    .as_deref(),
            ),
            escape(&waypoint.requirement_ids.join(", ")),
            escape(&waypoint.non_goals.join(", ")),
            escape(&waypoint.commit_checkpoint.message)
        ));
        render_gate_section(
            &mut roadmap,
            &format!("{label} Validation"),
            &waypoint.validation,
        );
        outcome_plan::render_outcome_plan(&mut roadmap, waypoint.outcome_plan.as_ref());
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
            "# {label}. {} (`{}`)\n\n<!-- mission: {mission_id}; revision: {}; renderer: {renderer}{snapshot_provenance} -->\n\n{navigation}\n\n",
            escape(&waypoint.title),
            waypoint.waypoint_id,
            bundle.manifest.revision,
        );
        part.push_str(&format!(
            "## Waypoint Details\n\n| Field | Value |\n|---|---|\n| **Mode** | {:?} |\n| **Scope** | {} |\n| **Child mission** | {} |\n| **State** | {} |\n| **Prerequisites** | {} |\n| **Session package** | {} |\n| **Prompt** | {} |\n| **Artifacts** | {} |\n| **Requirements** | {} |\n| **Non-goals** | {} |\n| **Commit checkpoint** | {} |\n\n",
            mode_label(&waypoint.mode),
            escape(&scope),
            escape(child_mission),
            escape(&state),
            escape(&dependencies),
            escape(&waypoint.session_package),
            escape(&waypoint.prompt),
            render_artifacts(
                &waypoint.artifacts,
                workspace,
                generated_directory.map(|path| path.join(&filename)).as_deref(),
            ),
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
        if waypoint.outcome_plan.is_some() {
            part.push('\n');
            outcome_plan::render_outcome_plan(&mut part, waypoint.outcome_plan.as_ref());
        }
        operator_results::render(
            &mut part,
            &bundle.manifest.operator_results,
            Some(&waypoint.waypoint_id),
        );
        parts.insert(filename, part.into_bytes());
    }
    let mut map = String::from("# Current waypoint Parts\n\n");
    for name in parts.keys() {
        let name = name.strip_prefix("waypoints/").unwrap_or(name);
        map.push_str(&format!("- [{name}]({name})\n"));
    }
    parts.insert("waypoints/README.md".into(), map.into_bytes());
    // A pure preview cannot know which archives actually exist.
    parts.insert(
        "HISTORY.md".into(),
        b"# Mission output history\n\nPreview only: archive availability is unknown. Store-backed publication and verification use preserved states; this preview does not establish historical availability.\n".to_vec(),
    );
    RenderedRoadmap {
        readme: review_entry::render(
            bundle,
            mission_id,
            snapshot_id,
            workspace,
            generated_directory
                .map(|path| path.join("README.md"))
                .as_deref(),
        ),
        roadmap: roadmap.into_bytes(),
        parts,
    }
}

fn render_artifact_section(
    output: &mut String,
    values: &[String],
    workspace: Option<&Path>,
    consumer: Option<&Path>,
) {
    if values.is_empty() {
        return;
    }
    output.push_str("## Artifacts\n\n");
    for value in values {
        output.push_str(&format!(
            "- {}\n",
            render_artifacts(std::slice::from_ref(value), workspace, consumer)
        ));
    }
    output.push('\n');
}

fn render_artifacts(
    values: &[String],
    workspace: Option<&Path>,
    consumer: Option<&Path>,
) -> String {
    values
        .iter()
        .map(|reference| review_entry::render_reference(reference, workspace, consumer))
        .collect::<Vec<_>>()
        .join(", ")
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
    schema_renderers::markdown_text(value)
}

pub struct MissionStore {
    root: PathBuf,
    #[cfg(test)]
    test_event_metadata: Option<(Uuid, u64)>,
}

/// File-backed lifecycle boundary used by the shared Mission facade.
pub struct MissionLifecycle {
    store: MissionStore,
}

/// The API-owned identity allocated for a create operation and its outcome.
///
/// A rejected create retains this identity for callers that report the attempted
/// mission, while no accepted record is written.
pub struct CreateMissionResult {
    pub mission_id: Uuid,
    pub result: Result<AcceptedMission, MissionError>,
}

impl MissionLifecycle {
    pub fn open(workspace_root: &Path) -> Self {
        Self {
            store: MissionStore::open(workspace_root),
        }
    }

    pub fn create(
        &self,
        title: String,
        objective: String,
        waypoints: Vec<DirectWaypoint>,
    ) -> Result<AcceptedMission, MissionError> {
        self.create_with_identity(title, objective, waypoints)
            .result
    }

    /// Creates a mission while retaining the API-allocated identity on failure.
    pub fn create_with_identity(
        &self,
        title: String,
        objective: String,
        waypoints: Vec<DirectWaypoint>,
    ) -> CreateMissionResult {
        let mission_id = Uuid::new_v4();
        let bundle = MissionBundle {
            manifest: BundleManifest {
                schema_version: SCHEMA_VERSION,
                revision: 1,
                title,
                objective,
                waypoint_files: vec![],
                execution_order: waypoints
                    .iter()
                    .map(|waypoint| waypoint.waypoint_id.clone())
                    .collect(),
                artifacts: vec![],
                requirements: vec![],
                validation_gates: vec![],
                notes: vec![],
                operator_results: vec![],
            },
            waypoints: waypoints.into_iter().map(Self::fragment).collect(),
        };
        CreateMissionResult {
            mission_id,
            result: self.store.import(mission_id, bundle, 0),
        }
    }

    pub fn edit_objective(
        &self,
        mission_id: Uuid,
        objective: String,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            bundle.manifest.objective = objective;
            Ok(())
        })
    }

    pub fn add_waypoint(
        &self,
        mission_id: Uuid,
        waypoint: DirectWaypoint,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            bundle
                .manifest
                .execution_order
                .push(waypoint.waypoint_id.clone());
            bundle.waypoints.push(Self::fragment(waypoint));
            Ok(())
        })
    }

    pub fn edit_waypoint(
        &self,
        mission_id: Uuid,
        waypoint: DirectWaypoint,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            let item = Self::local_waypoint(bundle, &waypoint.waypoint_id)?;
            item.title = waypoint.title;
            item.depends_on = waypoint.depends_on;
            item.prompt = waypoint.prompt;
            item.validation = waypoint.validation;
            item.commit_checkpoint = waypoint.commit_checkpoint;
            item.non_goals = waypoint.non_goals;
            Ok(())
        })
    }

    pub fn order_waypoints(
        &self,
        mission_id: Uuid,
        execution_order: Vec<String>,
        dependencies: BTreeMap<String, Vec<String>>,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            for (waypoint_id, depends_on) in dependencies {
                Self::local_waypoint(bundle, &waypoint_id)?.depends_on = depends_on;
            }
            bundle.manifest.execution_order = execution_order;
            Ok(())
        })
    }

    pub fn record_progress(
        &self,
        mission_id: Uuid,
        waypoint_id: &str,
        state: WaypointState,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            Self::local_waypoint(bundle, waypoint_id)?.state = Some(state);
            Ok(())
        })
    }

    pub fn record_result(
        &self,
        mission_id: Uuid,
        result: OperatorResult,
        expected: u64,
    ) -> Result<AcceptedMission, MissionError> {
        self.update(mission_id, expected, |bundle| {
            bundle.manifest.operator_results.push(result);
            Ok(())
        })
    }

    pub fn validate(&self, mission_id: Uuid) -> Result<AcceptedMission, MissionError> {
        let accepted = self.store.get(mission_id)?;
        validate_bundle(&accepted.bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        waypoint_relationships::resolve_bundle(
            self.store
                .root
                .ancestors()
                .nth(3)
                .unwrap_or(&self.store.root),
            &accepted.bundle,
        )?;
        Ok(accepted)
    }

    pub fn get(&self, mission_id: Uuid) -> Result<AcceptedMission, MissionError> {
        self.store.get(mission_id)
    }

    pub fn publish(
        &self,
        mission_id: Uuid,
        expected: u64,
        states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<AcceptedMission, MissionError> {
        let bundle = self.next_bundle(mission_id, expected)?;
        self.store
            .publish_with_status_snapshot(mission_id, bundle, expected, states, snapshot_id)
    }

    fn fragment(input: DirectWaypoint) -> WaypointFragment {
        WaypointFragment {
            outcome_plan: None,
            session_package: input.waypoint_id.clone(),
            waypoint_id: input.waypoint_id,
            title: input.title,
            depends_on: input.depends_on,
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            relationships: WaypointRelationships::default(),
            child_mission_ref: None,
            part_markdown: input.prompt.clone(),
            prompt: input.prompt,
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: input.non_goals,
            validation: input.validation,
            commit_checkpoint: input.commit_checkpoint,
        }
    }

    fn update(
        &self,
        mission_id: Uuid,
        expected: u64,
        update: impl FnOnce(&mut MissionBundle) -> Result<(), MissionError>,
    ) -> Result<AcceptedMission, MissionError> {
        let mut bundle = self.next_bundle(mission_id, expected)?;
        update(&mut bundle)?;
        self.store.import(mission_id, bundle, expected)
    }

    fn next_bundle(&self, mission_id: Uuid, expected: u64) -> Result<MissionBundle, MissionError> {
        let mut bundle = self.store.get(mission_id)?.bundle;
        if bundle.manifest.revision != expected {
            return Err(MissionError::StaleRevision {
                expected,
                actual: bundle.manifest.revision,
            });
        }
        bundle.manifest.revision = expected.checked_add(1).ok_or_else(|| {
            MissionError::Io(std::io::Error::other(
                "Mission revision exhausted; cannot allocate another revision.",
            ))
        })?;
        Ok(bundle)
    }

    fn local_waypoint<'a>(
        bundle: &'a mut MissionBundle,
        waypoint_id: &str,
    ) -> Result<&'a mut WaypointFragment, MissionError> {
        let waypoint = bundle
            .waypoints
            .iter_mut()
            .find(|item| item.waypoint_id == waypoint_id)
            .ok_or_else(|| {
                MissionError::Validation(
                    1,
                    vec![Diagnostic {
                        code: "unknown-waypoint",
                        source: "accepted.json".into(),
                        pointer: "/waypoints".into(),
                        waypoint_id: Some(waypoint_id.into()),
                        message: "Use a waypoint ID returned by get.".into(),
                    }],
                )
            })?;
        if waypoint.ticket_ref.is_some()
            || waypoint.child_mission_ref.is_some()
            || matches!(waypoint.scope, Scope::TicketBacked { .. })
        {
            return Err(MissionError::Validation(1, vec![Diagnostic {
                code: "externally-owned-waypoint",
                source: "accepted.json".into(),
                pointer: "/waypoints".into(),
                waypoint_id: Some(waypoint_id.into()),
                message: "Change the authoritative Ticket or child Mission, not local waypoint content or state.".into(),
            }]));
        }
        Ok(waypoint)
    }
}

impl MissionStore {
    fn render_projection(
        &self,
        bundle: &MissionBundle,
        mission_id: Uuid,
        snapshot_id: Option<&str>,
    ) -> Result<RenderedRoadmap, MissionError> {
        let generated_directory = Path::new(".workflow-tools")
            .join("mission")
            .join("missions")
            .join(mission_id.to_string())
            .join("generated");
        let mut rendered = render_with_consumer(
            bundle,
            mission_id,
            snapshot_id,
            self.root.ancestors().nth(3),
            Some(&generated_directory),
        )
        .map_err(|errors| MissionError::Validation(errors.len(), errors))?;
        rendered.parts.insert(
            "HISTORY.md".into(),
            output_history::history(
                Some(&self.root.join(mission_id.to_string())),
                bundle.manifest.revision,
            )?,
        );
        Ok(rendered)
    }

    fn at_workspace(workspace_root: &Path) -> Self {
        let root = workspace_root
            .join(".workflow-tools")
            .join("mission")
            .join("missions");
        Self {
            root,
            #[cfg(test)]
            test_event_metadata: None,
        }
    }

    #[cfg(test)]
    // The test-only integration harness uses these helpers; the unit-test
    // target compiles this module without that integration target.
    #[allow(dead_code)]
    pub(crate) fn with_test_event_metadata(
        mut self,
        event_id: Uuid,
        recorded_at_unix_ms: u64,
    ) -> Self {
        self.test_event_metadata = Some((event_id, recorded_at_unix_ms));
        self
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn check_generated_output(&self, mission_id: Uuid) -> Result<(), MissionError> {
        let accepted = self.get(mission_id)?;
        let rendered = self.render_projection(&accepted.bundle, mission_id, None)?;
        let generated = self.root.join(mission_id.to_string()).join("generated");
        let diagnostics = rendered.check_generated(&generated)?;
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(MissionError::Validation(diagnostics.len(), diagnostics))
        }
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
        self.validate_child_missions(mission_id, &bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.root, mission_id)?;
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
        let source = self.source_for_legacy(&accepted)?;
        self.write_authority(&accepted, source.as_ref(), None)?;
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
        self.publish_inner(mission_id, bundle, expected_current_revision, None, None)
    }

    /// Publish request material and generated output in the same whole-state transaction.
    pub fn publish_with_inputs(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        inputs: &MissionOutputInputs,
        status_snapshot: Option<(&BTreeMap<String, WaypointState>, &str)>,
    ) -> Result<AcceptedMission, MissionError> {
        self.publish_inner(
            mission_id,
            bundle,
            expected_current_revision,
            status_snapshot,
            Some(inputs),
        )
    }

    pub fn publish_with_status_snapshot(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        ticket_states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<AcceptedMission, MissionError> {
        self.publish_inner(
            mission_id,
            bundle,
            expected_current_revision,
            Some((ticket_states, snapshot_id)),
            None,
        )
    }

    fn publish_inner(
        &self,
        mission_id: Uuid,
        bundle: MissionBundle,
        expected_current_revision: u64,
        status_snapshot: Option<(&BTreeMap<String, WaypointState>, &str)>,
        inputs: Option<&MissionOutputInputs>,
    ) -> Result<AcceptedMission, MissionError> {
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.validate_child_missions(mission_id, &bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        self.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.root, mission_id)?;
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
        let rendered = if let Some((ticket_states, snapshot_id)) = status_snapshot {
            self.render_bundle_with_status_snapshot(
                mission_id,
                &bundle,
                ticket_states,
                snapshot_id,
            )?
        } else {
            self.render_projection(&bundle, mission_id, None)?
        };
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
        let source = self.source_for_legacy(&accepted)?;
        self.write_authority_with_inputs(&accepted, source.as_ref(), Some(&rendered), inputs)?;
        self.get(mission_id)
    }

    /// Derive a mission's current state from its accepted ordered waypoints.
    /// Child mission waypoints use the recursively derived child state.
    pub fn derived_state(&self, mission_id: Uuid) -> Result<WaypointState, MissionError> {
        self.derived_state_inner(mission_id, &mut BTreeSet::new())
    }

    /// Materialize externally owned ticket state and recursive child state into
    /// an ephemeral rendering bundle; accepted records are never changed.
    pub fn render_with_status_snapshot(
        &self,
        mission_id: Uuid,
        ticket_states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<RenderedRoadmap, MissionError> {
        let accepted = self.get(mission_id)?;
        self.render_bundle_with_status_snapshot(
            mission_id,
            &accepted.bundle,
            ticket_states,
            snapshot_id,
        )
    }

    pub fn derived_state_with_status_snapshot(
        &self,
        mission_id: Uuid,
        ticket_states: &BTreeMap<String, WaypointState>,
    ) -> Result<WaypointState, MissionError> {
        let accepted = self.get(mission_id)?;
        let mut bundle = accepted.bundle;
        self.materialize_accepted_bundle(mission_id, &mut bundle, ticket_states)?;
        Ok(first_incomplete_state(&bundle))
    }

    fn render_bundle_with_status_snapshot(
        &self,
        mission_id: Uuid,
        source_bundle: &MissionBundle,
        ticket_states: &BTreeMap<String, WaypointState>,
        snapshot_id: &str,
    ) -> Result<RenderedRoadmap, MissionError> {
        validate_bundle(source_bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let mut bundle = source_bundle.clone();
        self.materialize_accepted_bundle(mission_id, &mut bundle, ticket_states)?;
        self.render_projection(&bundle, mission_id, Some(snapshot_id))
    }

    fn materialize_accepted_bundle(
        &self,
        mission_id: Uuid,
        bundle: &mut MissionBundle,
        ticket_states: &BTreeMap<String, WaypointState>,
    ) -> Result<(), MissionError> {
        self.validate_child_missions(mission_id, bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let mut visiting = BTreeSet::from([mission_id]);
        self.materialize_states(bundle, ticket_states, &mut visiting)
    }

    fn materialize_states(
        &self,
        bundle: &mut MissionBundle,
        ticket_states: &BTreeMap<String, WaypointState>,
        visiting: &mut BTreeSet<Uuid>,
    ) -> Result<(), MissionError> {
        for waypoint in &mut bundle.waypoints {
            if let Some(reference) = &waypoint.child_mission_ref {
                let child_id = mission_reference_id(reference).expect("validated child reference");
                if !visiting.insert(child_id) {
                    return Err(MissionError::Validation(
                        1,
                        vec![diag(
                            "child-mission-cycle",
                            Path::new("accepted.json"),
                            "",
                            Some(&waypoint.waypoint_id),
                            "Child mission references must not form a cycle.",
                        )],
                    ));
                }
                let mut child = self.get(child_id)?.bundle;
                self.materialize_states(&mut child, ticket_states, visiting)?;
                waypoint.state = Some(first_incomplete_state(&child));
                visiting.remove(&child_id);
            } else if let Some(reference) = waypoint.ticket_ref.as_ref().or(match &waypoint.scope {
                Scope::TicketBacked { ticket_ref } => Some(ticket_ref),
                Scope::SingleSession => None,
            }) {
                let Some(state) = ticket_states.get(reference).cloned() else {
                    return Err(MissionError::Validation(
                        1,
                        vec![diag(
                            "external-status-unresolved",
                            Path::new("accepted.json"),
                            "/ticket_ref",
                            Some(&waypoint.waypoint_id),
                            &format!(
                                "Ticket status snapshot does not contain canonical reference `{reference}`."
                            ),
                        )],
                    ));
                };
                waypoint.state = Some(state);
            }
        }
        Ok(())
    }

    fn derived_state_inner(
        &self,
        mission_id: Uuid,
        visiting: &mut BTreeSet<Uuid>,
    ) -> Result<WaypointState, MissionError> {
        if !visiting.insert(mission_id) {
            return Err(MissionError::Validation(
                1,
                vec![diag(
                    "child-mission-cycle",
                    Path::new("accepted.json"),
                    "",
                    None,
                    "Child mission references must not form a cycle.",
                )],
            ));
        }
        let accepted = self.get(mission_id)?;
        for waypoint_id in &accepted.bundle.manifest.execution_order {
            let waypoint = accepted
                .bundle
                .waypoints
                .iter()
                .find(|waypoint| waypoint.waypoint_id == *waypoint_id)
                .expect("accepted order is valid");
            let state = match &waypoint.child_mission_ref {
                Some(reference) => {
                    let child =
                        mission_reference_id(reference).expect("accepted child reference is valid");
                    self.derived_state_inner(child, visiting)?
                }
                None => waypoint.state.clone().unwrap_or(WaypointState::Pending),
            };
            if state != WaypointState::Completed {
                visiting.remove(&mission_id);
                return Ok(state);
            }
        }
        visiting.remove(&mission_id);
        Ok(WaypointState::Completed)
    }

    fn validate_child_missions(
        &self,
        mission_id: Uuid,
        bundle: &MissionBundle,
    ) -> Result<(), Vec<Diagnostic>> {
        let workspace = self
            .root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let mut diagnostics = Vec::new();
        self.validate_child_missions_inner(
            mission_id,
            bundle,
            workspace,
            &mut BTreeSet::new(),
            &mut diagnostics,
        );
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }

    fn validate_child_missions_inner(
        &self,
        mission_id: Uuid,
        bundle: &MissionBundle,
        workspace: &str,
        visiting: &mut BTreeSet<Uuid>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !visiting.insert(mission_id) {
            diagnostics.push(diag(
                "child-mission-cycle",
                Path::new("manifest.json"),
                "",
                None,
                "Child mission references must not form a cycle.",
            ));
            return;
        }
        for waypoint in &bundle.waypoints {
            let Some(reference) = &waypoint.child_mission_ref else {
                continue;
            };
            let Some((reference_workspace, child_id)) = mission_reference(reference) else {
                continue;
            };
            if reference_workspace != workspace && reference_workspace != "default" {
                diagnostics.push(diag(
                    "cross-workspace-child-mission",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child missions must resolve from the selected workspace mission store.",
                ));
                continue;
            }
            if visiting.contains(&child_id) {
                diagnostics.push(diag(
                    "child-mission-cycle",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child mission references must not form a cycle.",
                ));
                continue;
            }
            match self.get(child_id) {
                Ok(child) if child.bundle.waypoints.is_empty() => diagnostics.push(diag(
                    "empty-child-mission",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child missions must contain at least one waypoint.",
                )),
                Ok(child) => self.validate_child_missions_inner(
                    child_id,
                    &child.bundle,
                    workspace,
                    visiting,
                    diagnostics,
                ),
                Err(MissionError::NotFound(_)) => diagnostics.push(diag(
                    "unaccepted-child-mission",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child mission must be accepted in the selected workspace.",
                )),
                Err(error) => diagnostics.push(diag(
                    "child-mission-resolution-failed",
                    Path::new("manifest.json"),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    &error.to_string(),
                )),
            }
        }
        visiting.remove(&mission_id);
    }
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), MissionError> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_symlink() {
            return Err(MissionError::Io(std::io::Error::other(format!(
                "refusing symlink in Mission payload: {}",
                entry.path().display()
            ))));
        } else if entry.file_type()?.is_dir() {
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
            outcome_plan: None,
            waypoint_id: id.into(),
            title: id.into(),
            depends_on,
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            relationships: WaypointRelationships::default(),
            child_mission_ref: None,
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
                operator_results: vec![],
            },
            waypoints: vec![
                waypoint("first", vec![]),
                waypoint("second", vec!["first".into()]),
            ],
        }
    }

    #[test]
    fn published_waypoints_may_have_empty_parts_when_other_required_content_is_present() {
        let mut bundle = roadmap(1);
        bundle.waypoints[0].part_markdown.clear();

        assert!(
            validate_bundle(&bundle).is_ok(),
            "Part content is optional; required waypoint metadata and validation gates remain enforced"
        );
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
