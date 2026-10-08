use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use mission_api::{
    BundleManifest, CommitCheckpoint, MissionBundle, MissionError, MissionStore, SCHEMA_VERSION,
    Scope, ValidationGate, WaypointFragment, WaypointMode, WaypointState, validate_bundle,
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DossierMigrationReport {
    pub dry_run: bool,
    pub migrated: usize,
    pub would_migrate: usize,
    pub skipped: usize,
    pub blocked: usize,
    pub records: Vec<DossierMigrationRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DossierMigrationRecord {
    pub source_path: String,
    pub mission_id: Option<Uuid>,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug)]
struct ParsedWaypoint {
    source_label: usize,
    fragment: WaypointFragment,
    dependencies: Vec<usize>,
}

pub fn migrate_dossiers(
    workspace_root: &Path,
    dossier_path: Option<&Path>,
    dry_run: bool,
) -> Result<DossierMigrationReport, MissionError> {
    let workspace_root = workspace_root.canonicalize()?;
    let transcripts_root = workspace_root.join("transcripts");
    let mut dossiers = if let Some(dossier_path) = dossier_path {
        let selected = if dossier_path.is_absolute() {
            dossier_path.to_path_buf()
        } else {
            workspace_root.join(dossier_path)
        };
        let selected = selected.canonicalize()?;
        if !selected.starts_with(&transcripts_root) || !selected.is_dir() {
            return Err(invalid_migration_input(
                selected,
                "Select a dossier directory inside the workspace transcripts/ folder.",
            ));
        }
        vec![selected]
    } else if transcripts_root.is_dir() {
        let mut directories = Vec::new();
        for entry in fs::read_dir(&transcripts_root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                directories.push(entry.path());
            }
        }
        directories
    } else {
        Vec::new()
    };
    dossiers.sort();

    let store = MissionStore::open(&workspace_root);
    let mut report = DossierMigrationReport {
        dry_run,
        ..DossierMigrationReport::default()
    };
    for dossier in dossiers {
        let source_path = dossier
            .strip_prefix(&workspace_root)
            .unwrap_or(&dossier)
            .to_string_lossy()
            .replace('\\', "/");
        let mission_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("workflow-tools-mission-dossier-v1:{source_path}").as_bytes(),
        );
        let outcome = migrate_one(
            &workspace_root,
            &dossier,
            &source_path,
            mission_id,
            &store,
            dry_run,
        );
        let (outcome, reason) = match outcome {
            Ok((outcome, reason)) => (outcome, reason),
            Err(reason) => ("blocked", Some(reason)),
        };
        match outcome {
            "migrated" => report.migrated += 1,
            "would-migrate" => report.would_migrate += 1,
            "skipped" => report.skipped += 1,
            "blocked" => report.blocked += 1,
            _ => {}
        }
        report.records.push(DossierMigrationRecord {
            source_path,
            mission_id: Some(mission_id),
            outcome: outcome.into(),
            reason,
        });
    }
    Ok(report)
}

fn migrate_one(
    workspace_root: &Path,
    dossier: &Path,
    source_path: &str,
    mission_id: Uuid,
    store: &MissionStore,
    dry_run: bool,
) -> Result<(&'static str, Option<String>), String> {
    let roadmap_path = dossier.join("ROADMAP.md");
    if !roadmap_path.is_file() {
        return Ok(("skipped", Some("No current ROADMAP.md was found.".into())));
    }

    let mut bundle = parse_roadmap(workspace_root, dossier, &roadmap_path, source_path, 1)?;
    for historical_path in historical_roadmap_paths(dossier)? {
        let relative_path = historical_path
            .strip_prefix(workspace_root)
            .unwrap_or(&historical_path)
            .to_string_lossy()
            .replace('\\', "/");
        bundle
            .manifest
            .notes
            .push(format!("legacy-history-excluded: {relative_path}"));
    }
    let marker = format!("legacy-source: {source_path}");
    let current = match store.get(mission_id) {
        Ok(accepted) => Some(accepted),
        Err(MissionError::NotFound(_)) => None,
        Err(error) => return Err(format!("Could not read the existing mission: {error}")),
    };

    let actual_revision = if let Some(accepted) = current.as_ref() {
        if !accepted
            .bundle
            .manifest
            .notes
            .iter()
            .any(|note| note == &marker)
        {
            return Err(format!(
                "Deterministic mission ID {mission_id} is already used by a record with different provenance."
            ));
        }
        let actual_revision = accepted.bundle.manifest.revision;
        if actual_revision != 1 {
            return Err(format!(
                "Stored revision {actual_revision} does not match the current-roadmap-only migration."
            ));
        }
        if accepted.bundle != bundle {
            return Err(format!(
                "Stored revision {actual_revision} differs from its source snapshot; refusing to overwrite it."
            ));
        }
        actual_revision
    } else {
        0
    };

    if dry_run {
        return if actual_revision == 1 {
            Ok(("skipped", Some("This source is already migrated.".into())))
        } else {
            Ok(("would-migrate", None))
        };
    }

    if actual_revision == 1 {
        return Ok(("skipped", Some("This source is already migrated.".into())));
    }

    store
        .publish(mission_id, bundle, 0)
        .map_err(|error| format!("Could not publish the migrated mission: {error}"))?;
    Ok(("migrated", None))
}

fn historical_roadmap_paths(dossier: &Path) -> Result<Vec<PathBuf>, String> {
    let mut historical = Vec::new();
    for entry in fs::read_dir(dossier).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && name
                .strip_prefix("ROADMAP.v")
                .and_then(|name| name.strip_suffix(".md"))
                .is_some_and(|number| {
                    !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
                })
        {
            historical.push(entry.path());
        }
    }
    historical.sort();
    Ok(historical)
}

fn parse_roadmap(
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
    let objective = section_text(&lines, "Outcome Summary")
        .filter(|summary| !summary.trim().is_empty())
        .ok_or_else(|| "The roadmap has no non-empty Outcome Summary section.".to_string())?;

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
    let mut validation_gates =
        backtick_values(&section_text(&lines, "Validation Gates").unwrap_or_default())
            .into_iter()
            .map(|command| ValidationGate { command, cwd: None })
            .collect::<Vec<_>>();
    validation_gates.sort_by(|left, right| left.command.cmp(&right.command));
    validation_gates.dedup_by(|left, right| left.command == right.command);

    let bundle = MissionBundle {
        manifest: BundleManifest {
            schema_version: SCHEMA_VERSION,
            revision,
            title,
            objective,
            waypoint_files: Vec::new(),
            execution_order,
            artifacts: vec![source_path.to_string(), roadmap_relative.clone()],
            requirements: Vec::new(),
            validation_gates,
            notes: vec![
                format!("legacy-source: {source_path}"),
                format!("legacy-roadmap: {roadmap_relative}"),
            ],
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

fn parse_waypoint(
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
        "research" => WaypointMode::Research,
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
    let status = clean_text(get("status")?).to_ascii_lowercase();
    let state = match (&scope, status.as_str()) {
        (Scope::TicketBacked { .. }, status) if !status.is_empty() => None,
        (Scope::TicketBacked { .. }, _) => {
            return Err(format!(
                "W{source_label} has an empty ticket-backed status field."
            ));
        }
        (Scope::SingleSession, "pending" | "planned") => Some(WaypointState::Pending),
        (Scope::SingleSession, "in-progress" | "in progress") => Some(WaypointState::InProgress),
        (Scope::SingleSession, "completed" | "complete" | "done") => Some(WaypointState::Completed),
        (Scope::SingleSession, "blocked") => Some(WaypointState::Blocked),
        (Scope::SingleSession, other) => {
            return Err(format!(
                "W{source_label} has unsupported single-session status `{other}`."
            ));
        }
    };
    let depends_text = fields
        .get("depends")
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
            waypoint_id,
            title,
            depends_on: Vec::new(),
            mode,
            scope,
            state,
            ticket_ref,
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

fn section_text(lines: &[&str], section: &str) -> Option<String> {
    let header = format!("## {section}");
    let start = lines.iter().position(|line| line.trim() == header)? + 1;
    let end = lines[start..]
        .iter()
        .position(|line| line.trim_start().starts_with("## "))
        .map_or(lines.len(), |offset| start + offset);
    Some(lines[start..end].join("\n").trim().to_string())
}

fn parse_waypoint_heading(line: &str) -> Option<(usize, String)> {
    let heading = line.trim().strip_prefix("### W")?;
    let (label, title) = heading.split_once(". ")?;
    let label = label.parse().ok()?;
    let title = title.trim();
    (!title.is_empty()).then(|| (label, title.to_string()))
}

fn table_fields(lines: &[&str]) -> BTreeMap<String, String> {
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

fn split_table_cells(line: &str) -> Option<Vec<String>> {
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

fn clean_text(value: &str) -> String {
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

fn split_items(value: &str) -> Vec<String> {
    value
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .lines()
        .map(|line| line.trim().trim_start_matches("- ").trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn backtick_values(value: &str) -> Vec<String> {
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

fn source_labels(value: &str) -> Vec<usize> {
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

fn title_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    slug
}

fn extract_uuid(value: &str) -> Option<Uuid> {
    value
        .as_bytes()
        .windows(36)
        .filter_map(|candidate| std::str::from_utf8(candidate).ok())
        .find_map(|candidate| {
            let id = Uuid::parse_str(candidate).ok()?;
            (id.to_string() == candidate.to_ascii_lowercase()).then_some(id)
        })
}

fn markdown_link_target(value: &str) -> Option<String> {
    let start = value.find("](")? + 2;
    let end = value[start..].find(')')? + start;
    let target = value[start..end].trim().split('#').next()?.trim();
    (!target.is_empty()).then(|| target.to_string())
}

fn safe_source_file(
    workspace_root: &Path,
    roadmap_path: &Path,
    target: &str,
) -> Result<PathBuf, String> {
    let target = Path::new(target);
    if target.is_absolute() {
        return Err("absolute Part paths are not allowed.".into());
    }
    let candidate = roadmap_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(target);
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("Part path {} is unavailable: {error}", candidate.display()))?;
    if !canonical.starts_with(workspace_root) || !canonical.is_file() {
        return Err(format!(
            "Part path {} escapes the workspace or is not a file.",
            candidate.display()
        ));
    }
    Ok(canonical)
}

fn strip_part_navigation(contents: &str) -> String {
    let mut lines = contents.lines();
    let first = lines.next().unwrap_or_default();
    if first.starts_with("Previous Part:") {
        lines.collect::<Vec<_>>().join("\n").trim().to_string()
    } else {
        contents.trim().to_string()
    }
}

fn invalid_migration_input(path: PathBuf, message: &str) -> MissionError {
    MissionError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("{}: {message}", path.display()),
    ))
}
