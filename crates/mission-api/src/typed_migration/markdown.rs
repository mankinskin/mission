use super::*;

pub(crate) fn parse_markdown(input: &str, waypoint_id: &str) -> Result<Document, MissionError> {
    let mut blocks = Vec::new();
    let mut lines = input.lines().enumerate().peekable();
    while let Some((line_no, line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(migration_error(
                waypoint_id,
                line_no + 1,
                "indented content is not supported; use typed block structure",
            ));
        }
        if line.starts_with("~~~") {
            return Err(migration_error(
                waypoint_id,
                line_no + 1,
                "tilde code fences are not supported; use ``` fences",
            ));
        }
        if line.starts_with('<') {
            return Err(migration_error(
                waypoint_id,
                line_no + 1,
                "inline HTML blocks are not supported",
            ));
        }
        if line.starts_with('[') && line.contains("]:") {
            return Err(migration_error(
                waypoint_id,
                line_no + 1,
                "reference-style links are not supported",
            ));
        }
        if line.starts_with("```") {
            let language = line.trim_start_matches("```").to_string();
            let mut text = String::new();
            let mut closed = false;
            for (_, value) in lines.by_ref() {
                if value.starts_with("```") {
                    closed = true;
                    break;
                }
                text.push_str(value);
                text.push('\n');
            }
            if !closed {
                return Err(migration_error(
                    waypoint_id,
                    line_no + 1,
                    "unclosed fenced code block",
                ));
            }
            blocks.push(Block::CodeFence {
                language,
                text: text.trim_end_matches('\n').into(),
            });
            continue;
        }
        if line == "---" || line == "***" {
            blocks.push(Block::ThematicBreak);
            continue;
        }
        if let Some((hashes, text)) = line.split_once(' ') {
            if !hashes.is_empty() && hashes.bytes().all(|b| b == b'#') && hashes.len() <= 6 {
                blocks.push(Block::Heading {
                    level: hashes.len() as u8,
                    inlines: parse_inlines(text)?,
                });
                continue;
            }
        }
        if line.starts_with('|') && line.ends_with('|') {
            let cells = |s: &str| {
                s.trim_matches('|')
                    .split('|')
                    .map(|v| parse_inlines(v.trim()))
                    .collect::<Result<Vec<_>, _>>()
            };
            let header = cells(line)?;
            if lines.peek().is_some_and(|(_, v)| v.contains("---")) {
                lines.next();
            }
            let mut rows = Vec::new();
            while lines
                .peek()
                .is_some_and(|(_, v)| v.starts_with('|') && v.ends_with('|'))
            {
                let (_, row) = lines.next().unwrap();
                rows.push(cells(row)?);
            }
            blocks.push(Block::Table { header, rows });
            continue;
        }
        if let Some(text) = line.strip_prefix("> ") {
            blocks.push(Block::Blockquote {
                blocks: vec![Block::Paragraph {
                    inlines: parse_inlines(text)?,
                }],
            });
            continue;
        }
        if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
            blocks.push(Block::BulletList {
                items: vec![vec![Block::Paragraph {
                    inlines: parse_inlines(text)?,
                }]],
            });
            continue;
        }
        if let Some((number, text)) = line.split_once(". ")
            && number.bytes().all(|b| b.is_ascii_digit())
        {
            blocks.push(Block::OrderedList {
                items: vec![vec![Block::Paragraph {
                    inlines: parse_inlines(text)?,
                }]],
            });
            continue;
        }
        blocks.push(Block::Paragraph {
            inlines: parse_inlines(line)?,
        });
    }
    Ok(Document { blocks })
}

fn parse_inlines(input: &str) -> Result<Vec<Inline>, MissionError> {
    // Preserve all bytes while recognizing the inline syntax encountered in
    // Mission Parts.  Invalid delimiters remain text, rather than disappearing.
    let mut out = Vec::new();
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("**")
            && let Some(end) = after.find("**")
        {
            out.push(Inline::Strong {
                inlines: parse_inlines(&after[..end])?,
            });
            rest = &after[end + 2..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('*')
            && let Some(end) = after.find('*')
        {
            out.push(Inline::Emphasis {
                inlines: parse_inlines(&after[..end])?,
            });
            rest = &after[end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('`')
            && let Some(end) = after.find('`')
        {
            out.push(Inline::Code {
                text: after[..end].into(),
            });
            rest = &after[end + 1..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('[')
            && let Some(mid) = after.find("](")
            && let Some(end) = after[mid + 2..].find(')')
        {
            out.push(Inline::Link {
                inlines: parse_inlines(&after[..mid])?,
                href: after[mid + 2..mid + 2 + end].into(),
            });
            rest = &after[mid + 3 + end..];
            continue;
        }
        let index = rest.char_indices().nth(1).map_or(rest.len(), |(i, _)| i);
        out.push(Inline::Text {
            text: rest[..index].into(),
        });
        rest = &rest[index..];
    }
    Ok(out)
}

pub(crate) fn migration_error(
    waypoint_id: &str,
    line_number: usize,
    message: &str,
) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code: "semantic_or_formatting",
            source: "part_markdown".into(),
            pointer: format!("/waypoints/{waypoint_id}/part_markdown/line/{line_number}"),
            waypoint_id: Some(waypoint_id.into()),
            message: message.into(),
        }],
    )
}

pub(crate) fn describe_migration_error(error: &MissionError) -> String {
    if let MissionError::Validation(_, diagnostics) = error
        && let Some(first) = diagnostics.first()
    {
        return format!("{} {} {}", first.code, first.pointer, first.message);
    }
    error.to_string()
}
