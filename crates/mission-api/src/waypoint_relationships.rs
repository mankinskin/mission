//! Reference-only waypoint interoperability; foreign authorities are read, never mutated.
use super::*;
use spec_api::{SpecManifest, SpecStore};
use test_api::TestStoreConfig;
use ticket_api::{model::ticket::SpecRef, storage::ticket_fs::TicketFs};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaypointRelationships {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "Vec<String>")]
    pub specification_ids: Vec<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub criteria: Vec<CriterionReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executions: Vec<WaypointExecutionReference>,
}

impl WaypointRelationships {
    pub fn is_empty(&self) -> bool {
        self.specification_ids.is_empty() && self.criteria.is_empty() && self.executions.is_empty()
    }

    pub fn validate(&self) -> Result<(), Diagnostic> {
        let invalid =
            |message| contract_diag("mission-waypoint-link-invalid", "/relationships", message);
        let mut specs = BTreeSet::new();
        for id in &self.specification_ids {
            if id.is_nil() || !specs.insert(id) {
                return Err(invalid("Specification IDs must be unique non-nil UUIDs."));
            }
        }
        let mut criteria = BTreeSet::new();
        for reference in &self.criteria {
            reference.validate()?;
            if !criteria.insert(reference) {
                return Err(invalid("Criterion references must be unique."));
            }
        }
        let mut executions = BTreeSet::new();
        for reference in &self.executions {
            if reference.execution_id.trim().is_empty()
                || reference.execution_id.contains(['/', '\\'])
                || matches!(reference.execution_id.as_str(), "." | "..")
                || !executions.insert(&reference.execution_id)
            {
                return Err(invalid(
                    "Execution IDs must be unique nonblank Test IDs, not paths.",
                ));
            }
            if let Some(criterion) = &reference.criterion {
                criterion.validate()?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaypointExecutionReference {
    /// Test-owned opaque execution ID, not a Mission UUID.
    pub execution_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criterion: Option<CriterionReference>,
}

fn failure(reference: &str, message: impl std::fmt::Display) -> MissionError {
    one_diagnostic(contract_diag(
        "mission-waypoint-link-unavailable",
        reference,
        &message.to_string(),
    ))
}

/// Exact local paths only: neither indexes nor symlinks may redirect a link.
fn canonical_file(workspace: &Path, relative: &Path) -> Result<PathBuf, MissionError> {
    let path = workspace.join(relative);
    resolve_artifact_reference(
        workspace,
        Path::new("waypoint-consumer.md"),
        &relative.to_string_lossy(),
    )
    .map_err(|error| failure(&path.to_string_lossy(), error.message))?;
    if path.canonicalize()? != workspace.canonicalize()?.join(relative) {
        return Err(failure(
            &path.to_string_lossy(),
            "Link redirects from its canonical owning root.",
        ));
    }
    Ok(path)
}

/// Shared local URI semantics used by Mission's transport status reader too.
pub fn local_entity_reference_id(
    workspace: &Path,
    reference: &str,
    kind: &str,
) -> Result<Uuid, MissionError> {
    let mut parts = reference.strip_prefix("ce://").unwrap_or("").split('/');
    let owner = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    let raw_id = parts.next().unwrap_or("");
    let selected = workspace
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("default");
    let id = Uuid::parse_str(raw_id).map_err(|error| failure(reference, error))?;
    if parts.next().is_some()
        || domain != kind
        || (owner != "default" && owner != selected)
        || id.is_nil()
    {
        return Err(failure(
            reference,
            "Reference must identify a non-nil canonical entity in the explicitly selected workspace.",
        ));
    }
    Ok(id)
}

fn specification(workspace: &Path, id: Uuid) -> Result<(SpecManifest, String), MissionError> {
    if id.is_nil() {
        return Err(failure(
            &id.to_string(),
            "Specification UUID must be non-nil.",
        ));
    }
    let relative = Path::new(".workflow-tools/spec/specs")
        .join(id.to_string())
        .join("spec.toml");
    let path = canonical_file(workspace, &relative)?;
    let store = SpecStore::open(&workspace.join(".workflow-tools/spec"))
        .map_err(|error| failure(&path.to_string_lossy(), error))?;
    let indexed = store
        .entity_store()
        .get_indexed(&id)
        .map_err(|error| failure(&path.to_string_lossy(), error))?
        .ok_or_else(|| {
            failure(
                &id.to_string(),
                "Specification is not indexed in its owning store.",
            )
        })?;
    if indexed.path.canonicalize()?
        != path
            .parent()
            .ok_or_else(|| failure("", "Missing Spec directory."))?
            .canonicalize()?
    {
        return Err(failure(
            &id.to_string(),
            "Specification index points to another owning root.",
        ));
    }
    let (spec, body) = store
        .get_full(&id.to_string())
        .map_err(|error| failure(&id.to_string(), error))?;
    if spec.id != id {
        return Err(failure(
            &id.to_string(),
            "Specification identity differs from its canonical path.",
        ));
    }
    Ok((spec, body))
}

pub(super) fn linked_ticket_ids(
    workspace: &Path,
    bundle: &MissionBundle,
) -> Result<BTreeSet<Uuid>, MissionError> {
    let mut ids = BTreeSet::new();
    for waypoint in &bundle.waypoints {
        let reference = waypoint.ticket_ref.as_ref().or(match &waypoint.scope {
            Scope::TicketBacked { ticket_ref } => Some(ticket_ref),
            Scope::SingleSession => None,
        });
        if let Some(reference) = reference {
            ids.insert(local_entity_reference_id(workspace, reference, "ticket")?);
        }
    }
    Ok(ids)
}

pub(super) fn resolve_bundle(
    workspace: &Path,
    bundle: &MissionBundle,
) -> Result<BTreeMap<String, String>, MissionError> {
    let mut resolved = BTreeMap::new();
    for waypoint in &bundle.waypoints {
        let links = &waypoint.relationships;
        links.validate().map_err(one_diagnostic)?;
        let ticket_ref = waypoint.ticket_ref.as_ref().or(match &waypoint.scope {
            Scope::TicketBacked { ticket_ref } => Some(ticket_ref),
            Scope::SingleSession => None,
        });
        if ticket_ref.is_none() && links.is_empty() {
            continue;
        }
        let mut output = String::from(
            "Owning workspace: selected workspace root (local records in .workflow-tools/).\n\n",
        );
        let mut specs: BTreeSet<Uuid> = links.specification_ids.iter().copied().collect();
        if let Some(reference) = ticket_ref {
            let id = local_entity_reference_id(workspace, reference, "ticket")?;
            let relative = Path::new(".workflow-tools/ticket/tickets")
                .join(id.to_string())
                .join("ticket.toml");
            let path = canonical_file(workspace, &relative)?;
            let ticket = TicketFs::read(
                path.parent()
                    .ok_or_else(|| failure(reference, "Missing Ticket directory."))?,
            )
            .map_err(|error| failure(reference, error))?;
            if ticket.id != id {
                return Err(failure(
                    reference,
                    "Ticket identity differs from its canonical path.",
                ));
            }
            output.push_str(&render_ticket_entity(&ticket).map_err(one_diagnostic)?);
            let state = ticket
                .extra
                .get("state")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| failure(reference, "Ticket has no string state."))?;
            output.push_str(&format!(
                "Ticket state (read-only): {}\n\n",
                schema_renderers::markdown_text(state)
            ));
            if let Some(value) = ticket.extra.get("related_specs") {
                let values: Vec<&str> = match value {
                    serde_json::Value::String(value) => value
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect(),
                    serde_json::Value::Array(values) => {
                        for value in values {
                            if value.is_object() {
                                let spec_ref: SpecRef = serde_json::from_value(value.clone())
                                    .map_err(|error| failure(reference, error))?;
                                let selected = workspace
                                    .file_name()
                                    .and_then(|name| name.to_str())
                                    .unwrap_or("default");
                                if (spec_ref.workspace != "default"
                                    && spec_ref.workspace != selected)
                                    || spec_ref.store_root != ".workflow-tools/spec"
                                {
                                    return Err(failure(
                                        reference,
                                        "Ticket's Specification reference selects another owning root.",
                                    ));
                                }
                                specs.insert(spec_ref.spec_id);
                            } else if let Some(value) = value.as_str() {
                                specs.insert(
                                    Uuid::parse_str(value)
                                        .map_err(|error| failure(value, error))?,
                                );
                            } else {
                                return Err(failure(
                                    reference,
                                    "related_specs must contain canonical SpecRef objects or UUID strings.",
                                ));
                            }
                        }
                        Vec::new()
                    }
                    _ => return Err(failure(reference, "related_specs must be UUID strings.")),
                };
                for value in values {
                    specs.insert(Uuid::parse_str(value).map_err(|error| failure(value, error))?);
                }
            }
        }
        let mut criteria = BTreeSet::new();
        for reference in &links.criteria {
            criteria.insert(reference.clone());
        }
        for reference in &links.executions {
            // Test owns IDs and outcomes. Checking the exact path first disallows
            // traversal and redirection before the API can read the entity.
            let relative = Path::new(".workflow-tools/test/executions")
                .join(format!("{}.json", reference.execution_id));
            canonical_file(workspace, &relative)?;
            let execution = TestStoreConfig::new(workspace.join(".workflow-tools/test"))
                .get_execution(&reference.execution_id)
                .map_err(|error| failure(&reference.execution_id, error))?;
            if execution.id != reference.execution_id {
                return Err(failure(
                    &reference.execution_id,
                    "Execution identity differs from its canonical path.",
                ));
            }
            if let Some(criterion) = &reference.criterion {
                criterion.validate().map_err(one_diagnostic)?;
                if !execution
                    .links
                    .links_to_spec(&criterion.specification_id.to_string())
                    || !execution
                        .links
                        .links_to_acceptance(criterion.criterion_id.as_str())
                {
                    return Err(failure(
                        &reference.execution_id,
                        "Execution does not link to the selected Specification and criterion.",
                    ));
                }
                criteria.insert(criterion.clone());
            }
            output.push_str(&format!("Execution {} (Test store: .workflow-tools/test): {:?}; validation spec: {}; detail: {}\n\n",
                schema_renderers::markdown_text(&execution.id), execution.outcome,
                schema_renderers::markdown_text(&execution.validation_spec_id),
                schema_renderers::markdown_text(execution.detail.as_deref().unwrap_or(""))));
        }
        for reference in &criteria {
            specs.insert(reference.specification_id);
        }
        for id in specs {
            let (spec, body) = specification(workspace, id)?;
            output.push_str(&render_specification_entity(&spec));
            output.push_str(&format!(
                "Specification body (read-only): {}\n\n",
                schema_renderers::markdown_text(&body)
            ));
        }
        for reference in &criteria {
            let criterion = reference.resolve(workspace)?;
            output
                .push_str(&render_criterion_entity(reference, &criterion).map_err(one_diagnostic)?);
        }
        resolved.insert(waypoint.waypoint_id.clone(), output);
    }
    Ok(resolved)
}

/// Entity-driven roadmap rendering against exactly one selected owning workspace.
pub fn render_in_workspace(
    workspace: &Path,
    bundle: &MissionBundle,
    mission_id: Uuid,
) -> Result<RenderedRoadmap, MissionError> {
    render_with_consumer(bundle, mission_id, None, Some(workspace), None)
        .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))
}
