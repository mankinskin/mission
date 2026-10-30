use super::*;

pub(super) fn write_new(path: &Path, bytes: &[u8]) -> Result<(), MissionError> {
    use std::io::Write;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}

pub(super) fn sync_tree(path: &Path) -> Result<(), MissionError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    }
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

pub(super) fn promote_authority(stage: &Path, directory: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if PROMOTION_FAILURE.with(|failure| failure.get()) {
        return Err(std::io::Error::other(
            "injected authority promotion failure",
        ));
    }
    fs::rename(stage, directory)
}

#[cfg(test)]
thread_local! {
    static PROMOTION_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static CRASH_AFTER_INTENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static CRASH_BEFORE_PROMOTE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(super) static TYPED_BACKUP_CLEANUP_FAILURE: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

pub(super) fn present(path: &Path) -> std::io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn remove_typed_backup(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if TYPED_BACKUP_CLEANUP_FAILURE.with(std::cell::Cell::get) {
        return Err(std::io::Error::other(
            "injected typed backup cleanup failure",
        ));
    }
    fs::remove_dir_all(path)
}
