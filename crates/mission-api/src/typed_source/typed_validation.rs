use super::*;

pub(crate) fn typed_error(code: &'static str, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "source.json".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.into(),
        }],
    )
}

pub(crate) fn validate_typed(draft: &TypedDraft, publishing: bool) -> Result<(), MissionError> {
    if !matches!(
        draft.schema_version,
        TYPED_SOURCE_SCHEMA_VERSION | OUTCOME_SOURCE_SCHEMA_VERSION
    ) {
        return Err(typed_error(
            "invalid-schema-version",
            &format!(
                "source schema_version must be {} (legacy) or {} (planned outcomes).",
                TYPED_SOURCE_SCHEMA_VERSION, OUTCOME_SOURCE_SCHEMA_VERSION
            ),
        ));
    }
    let mut ids = BTreeSet::new();
    if publishing && draft.waypoints.is_empty() {
        return Err(typed_error(
            "draft-not-ready",
            "publishing requires an ordered waypoint set.",
        ));
    }
    for waypoint in &draft.waypoints {
        outcome_plan::validate_outcome_plan(
            waypoint.outcome_plan.as_ref(),
            draft.schema_version == OUTCOME_SOURCE_SCHEMA_VERSION,
            &waypoint.waypoint_id,
        )
        .map_err(one_diagnostic)?;
        waypoint.relationships.validate().map_err(one_diagnostic)?;
        let refs = &waypoint.ownership_refs;
        if let (Some(id), Some(uri)) = (refs.ticket_id, &refs.ticket_uri)
            && uri
                .rsplit('/')
                .next()
                .and_then(|value| Uuid::parse_str(value).ok())
                != Some(id)
        {
            return Err(typed_error(
                "mission-waypoint-link-identity-mismatch",
                "ownership_refs.ticket_id and ticket_uri must identify the same canonical Ticket.",
            ));
        }
        if waypoint.waypoint_id.is_empty()
            || waypoint.waypoint_id.contains('/')
            || waypoint.waypoint_id.contains('\\')
            || waypoint.waypoint_id == "."
            || waypoint.waypoint_id == ".."
        {
            return Err(typed_error(
                "invalid-waypoint-id",
                "waypoint IDs must be safe path components.",
            ));
        }
        if !ids.insert(&waypoint.waypoint_id) || waypoint.part.waypoint_id != waypoint.waypoint_id {
            return Err(typed_error(
                "invalid-part-reference",
                "each waypoint needs one matching unique managed Part.",
            ));
        }
        if waypoint
            .non_goals
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(typed_error(
                "invalid-non-goal",
                "non_goals entries must be nonblank.",
            ));
        }
        validate_blocks(&waypoint.part.document.blocks)?;
        if publishing
            && (waypoint.validation.is_empty()
                || waypoint
                    .validation
                    .iter()
                    .any(|gate| gate.command.trim().is_empty())
                || waypoint.commit_checkpoint.message.trim().is_empty())
        {
            return Err(typed_error(
                "draft-not-ready",
                "publishing requires a nonblank validation command and checkpoint per waypoint.",
            ));
        }
        if waypoint
            .validation
            .iter()
            .any(|gate| gate.cwd.as_ref().is_some_and(|cwd| cwd.trim().is_empty()))
        {
            return Err(typed_error(
                "invalid-validation-cwd",
                "a supplied execution cwd must be nonblank.",
            ));
        }
    }
    for waypoint in &draft.waypoints {
        let mut dependencies = BTreeSet::new();
        for dependency in &waypoint.depends_on {
            if dependency == &waypoint.waypoint_id
                || !ids.contains(dependency)
                || !dependencies.insert(dependency)
            {
                return Err(typed_error(
                    "invalid-dependency-reference",
                    "dependencies must be unique references to other waypoints.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_blocks(blocks: &[Block]) -> Result<(), MissionError> {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } if *level == 0 || *level > 6 => {
                return Err(typed_error(
                    "invalid-heading-level",
                    "heading levels must be 1 through 6.",
                ));
            }
            Block::Heading { inlines, .. } | Block::Paragraph { inlines } => {
                validate_inlines(inlines)?
            }
            Block::BulletList { items } | Block::OrderedList { items } => {
                for item in items {
                    validate_blocks(item)?;
                }
            }
            Block::Blockquote { blocks } => validate_blocks(blocks)?,
            Block::Table { header, rows } => {
                for row in header {
                    validate_inlines(row)?;
                }
                for row in rows {
                    for cell in row {
                        validate_inlines(cell)?;
                    }
                }
            }
            Block::CodeFence { .. } | Block::ThematicBreak => {}
        }
    }
    Ok(())
}
fn validate_inlines(inlines: &[Inline]) -> Result<(), MissionError> {
    for inline in inlines {
        match inline {
            Inline::Emphasis { inlines }
            | Inline::Strong { inlines }
            | Inline::Link { inlines, .. } => validate_inlines(inlines)?,
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn render_blocks(out: &mut String, blocks: &[Block], depth: usize) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                out.push_str(&"#".repeat(*level as usize));
                out.push(' ');
                render_inlines(out, inlines);
                out.push_str("\n\n");
            }
            Block::Paragraph { inlines } => {
                render_inlines(out, inlines);
                out.push_str("\n\n");
            }
            Block::BulletList { items } | Block::OrderedList { items } => {
                for (i, item) in items.iter().enumerate() {
                    out.push_str(&"  ".repeat(depth));
                    if matches!(block, Block::OrderedList { .. }) {
                        out.push_str(&format!("{}. ", i + 1));
                    } else {
                        out.push_str("- ");
                    }
                    render_blocks(out, item, depth + 1);
                }
            }
            Block::Blockquote { blocks } => {
                let mut nested = String::new();
                render_blocks(&mut nested, blocks, depth);
                for line in nested.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            Block::CodeFence { language, text } => {
                out.push_str("```");
                out.push_str(language);
                out.push('\n');
                out.push_str(text);
                out.push_str("\n```\n\n");
            }
            Block::Table { header, rows } => {
                for row in std::iter::once(header).chain(rows.iter()) {
                    out.push('|');
                    for cell in row {
                        render_inlines(out, cell);
                        out.push('|');
                    }
                    out.push('\n');
                }
            }
            Block::ThematicBreak => out.push_str("---\n\n"),
        }
    }
}
pub(crate) fn render_inlines(out: &mut String, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text { text } => out.push_str(text),
            Inline::Code { text } => {
                out.push('`');
                out.push_str(text);
                out.push('`');
            }
            Inline::HardBreak => out.push_str("  \n"),
            Inline::Emphasis { inlines } => {
                out.push('*');
                render_inlines(out, inlines);
                out.push('*');
            }
            Inline::Strong { inlines } => {
                out.push_str("**");
                render_inlines(out, inlines);
                out.push_str("**");
            }
            Inline::Link { href, inlines } => {
                out.push('[');
                render_inlines(out, inlines);
                out.push_str("](");
                out.push_str(href);
                out.push(')');
            }
        }
    }
}
