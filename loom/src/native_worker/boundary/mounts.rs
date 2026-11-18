//! Mount items of the worktree and the private Codex home, and the paths they
//! expose. Each optional bind is tested first: a missing source makes
//! bubblewrap fail.
use std::ffi::OsStr;
use std::fs::Metadata;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::context::graph_store::OVERLAY_RELATIVE_DIR;
use crate::context::store::CACHE_RELATIVE_DIR;
use crate::verify::criteria::{git_dirs, NetworkAccess};

/// Codex home files bound writable: token refresh rewrites them.
const AUTH_FILES: [&str; 3] = ["auth.json", "auth.json.lock", "auth.lock"];

/// Marker file of a standalone codex release directory.
const RELEASE_MARKER: &str = "codex-package.json";

/// The canonical codex executable and what of it the boundary binds.
#[derive(Debug)]
pub(super) struct CodexPackage {
    pub(super) program: PathBuf,
    /// Bound read-only: the standalone release directory, else the
    /// executable file alone.
    pub(super) root: PathBuf,
    /// The release's `codex-path` helper directory, put on PATH.
    pub(super) helper_dir: Option<PathBuf>,
}

/// Resolve `program`, refusing a script. A standalone release
/// (`<release>/bin/codex` beside `codex-package.json`) holds only codex
/// and binds whole; any other layout binds the executable alone, so a
/// codex in `~/.local/bin` exposes nothing else of `~/.local`.
pub(super) fn codex_package(program: &Path) -> Result<CodexPackage> {
    let canonical = program
        .canonicalize()
        .with_context(|| format!("resolve the codex executable {}", program.display()))?;
    let mut head = Vec::new();
    std::fs::File::open(&canonical)
        .and_then(|file| file.take(2).read_to_end(&mut head))
        .with_context(|| format!("read {}", canonical.display()))?;
    if head == b"#!" {
        bail!(
            "native Codex workers need the native codex binary; {} is a script",
            canonical.display()
        );
    }
    let release = canonical
        .parent()
        .filter(|bin| bin.file_name() == Some(OsStr::new("bin")))
        .and_then(Path::parent)
        .filter(|dir| {
            let marker = dir.join(RELEASE_MARKER).symlink_metadata();
            marker.is_ok_and(|metadata| metadata.is_file())
        });
    let helper_dir = release.map(|dir| dir.join("codex-path")).filter(|dir| {
        dir.symlink_metadata()
            .is_ok_and(|metadata| metadata.is_dir())
    });
    Ok(CodexPackage {
        root: release.map_or_else(|| canonical.clone(), Path::to_path_buf),
        helper_dir,
        program: canonical,
    })
}

/// Refuse a package directory that contains a path the boundary keeps
/// private or read-only (HOME, the Codex home, the worktree): binding it
/// would expose that path whole.
pub(super) fn refuse_wide_package<'a>(
    root: &Path,
    exposed: impl Iterator<Item = &'a PathBuf>,
) -> Result<()> {
    for path in exposed {
        let path = path.canonicalize().unwrap_or_else(|_| path.clone());
        if path.starts_with(root) {
            bail!(
                "the codex package directory {} also holds {}; install the standalone codex release",
                root.display(),
                path.display()
            );
        }
    }
    Ok(())
}

/// Metadata of `path` without following it; `None` when absent. A
/// symlink is refused: it could redirect a bind outside its policy.
fn entry(path: &Path) -> Result<Option<Metadata>> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            bail!("{} is a symlink; the boundary refuses it", path.display())
        }
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn require_dir(path: &Path) -> Result<()> {
    match entry(path)? {
        Some(metadata) if metadata.is_dir() => Ok(()),
        _ => bail!("{} is not a directory", path.display()),
    }
}

fn regular_file(dir: &Path, name: &str) -> Result<Option<PathBuf>> {
    let path = dir.join(name);
    Ok(entry(&path)?.filter(Metadata::is_file).map(|_| path))
}

fn bind(command: &mut Command, flag: &str, path: &Path) {
    command.arg(flag).arg(path).arg(path);
}

/// The writable worktree `w`, its read-only `.loom` (state root hidden),
/// `.git`, `.codex` and (with `claude`) `.claude`, its Git metadata and the
/// map stores. A symlinked `.loom`, `.git`, `.codex` or `.claude` is refused.
pub(super) fn worktree(command: &mut Command, w: &Path, claude: bool) -> Result<()> {
    bind(command, "--bind", w);
    let loom = w.join(".loom");
    if let Some(metadata) = entry(&loom)? {
        bind(command, "--ro-bind", &loom);
        if metadata.is_dir() {
            hide_state_root(command, w, &loom.join("work"))?;
        }
    }
    let git = w.join(".git");
    if entry(&git)?.is_some() {
        bind(command, "--ro-bind", &git);
    }
    if claude {
        let claude = w.join(".claude");
        require_dir(&claude)?;
        bind(command, "--ro-bind", &claude);
    }
    // Read-only, so no worker plants project Codex configuration for the next one.
    let codex = w.join(".codex");
    if entry(&codex)?.is_some() {
        require_dir(&codex)?;
        bind(command, "--ro-bind", &codex);
    }
    let git_roots = git_dirs(w, NetworkAccess::Denied).map(|dirs| dirs.roots);
    for root in git_roots.unwrap_or_default() {
        bind(command, "--ro-bind", &root);
    }
    for root in map_store_roots(w) {
        bind(command, "--ro-bind", &root);
    }
    Ok(())
}

/// The boundary never mounts the state root (`W/.loom/work`): in a stage
/// worktree it is a link whose target lies outside `w`'s bind, of which
/// only `map_store_roots` binds parts; in a main checkout it is a real
/// directory, covered here by an empty read-only tmpfs. A link resolving
/// into `w` would expose its target through the writable bind: refused.
fn hide_state_root(command: &mut Command, w: &Path, work: &Path) -> Result<()> {
    let metadata = match work.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).with_context(|| format!("inspect {}", work.display())),
    };
    if metadata.file_type().is_symlink() {
        if let Ok(target) = work.canonicalize() {
            if target.starts_with(w) {
                bail!(
                    "{} links into the worktree ({}); the boundary refuses it",
                    work.display(),
                    target.display()
                );
            }
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        bail!("{} is neither a directory nor a symlink", work.display());
    }
    command
        .arg("--tmpfs")
        .arg(work)
        .arg("--remount-ro")
        .arg(work);
    Ok(())
}

/// A private tmpfs at `home` with only `sessions/`, the auth files,
/// `AGENTS.md` and the codex package bound onto it, in that order: the
/// tmpfs first, every bind after it. `sessions/` must exist: the
/// supervisor preflight creates it, mode 0700.
pub(super) fn codex_home(
    command: &mut Command,
    home: &Path,
    package_root: Option<&Path>,
) -> Result<()> {
    require_dir(home)?;
    let sessions = home.join("sessions");
    require_dir(&sessions)?;
    command
        .arg("--size")
        .arg(super::linux::CODEX_HOME_TMPFS_BYTES.to_string())
        .arg("--tmpfs")
        .arg(home);
    hide_system_config(command, Path::new(SYSTEM_CODEX_CONFIG))?;
    bind(command, "--bind", &sessions);
    for name in AUTH_FILES {
        if let Some(path) = regular_file(home, name)? {
            bind(command, "--bind", &path);
        }
    }
    if let Some(agents) = regular_file(home, "AGENTS.md")? {
        bind(command, "--ro-bind", &agents);
    }
    if let Some(root) = package_root {
        bind(command, "--ro-bind", root);
    }
    Ok(())
}

/// Codex's system configuration directory, visible through the read-only `/etc`.
pub(super) const SYSTEM_CODEX_CONFIG: &str = "/etc/codex";

/// Covers the host's system Codex configuration (`config.toml`,
/// `managed_config.toml`, `requirements.toml`) with an empty read-only tmpfs:
/// a worker inherits no operator Codex configuration. A symlink is refused.
pub(super) fn hide_system_config(command: &mut Command, dir: &Path) -> Result<()> {
    if entry(dir)?.is_some_and(|metadata| metadata.is_dir()) {
        command.arg("--tmpfs").arg(dir).arg("--remount-ro").arg(dir);
    }
    Ok(())
}

/// The main checkout's context cache and graph overlays, which `loom map`
/// reads through the worktree's `.loom/work` symlink. Nothing else of the
/// state root is exposed.
pub(super) fn map_store_roots(w: &Path) -> Vec<PathBuf> {
    let Ok(work) = w.join(".loom/work").canonicalize() else {
        return Vec::new();
    };
    if work.starts_with(w) || work.file_name() != Some(OsStr::new("work")) {
        return Vec::new();
    }
    let Some(loom_dir) = work
        .parent()
        .filter(|dir| dir.file_name() == Some(OsStr::new(".loom")))
    else {
        return Vec::new();
    };
    let Ok(cache) = Path::new(CACHE_RELATIVE_DIR).strip_prefix(".loom") else {
        return Vec::new();
    };
    [loom_dir.join(cache), work.join(OVERLAY_RELATIVE_DIR)]
        .into_iter()
        .filter(|dir| dir.canonicalize().is_ok_and(|canonical| canonical == *dir))
        .filter(|dir| dir.is_dir())
        .collect()
}
