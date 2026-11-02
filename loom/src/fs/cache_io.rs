//! Directory and publication helpers for private caches and tracker state.
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Create private directories, subject to the process umask.
pub(crate) fn create_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .with_context(|| format!("failed to create directory: {}", dir.display()))
}

/// Refuse a target symlink before replacing a cache, history, or token file.
pub(crate) fn reject_symlink(path: &Path) -> Result<()> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            bail!("refusing to write through a symlink: {}", path.display());
        }
    }
    Ok(())
}

/// Publish with unique temporary files. Callers provide any required lock.
/// A directory-sync failure is returned as a warning after successful rename;
/// an error means the rename did not happen. There is no size limit here.
pub(crate) fn publish(path: &Path, content: &str) -> Result<Option<anyhow::Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .context("publication path has no filename")?;
    // Some cache owners allow their directory itself to be a symlink. Open it
    // once, then anchor the temporary file and rename to that directory.
    let dir = std::fs::File::open(parent)
        .with_context(|| format!("cannot open directory {}", parent.display()))?;
    if !dir.metadata()?.is_dir() {
        bail!("{} is not a directory", parent.display());
    }
    let dir: std::os::fd::OwnedFd = dir.into();
    super::safe_replace::publish_in_dir_reporting(&dir, name, content.as_bytes(), 0o666, false)
}

/// Log durability warnings while preserving success after publication.
pub(crate) fn atomic_write(path: &Path, content: &str) -> Result<()> {
    if let Some(error) = publish(path, content)? {
        tracing::warn!(
            "cannot sync directory after publishing {}: {error:#}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn follows_a_caller_permitted_directory_symlink() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("actual")).unwrap();
        std::os::unix::fs::symlink("actual", root.path().join("link")).unwrap();
        super::atomic_write(&root.path().join("link/file"), "value").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("actual/file")).unwrap(),
            "value"
        );
    }

    #[test]
    fn independent_publishers_do_not_interfere() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.json");
        std::thread::scope(|scope| {
            for n in 0..8 {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..20 {
                        super::atomic_write(path, &format!("{n}")).unwrap();
                    }
                });
            }
        });
        assert!(
            std::fs::read_to_string(path)
                .unwrap()
                .parse::<u8>()
                .unwrap()
                < 8
        );
    }
}
