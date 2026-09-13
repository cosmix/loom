//! Where a sandboxed CLI invocation stages its relay tickets, and the checks
//! that keep that directory from being spoofed.
//!
//! Every function here is pure given its inputs (no process-environment
//! reads); [`scratch_root_from_env`] is the one thin wrapper that reads
//! `XDG_RUNTIME_DIR`/`HOME` and the real uid, for production callers.

use anyhow::{bail, Context, Result};
use std::fs::{self, DirBuilder, Metadata};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

const REQUIRED_MODE: u32 = 0o700;

/// The two operating systems loom runs on, each with its own scratch root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Linux,
    Macos,
}

fn current_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Macos
    } else {
        Platform::Linux
    }
}

/// The directory every session's scratch subdirectory lives under. Never
/// under `/tmp`: every sandbox on the host can write `/tmp/claude-<uid>`, so
/// a root there would let any sandboxed process on the machine plant tickets.
pub fn scratch_root(
    platform: Platform,
    xdg_runtime_dir: Option<&Path>,
    home: &Path,
    uid: u32,
) -> Result<PathBuf> {
    let root = match platform {
        Platform::Linux => linux_scratch_root(xdg_runtime_dir, home, uid),
        Platform::Macos => home
            .join("Library")
            .join("Caches")
            .join("loom")
            .join("scratch"),
    };
    if root.starts_with("/tmp") {
        bail!("scratch root {} must not live under /tmp", root.display());
    }
    Ok(root)
}

fn linux_scratch_root(xdg_runtime_dir: Option<&Path>, home: &Path, uid: u32) -> PathBuf {
    if let Some(xdg) = xdg_runtime_dir {
        if xdg.is_absolute() && is_owned_dir_0700(xdg, uid) {
            return xdg.join("loom").join("scratch");
        }
    }
    home.join(".cache").join("loom").join("scratch")
}

/// Read `XDG_RUNTIME_DIR`, `HOME` and the real uid, then defer to
/// [`scratch_root`]. The only place in this module that touches the process
/// environment.
pub fn scratch_root_from_env() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine the home directory")?;
    let xdg_runtime_dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    scratch_root(current_platform(), xdg_runtime_dir.as_deref(), &home, uid)
}

/// Create `path` if missing (mode 0700), then require it stays a real
/// directory owned by `uid` at mode 0700 either way.
pub fn ensure_dir_0700(path: &Path, uid: u32) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = DirBuilder::new();
            builder.recursive(true).mode(REQUIRED_MODE);
            builder
                .create(path)
                .with_context(|| format!("failed to create {}", path.display()))?;
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to stat {}", path.display()))
        }
    }
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    require_owned_dir_0700(&metadata, path, uid)
}

/// `<root>/<session_id>`, validating the id first so it cannot escape `root`.
pub fn session_dir(root: &Path, session_id: &str) -> Result<PathBuf> {
    crate::validation::validate_id(session_id)
        .context("invalid session id for a scratch directory")?;
    Ok(root.join(session_id))
}

/// The relay hook's check on `LOOM_SCRATCH_DIR` before it trusts anything
/// inside it: a real directory, not a symlink, named for this session, owned
/// by `uid`, mode 0700.
pub fn validate_session_dir(path: &Path, session_id: &str, uid: u32) -> Result<()> {
    let last_component = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("scratch session directory has no final path component")?;
    if last_component != session_id {
        bail!(
            "scratch session directory {} does not end in session id {session_id}",
            path.display()
        );
    }
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    require_owned_dir_0700(&metadata, path, uid)
}

fn is_owned_dir_0700(path: &Path, uid: u32) -> bool {
    fs::symlink_metadata(path)
        .ok()
        .is_some_and(|metadata| require_owned_dir_0700(&metadata, path, uid).is_ok())
}

fn require_owned_dir_0700(metadata: &Metadata, path: &Path, uid: u32) -> Result<()> {
    if metadata.file_type().is_symlink() {
        bail!("{} is a symlink, refusing to use it", path.display());
    }
    if !metadata.is_dir() {
        bail!("{} is not a directory", path.display());
    }
    if metadata.uid() != uid {
        bail!(
            "{} is owned by uid {}, expected {uid}",
            path.display(),
            metadata.uid()
        );
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != REQUIRED_MODE {
        bail!(
            "{} has mode {mode:o}, expected {REQUIRED_MODE:o}",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn current_uid() -> u32 {
        // SAFETY: `getuid` has no preconditions and cannot fail.
        unsafe { libc::getuid() }
    }

    fn make_0700_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
        dir
    }

    // The sandboxed test environment's $TMPDIR lives under /tmp, so a real
    // directory built with `TempDir` cannot stand in for `home` in these
    // tests without tripping the (separately tested) /tmp refusal. Where the
    // resulting root only needs a *real* directory for the XDG-ownership
    // check, this calls the private `linux_scratch_root` helper directly,
    // bypassing that refusal on purpose; everywhere else `home` is a
    // synthetic path the code never stats.

    #[test]
    fn linux_prefers_a_valid_xdg_runtime_dir() {
        let xdg = make_0700_dir();
        let home = Path::new("/home/test-user");
        let root = linux_scratch_root(Some(xdg.path()), home, current_uid());
        assert_eq!(root, xdg.path().join("loom").join("scratch"));
    }

    #[test]
    fn linux_falls_back_to_home_cache_when_xdg_is_not_owned_0700() {
        let xdg = TempDir::new().unwrap();
        fs::set_permissions(xdg.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let home = Path::new("/home/test-user");
        let root = scratch_root(Platform::Linux, Some(xdg.path()), home, current_uid()).unwrap();
        assert_eq!(root, home.join(".cache").join("loom").join("scratch"));
    }

    #[test]
    fn linux_falls_back_to_home_cache_when_xdg_is_absent() {
        let home = Path::new("/home/test-user");
        let root = scratch_root(Platform::Linux, None, home, current_uid()).unwrap();
        assert_eq!(root, home.join(".cache").join("loom").join("scratch"));
    }

    #[test]
    fn macos_uses_library_caches() {
        let home = Path::new("/home/test-user");
        let root = scratch_root(Platform::Macos, None, home, current_uid()).unwrap();
        assert_eq!(
            root,
            home.join("Library")
                .join("Caches")
                .join("loom")
                .join("scratch")
        );
    }

    #[test]
    fn refuses_a_root_under_tmp() {
        let error =
            scratch_root(Platform::Macos, None, Path::new("/tmp"), current_uid()).unwrap_err();
        assert!(error.to_string().contains("/tmp"));
    }

    #[test]
    fn ensure_dir_0700_creates_a_missing_directory() {
        let parent = TempDir::new().unwrap();
        let target = parent.path().join("scratch");
        ensure_dir_0700(&target, current_uid()).unwrap();
        let metadata = fs::symlink_metadata(&target).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.permissions().mode() & 0o777, REQUIRED_MODE);
    }

    #[test]
    fn ensure_dir_0700_refuses_a_symlink() {
        let parent = TempDir::new().unwrap();
        let victim = parent.path().join("victim");
        fs::create_dir(&victim).unwrap();
        let link = parent.path().join("scratch");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        assert!(ensure_dir_0700(&link, current_uid()).is_err());
    }

    #[test]
    fn ensure_dir_0700_refuses_a_wrong_mode_directory() {
        let parent = TempDir::new().unwrap();
        let target = parent.path().join("scratch");
        fs::create_dir(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(ensure_dir_0700(&target, current_uid()).is_err());
    }

    #[test]
    fn session_dir_validates_the_session_id() {
        let root = Path::new("/scratch-root");
        assert!(session_dir(root, "../escape").is_err());
        assert_eq!(
            session_dir(root, "session-1").unwrap(),
            root.join("session-1")
        );
    }

    #[test]
    fn validate_session_dir_accepts_a_matching_0700_directory() {
        let root = make_0700_dir();
        let session = root.path().join("session-1");
        fs::create_dir(&session).unwrap();
        fs::set_permissions(&session, fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
        validate_session_dir(&session, "session-1", current_uid()).unwrap();
    }

    #[test]
    fn validate_session_dir_rejects_a_mismatched_final_component() {
        let root = make_0700_dir();
        let session = root.path().join("session-1");
        fs::create_dir(&session).unwrap();
        fs::set_permissions(&session, fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
        assert!(validate_session_dir(&session, "session-2", current_uid()).is_err());
    }

    #[test]
    fn validate_session_dir_rejects_a_symlink() {
        let root = make_0700_dir();
        let real = root.path().join("real");
        fs::create_dir(&real).unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
        let link = root.path().join("session-1");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(validate_session_dir(&link, "session-1", current_uid()).is_err());
    }
}
