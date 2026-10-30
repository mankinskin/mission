use super::*;

pub fn resolve_artifact_reference(
    workspace_root: &Path,
    consumer_path: &Path,
    reference: &str,
) -> Result<String, Diagnostic> {
    if reference.starts_with("ce://") {
        return validate_canonical_uri(reference).then(|| reference.to_owned()).ok_or_else(|| {
            reference_diag(
                "mission-reference-unknown",
                consumer_path,
                reference,
                "Canonical entity references must have workspace, store and identifier segments.",
            )
        });
    }
    if reference.is_empty() || reference.contains(':') || reference.contains('\\') {
        return Err(reference_diag(
            "mission-reference-unknown",
            consumer_path,
            reference,
            "Only workspace-relative files and canonical ce:// entity references are supported.",
        ));
    }

    let root = workspace_root.canonicalize().map_err(|_| {
        reference_diag(
            "mission-reference-wrong-root",
            workspace_root,
            reference,
            "The owning workspace root must exist and resolve to a directory.",
        )
    })?;
    let consumer = relative_to_root(&root, consumer_path).ok_or_else(|| {
        reference_diag(
            "mission-reference-wrong-root",
            consumer_path,
            reference,
            "The consumer destination must remain inside its owning workspace.",
        )
    })?;
    if !safe_relative_path(&consumer) {
        return Err(reference_diag(
            "mission-reference-escaping",
            consumer_path,
            reference,
            "The consumer destination must not escape its owning workspace.",
        ));
    }
    ensure_existing_ancestors_inside(&root, &consumer).map_err(|code| {
        reference_diag(
            code,
            consumer_path,
            reference,
            "The consumer destination resolves outside its owning workspace.",
        )
    })?;

    let target = Path::new(reference);
    let relative_target = if target.is_absolute() {
        match target.strip_prefix(&root) {
            Ok(relative) => relative.to_path_buf(),
            Err(_) => {
                return Err(reference_diag(
                    "mission-reference-wrong-root",
                    target,
                    reference,
                    "Absolute artifact references must belong to the owning workspace.",
                ));
            }
        }
    } else {
        target.to_path_buf()
    };
    if !safe_relative_path(&relative_target) {
        return Err(reference_diag(
            "mission-reference-escaping",
            target,
            reference,
            "Artifact references must not escape the owning workspace.",
        ));
    }
    let resolved = root.join(&relative_target);
    let canonical = resolved.canonicalize().map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::NotFound {
            "mission-reference-missing"
        } else {
            "mission-reference-unknown"
        };
        reference_diag(
            code,
            &resolved,
            reference,
            "The referenced artifact must exist and be accessible.",
        )
    })?;
    if !canonical.starts_with(&root) {
        return Err(reference_diag(
            "mission-reference-escaping",
            &resolved,
            reference,
            "Artifact symlinks must resolve inside the owning workspace.",
        ));
    }
    if !canonical.is_file() {
        return Err(reference_diag(
            "mission-reference-unknown",
            &canonical,
            reference,
            "Artifact references must identify regular files.",
        ));
    }
    let target_relative = canonical.strip_prefix(&root).map_err(|_| {
        reference_diag(
            "mission-reference-wrong-root",
            &canonical,
            reference,
            "Artifact reference resolved outside the owning workspace.",
        )
    })?;
    let parent = consumer.parent().unwrap_or_else(|| Path::new(""));
    let relative = relative_path(parent, target_relative);
    Ok(encode_reference_path(&relative))
}

pub(crate) fn validate_canonical_uri(reference: &str) -> bool {
    let Some(rest) = reference.strip_prefix("ce://") else {
        return false;
    };
    let mut parts = rest.split('/');
    let (Some(workspace), Some(store), Some(id), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let known_store = matches!(
        store,
        "ticket"
            | "spec"
            | "test"
            | "session"
            | "mission"
            | "feedback"
            | "log"
            | "rule"
            | "file"
            | "commit"
    );
    known_store && valid_uri_segment(workspace) && valid_uri_segment(id)
}

fn valid_uri_segment(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

pub(super) fn safe_manifest_path(value: &str) -> bool {
    safe_relative_path(Path::new(value))
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn relative_to_root(root: &Path, path: &Path) -> Option<std::path::PathBuf> {
    if path.is_absolute() {
        path.strip_prefix(root).ok().map(Path::to_path_buf)
    } else {
        Some(path.to_path_buf())
    }
}

fn ensure_existing_ancestors_inside(root: &Path, relative: &Path) -> Result<(), &'static str> {
    let mut current = root.join(relative);
    while !current.exists() {
        if fs::symlink_metadata(&current).is_ok() {
            return Err("mission-reference-wrong-root");
        }
        if !current.pop() {
            return Err("mission-reference-wrong-root");
        }
    }
    let canonical = current
        .canonicalize()
        .map_err(|_| "mission-reference-wrong-root")?;
    if canonical.starts_with(root) {
        Ok(())
    } else {
        Err("mission-reference-wrong-root")
    }
}

fn relative_path(from: &Path, to: &Path) -> std::path::PathBuf {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = std::path::PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for component in &to[common..] {
        result.push(component.as_os_str());
    }
    result
}

fn encode_reference_path(path: &Path) -> String {
    path.to_string_lossy()
        .bytes()
        .fold(String::new(), |mut output, byte| {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
                output.push(char::from(byte));
            } else {
                output.push_str(&format!("%{byte:02X}"));
            }
            output
        })
}
