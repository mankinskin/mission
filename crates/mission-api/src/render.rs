use super::*;

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

pub(crate) fn render_with_consumer(
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

pub(crate) fn escape(value: &str) -> String {
    schema_renderers::markdown_text(value)
}
