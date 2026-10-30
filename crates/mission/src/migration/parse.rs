use super::*;

pub(super) fn parse_roadmap(
    workspace_root: &Path,
    dossier: &Path,
    roadmap_path: &Path,
    source_path: &str,
    revision: u64,
) -> Result<MissionBundle, String> {
    let text = fs::read_to_string(roadmap_path)
        .map_err(|error| format!("Could not read {}: {error}", roadmap_path.display()))?;
    let lines = text.lines().collect::<Vec<_>>();
    let title = lines
        .iter()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(|title| {
            title
                .trim()
                .trim_start_matches("Roadmap:")
                .trim()
                .to_string()
        })
        .filter(|title| !title.is_empty())
        .ok_or_else(|| "The roadmap has no top-level title.".to_string())?;
    if has_legacy_waypoint_index_table(&lines)
        && !lines
            .iter()
            .any(|line| parse_waypoint_heading(line).is_some())
    {
        return Err(
            "The roadmap uses a legacy waypoint index table. Re-author and re-review it with `### Wn. Title` waypoint sections and the required Status, Scope, Mode, Session package, Part, Prompt, Validate, and Commit checkpoint fields before migration."
                .into(),
        );
    }
    let objective = section_text(&lines, "Outcome Summary")
        .or_else(|| section_text(&lines, "Outcome"))
        .filter(|summary| !summary.trim().is_empty())
        .ok_or_else(|| {
            "The roadmap has no non-empty Outcome Summary (or legacy Outcome) section.".to_string()
        })?;

    let mut parsed = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some((label, title)) = parse_waypoint_heading(lines[index]) else {
            index += 1;
            continue;
        };
        let start = index + 1;
        index = start;
        while index < lines.len()
            && !lines[index].trim_start().starts_with("### ")
            && !lines[index].trim_start().starts_with("## ")
        {
            index += 1;
        }
        let fields = table_fields(&lines[start..index]);
        let fragment =
            parse_waypoint(workspace_root, dossier, roadmap_path, title, label, &fields)?;
        parsed.push(fragment);
    }
    if parsed.is_empty() {
        return Err("The roadmap has no `### Wn. Title` waypoint sections.".into());
    }
    for (index, waypoint) in parsed.iter().enumerate() {
        if waypoint.source_label != index + 1 {
            return Err(format!(
                "Source W-labels must be unique and contiguous from W1; found W{} at position {}.",
                waypoint.source_label,
                index + 1
            ));
        }
    }

    let mut labels = BTreeMap::new();
    for waypoint in &parsed {
        if labels
            .values()
            .any(|existing| existing == &waypoint.fragment.waypoint_id)
        {
            return Err(format!(
                "Source titles or explicit IDs produce duplicate waypoint ID `{}`; supply distinct stable IDs.",
                waypoint.fragment.waypoint_id
            ));
        }
        if labels
            .insert(waypoint.source_label, waypoint.fragment.waypoint_id.clone())
            .is_some()
        {
            return Err(format!(
                "Source label W{} appears more than once.",
                waypoint.source_label
            ));
        }
    }
    for waypoint in &mut parsed {
        waypoint.fragment.depends_on = waypoint
            .dependencies
            .iter()
            .map(|label| {
                labels.get(label).cloned().ok_or_else(|| {
                    format!(
                        "Waypoint {} depends on missing source label W{}.",
                        waypoint.fragment.waypoint_id, label
                    )
                })
            })
            .collect::<Result<_, _>>()?;
    }
    let execution_order = parsed
        .iter()
        .map(|waypoint| waypoint.fragment.waypoint_id.clone())
        .collect::<Vec<_>>();
    let waypoints = parsed
        .into_iter()
        .map(|waypoint| waypoint.fragment)
        .collect::<Vec<_>>();
    let roadmap_relative = roadmap_path
        .strip_prefix(workspace_root)
        .unwrap_or(roadmap_path)
        .to_string_lossy()
        .replace('\\', "/");
    let validation_gates =
        backtick_values(&section_text(&lines, "Validation Gates").unwrap_or_default())
            .into_iter()
            .map(|command| ValidationGate { command, cwd: None })
            .collect::<Vec<_>>();

    let bundle = MissionBundle {
        manifest: BundleManifest {
            schema_version: SCHEMA_VERSION,
            revision,
            title,
            objective,
            waypoint_files: Vec::new(),
            execution_order,
            artifacts: vec![roadmap_relative.clone()],
            requirements: Vec::new(),
            validation_gates,
            notes: vec![
                format!("legacy-source: {source_path}"),
                format!("legacy-roadmap: {roadmap_relative}"),
            ],
            operator_results: vec![],
        },
        waypoints,
    };
    validate_bundle(&bundle).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    Ok(bundle)
}

pub(super) fn parse_waypoint(
    workspace_root: &Path,
    dossier: &Path,
    roadmap_path: &Path,
    title: String,
    source_label: usize,
    fields: &BTreeMap<String, String>,
) -> Result<ParsedWaypoint, String> {
    let get = |key: &str| {
        fields
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| format!("W{source_label} is missing the `{key}` field."))
    };
    let explicit_id = fields
        .get("stable id")
        .or_else(|| fields.get("waypoint id"));
    let waypoint_id = explicit_id
        .map(|id| clean_text(id))
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| title_slug(&title));
    if waypoint_id.is_empty() {
        return Err(format!(
            "W{source_label} title cannot produce a safe stable waypoint ID."
        ));
    }
    let mode = match clean_text(get("mode")?).to_ascii_lowercase().as_str() {
        "execution side effect" | "execution-side-effect" => WaypointMode::ExecutionSideEffect,
        "research" | "dependent read-only waypoint" => WaypointMode::Research,
        other => return Err(format!("W{source_label} has unsupported mode `{other}`.")),
    };
    let scope_text = clean_text(get("scope")?).to_ascii_lowercase();
    let scope = if scope_text.contains("ticket") {
        let ticket_text = get("ticket")?;
        let ticket_id = extract_uuid(ticket_text).ok_or_else(|| {
            format!("W{source_label} is ticket-backed but its Ticket field has no UUID.")
        })?;
        let ticket_ref = format!("ce://default/ticket/{ticket_id}");
        Scope::TicketBacked {
            ticket_ref: ticket_ref.clone(),
        }
    } else if scope_text.contains("single-session") || scope_text.contains("single session") {
        Scope::SingleSession
    } else {
        return Err(format!(
            "W{source_label} has unsupported scope `{scope_text}`."
        ));
    };
    let child_mission_ref = fields
        .get("child mission")
        .or_else(|| fields.get("mission"))
        .map(|value| clean_text(value))
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("none"));
    let status = clean_text(get("status")?).to_ascii_lowercase();
    let state = match (&scope, child_mission_ref.as_ref(), status.as_str()) {
        (_, Some(_), _) => None,
        (Scope::TicketBacked { .. }, None, status) if !status.is_empty() => None,
        (Scope::TicketBacked { .. }, None, _) => {
            return Err(format!(
                "W{source_label} has an empty ticket-backed status field."
            ));
        }
        (Scope::SingleSession, None, "pending" | "planned") => Some(WaypointState::Pending),
        (Scope::SingleSession, None, "in-progress" | "in progress") => {
            Some(WaypointState::InProgress)
        }
        (Scope::SingleSession, None, "completed" | "complete" | "done") => {
            Some(WaypointState::Completed)
        }
        (Scope::SingleSession, None, "blocked") => Some(WaypointState::Blocked),
        (Scope::SingleSession, None, other) => {
            return Err(format!(
                "W{source_label} has unsupported single-session status `{other}`."
            ));
        }
    };
    let depends_text = fields
        .get("depends")
        .or_else(|| fields.get("depends on"))
        .map(String::as_str)
        .unwrap_or_default();
    let dependencies = source_labels(depends_text);
    let part_target = markdown_link_target(get("part")?)
        .ok_or_else(|| format!("W{source_label} Part must link to a source Markdown file."))?;
    let part_path = safe_source_file(workspace_root, roadmap_path, &part_target)
        .map_err(|error| format!("W{source_label} Part: {error}"))?;
    if !part_path.starts_with(dossier) {
        return Err(format!(
            "W{source_label} Part resolves outside its source dossier."
        ));
    }
    let part_markdown = fs::read_to_string(&part_path)
        .map_err(|error| format!("Could not read W{source_label} Part: {error}"))?;
    let part_markdown = strip_part_navigation(&part_markdown);
    let ticket_ref = match &scope {
        Scope::TicketBacked { ticket_ref } => Some(ticket_ref.clone()),
        Scope::SingleSession => None,
    };
    let session_package = clean_text(get("session package")?);
    let prompt = clean_text(get("prompt")?);
    let checkpoint = clean_text(get("commit checkpoint")?);
    if session_package.is_empty() || prompt.is_empty() || checkpoint.is_empty() {
        return Err(format!(
            "W{source_label} needs non-empty Session package, Prompt, and Commit checkpoint fields."
        ));
    }

    let validation = fields
        .get("validate")
        .map(|value| backtick_values(value))
        .unwrap_or_default()
        .into_iter()
        .map(|command| ValidationGate { command, cwd: None })
        .collect();
    let artifacts = fields
        .get("artifacts")
        .map(|value| split_items(value))
        .unwrap_or_default();
    let non_goals = fields
        .get("non-goal")
        .or_else(|| fields.get("non-goals"))
        .map(|value| split_items(value))
        .unwrap_or_default();
    Ok(ParsedWaypoint {
        source_label,
        fragment: WaypointFragment {
            outcome_plan: None,
            waypoint_id,
            title,
            depends_on: Vec::new(),
            mode,
            scope,
            state,
            ticket_ref,
            child_mission_ref,
            relationships: Default::default(),
            session_package,
            prompt,
            artifacts,
            requirement_ids: Vec::new(),
            non_goals,
            validation,
            commit_checkpoint: CommitCheckpoint {
                message: checkpoint,
            },
            part_markdown,
        },
        dependencies,
    })
}

pub(super) fn section_text(lines: &[&str], section: &str) -> Option<String> {
    let header = format!("## {section}");
    let start = lines
        .iter()
        .position(|line| line.trim().eq_ignore_ascii_case(&header))?
        + 1;
    let end = lines[start..]
        .iter()
        .position(|line| line.trim_start().starts_with("## "))
        .map_or(lines.len(), |offset| start + offset);
    Some(lines[start..end].join("\n").trim().to_string())
}

pub(super) fn has_legacy_waypoint_index_table(lines: &[&str]) -> bool {
    lines.iter().any(|line| {
        let Some(cells) = split_table_cells(line) else {
            return false;
        };
        cells.len() >= 4
            && cells[0].trim().eq_ignore_ascii_case("waypoint")
            && cells[1].trim().eq_ignore_ascii_case("package")
            && cells[2].trim().eq_ignore_ascii_case("depends on")
            && cells[3].trim().eq_ignore_ascii_case("status")
    })
}

pub(super) fn parse_waypoint_heading(line: &str) -> Option<(usize, String)> {
    let heading = line.trim().strip_prefix("### W")?;
    let (label, title) = heading.split_once(". ")?;
    let label = label.parse().ok()?;
    let title = title.trim();
    (!title.is_empty()).then(|| (label, title.to_string()))
}

pub(super) fn table_fields(lines: &[&str]) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in lines {
        let Some(cells) = split_table_cells(line) else {
            continue;
        };
        if cells.len() < 2 {
            continue;
        }
        let key = cells[0]
            .trim()
            .trim_matches('*')
            .trim()
            .to_ascii_lowercase();
        if !key.is_empty() {
            fields.insert(key, cells[1].trim().to_string());
        }
    }
    fields
}

pub(super) fn split_table_cells(line: &str) -> Option<Vec<String>> {
    let line = line.trim();
    if !line.starts_with('|') || !line.ends_with('|') {
        return None;
    }
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for character in line[1..line.len() - 1].chars() {
        if character == '|' && !escaped {
            cells.push(cell.trim().replace("\\|", "|"));
            cell.clear();
        } else {
            cell.push(character);
        }
        escaped = character == '\\' && !escaped;
        if character != '\\' {
            escaped = false;
        }
    }
    cells.push(cell.trim().replace("\\|", "|"));
    Some(cells)
}

pub(super) fn clean_text(value: &str) -> String {
    value
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("**", "")
        .trim()
        .trim_matches('`')
        .trim()
        .to_string()
}

pub(super) fn split_items(value: &str) -> Vec<String> {
    value
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .lines()
        .map(|line| line.trim().trim_start_matches("- ").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

pub(super) fn backtick_values(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut remaining = value;
    while let Some(start) = remaining.find('`') {
        remaining = &remaining[start + 1..];
        let Some(end) = remaining.find('`') else {
            break;
        };
        let value = remaining[..end].trim();
        if !value.is_empty() {
            values.push(value.to_string());
        }
        remaining = &remaining[end + 1..];
    }
    values
}

pub(super) fn source_labels(value: &str) -> Vec<usize> {
    let chars = value.chars().collect::<Vec<_>>();
    let mut labels = BTreeSet::new();
    let mut index = 0;
    while index + 1 < chars.len() {
        if chars[index].eq_ignore_ascii_case(&'w') && chars[index + 1].is_ascii_digit() {
            let start = index + 1;
            index = start;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            if let Ok(label) = chars[start..index].iter().collect::<String>().parse() {
                labels.insert(label);
            }
        } else {
            index += 1;
        }
    }
    labels.into_iter().collect()
}
