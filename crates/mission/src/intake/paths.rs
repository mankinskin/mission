use super::*;

pub(super) fn preflight(root: &Path) -> Result<(), IntakeError> {
    if !root.is_absolute() || root.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(unavailable(
            "workspace root must be an absolute path without `..`",
        ));
    }
    crate::validate_workspace_path(root).map_err(unavailable)?;
    match fs::metadata(root) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => {
            return Err(unavailable(format!(
                "{} is not a directory",
                root.display()
            )));
        }
        Err(error) => return Err(unavailable(format!("{}: {error}", root.display()))),
    }
    check_chain(root, &[])
}

pub(super) fn check_chain(root: &Path, extra: &[&str]) -> Result<(), IntakeError> {
    let mut current = root.to_path_buf();
    for segment in STORE_SEGMENTS.iter().copied().chain(extra.iter().copied()) {
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(unavailable(format!(
                    "{} is a symlink; managed Mission paths must not escape through symlinks",
                    current.display()
                )));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(incompatible(format!(
                    "{} exists but is not a directory; incompatible Mission store",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(unavailable(format!("{}: {error}", current.display()))),
        }
    }
    Ok(())
}

pub(super) fn check_binding(bound: &Path, requested: &Path) -> Result<(), IntakeError> {
    if bound == requested {
        Ok(())
    } else {
        Err(unavailable(format!(
            "request is already bound to Mission workspace root {}; refusing to use {}",
            bound.display(),
            requested.display()
        )))
    }
}
