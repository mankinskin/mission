use super::*;

pub fn render_mission_entity(mission: &AcceptedMission) -> Result<String, Diagnostic> {
    let common = SharedEntity::from_mission(mission)?;
    Ok(format!(
        "## Mission: {}\n\nIdentity: ce://default/mission/{}\n\n{}\n\n",
        markdown_text(&common.title),
        common.id,
        markdown_text(&mission.bundle.manifest.objective)
    ))
}

/// Reuse the native roadmap renderer, including W1/W2 provenance and references.
pub fn render_roadmap_entity(mission: &AcceptedMission) -> Result<String, Vec<Diagnostic>> {
    let rendered = render(&mission.bundle, mission.mission_id)?;
    Ok(String::from_utf8_lossy(&rendered.roadmap).into_owned())
}

/// Ticket lifecycle and arbitrary typed extras remain Ticket-owned.
pub fn render_ticket_entity(ticket: &TicketManifest) -> Result<String, Diagnostic> {
    let common = SharedEntity::from_ticket(ticket)?;
    Ok(format!(
        "## Ticket: {}\n\nIdentity: ce://default/ticket/{}\n\n",
        markdown_text(&common.title),
        common.id
    ))
}

pub fn render_specification_entity(spec: &SpecManifest) -> String {
    format!(
        "## Specification: {}\n\nIdentity: ce://default/spec/{}\n\n",
        markdown_text(spec.title().unwrap_or("(untitled)")),
        spec.id
    )
}

/// Render a resolved domain criterion only if its ID matches the reference.
pub fn render_criterion_entity(
    reference: &CriterionReference,
    criterion: &ResolvedCriterion,
) -> Result<String, Diagnostic> {
    reference.validate()?;
    let (id, statement) = match &criterion.value {
        CriterionValue::Acceptance(value) => (&value.id, &value.statement),
        CriterionValue::Artifact(value) => (&value.criterion_id, &value.statement),
    };
    if id != reference.criterion_id.as_str()
        || reference.specification_id != criterion.specification_id
    {
        return Err(contract_diag(
            "mission-requirement-criterion-identity-mismatch",
            "/criterion_id",
            "Rendered criterion must match the canonical reference.",
        ));
    }
    Ok(format!(
        "### Criterion: {}\n\nSpecification: {}\n\n{}\n\n",
        markdown_text(id),
        render_criterion_reference(reference),
        markdown_text(statement)
    ))
}

/// Requirement rendering carries links, never copied authoritative criteria.
pub fn render_requirement_entity(requirement: &Requirement) -> String {
    let mut text = format!(
        "### Requirement: {}\n\n{}\n\nDeferred: {}\n\n",
        markdown_text(&requirement.id),
        markdown_text(&requirement.text),
        requirement.deferred
    );
    for reference in &requirement.canonical_criteria {
        text.push_str(&format!("- {}\n", render_criterion_reference(reference)));
    }
    text.push('\n');
    text
}

/// Embeddable native roadmap row, shared with the complete roadmap renderer.
pub fn render_requirement_row(requirement: &Requirement) -> String {
    let references = requirement
        .canonical_criteria
        .iter()
        .map(render_criterion_reference)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "| {} | {} | {} | {} |\n",
        markdown_text(&requirement.id),
        markdown_text(&requirement.text),
        requirement.deferred,
        references
    )
}

/// Link the canonical specification without inventing a criterion URI scheme.
pub fn render_criterion_reference(reference: &CriterionReference) -> String {
    format!(
        "[Spec {}]({}); criterion: {}",
        reference.specification_id,
        reference.specification_uri(),
        markdown_text(reference.criterion_id.as_str())
    )
}

/// Plain-text escaping shared by embeddable entity renderers.
pub(crate) fn markdown_text(value: &str) -> String {
    let mut output = String::new();
    let numbered_prefix = value.chars().take_while(char::is_ascii_digit).count();
    for (index, c) in value.char_indices() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '\n' => output.push_str("<br>"),
            '\r' => {}
            '-' | '+' if index == 0 && value[1..].starts_with(' ') => {
                output.push('\\');
                output.push(c);
            }
            '.' if numbered_prefix > 0
                && index == numbered_prefix
                && value[index + 1..].starts_with(' ') =>
            {
                output.push('\\');
                output.push(c);
            }
            '\\' | '`' | '*' | '_' | '[' | ']' | '#' | '|' | '!' => {
                output.push('\\');
                output.push(c);
            }
            _ => output.push(c),
        }
    }
    output
}

pub(crate) fn contract_diag(code: &'static str, pointer: &str, message: &str) -> Diagnostic {
    diag(
        code,
        Path::new("schema-requirement-renderer"),
        pointer,
        None,
        message,
    )
}

pub(crate) fn one_diagnostic(diagnostic: Diagnostic) -> MissionError {
    MissionError::Validation(1, vec![diagnostic])
}
