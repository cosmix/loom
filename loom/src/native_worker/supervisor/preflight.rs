//! Host prerequisites of one native worker, checked before anything is spawned.

use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use crate::native_worker::boundary;
use crate::native_worker::model::NativeManifest;

/// `(codex_home, codex_program, loom_program)` once every host prerequisite holds.
pub(super) fn preflight(manifest: &NativeManifest) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    boundary::native_backend_qualified()?;
    let codex_program = crate::codex::find_codex_path()
        .and_then(|path| Ok(path.canonicalize()?))
        .map_err(|e| format!("codex executable not found: {e:#}"))?;
    let loom_program = boundary::loom_program::current()
        .map_err(|e| format!("cannot resolve the loom executable: {e}"))?;
    let codex_home = dirs::home_dir()
        .ok_or("cannot resolve the home directory")?
        .join(".codex");
    prepare_codex_home(&codex_home)?;
    prepare_claude_dir(&manifest.workspace_root)?;
    Ok((codex_home, codex_program, loom_program))
}

/// The Linux boundary binds `<codex_home>/sessions` and the auth files from the
/// host; the macOS boundary copies the auth files into its private home. Either
/// way `auth.json` must exist. This is the only place that creates the host
/// `sessions/`: private, mode 0700.
pub(super) fn prepare_codex_home(codex_home: &Path) -> Result<(), String> {
    ensure_real_dir(&codex_home.join("sessions"), 0o700)?;
    match fs::symlink_metadata(codex_home.join("auth.json")) {
        Ok(meta) if meta.file_type().is_file() => Ok(()),
        Ok(_) => Err("codex auth.json is not a regular file".into()),
        Err(_) => Err("codex is not logged in: auth.json missing".into()),
    }
}

/// The boundary binds `<workspace_root>/.claude` read-only and refuses to start without it.
pub(super) fn prepare_claude_dir(workspace_root: &Path) -> Result<(), String> {
    ensure_real_dir(&workspace_root.join(".claude"), 0o777)
}

/// Creates `path` (and missing parents) with `mode` under the umask when
/// absent; refuses a symlink or a non-directory.
fn ensure_real_dir(path: &Path, mode: u32) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(()),
        Ok(_) => Err(format!(
            "{} is not a directory (a symlink is refused)",
            path.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::DirBuilder::new()
            .recursive(true)
            .mode(mode)
            .create(path)
            .map_err(|e| format!("cannot create {}: {e}", path.display())),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

#[cfg(test)]
#[path = "preflight_tests.rs"]
mod tests;
