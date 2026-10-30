use super::*;

pub(super) fn title_slug(title: &str) -> String {
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

pub(super) fn extract_uuid(value: &str) -> Option<Uuid> {
    value
        .as_bytes()
        .windows(36)
        .filter_map(|candidate| std::str::from_utf8(candidate).ok())
        .find_map(|candidate| {
            let id = Uuid::parse_str(candidate).ok()?;
            (id.to_string() == candidate.to_ascii_lowercase()).then_some(id)
        })
}

pub(super) fn markdown_link_target(value: &str) -> Option<String> {
    let start = value.find("](")? + 2;
    let end = value[start..].find(')')? + start;
    let target = value[start..end].trim().split('#').next()?.trim();
    (!target.is_empty()).then(|| target.to_string())
}

pub(super) fn safe_source_file(
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

pub(super) fn strip_part_navigation(contents: &str) -> String {
    let mut lines = contents.lines();
    let first = lines.next().unwrap_or_default();
    if first.starts_with("Previous Part:") {
        lines.collect::<Vec<_>>().join("\n").trim().to_string()
    } else {
        contents.trim().to_string()
    }
}

pub(super) fn invalid_migration_input(path: PathBuf, message: &str) -> MissionError {
    MissionError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("{}: {message}", path.display()),
    ))
}
