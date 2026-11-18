//! Nothing is mounted at or below [`BOUNDARY_TMPDIR`] but the directory itself.
//!
//! Sandboxed commands may write `TMPDIR`. A mount target below it would get
//! its missing ancestors created there by bubblewrap, and a command could
//! rename one away and put a tree of its own in its place, under a path the
//! boundary treats as fixed.
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};

use super::BOUNDARY_TMPDIR;

/// What one bubblewrap option the boundary emits takes: how many arguments,
/// and which of them (1-based) is a mount target.
struct KnownOption {
    name: &'static str,
    arity: usize,
    target: usize,
}

const fn option(name: &'static str, arity: usize, target: usize) -> KnownOption {
    KnownOption {
        name,
        arity,
        target,
    }
}

/// Every option `linux::command` emits before `--`, and nothing else: an
/// unlisted one (`--overlay`, `--file`, `--bind-data`, ...) could mount
/// something unchecked, so the scan refuses it. Extend this list with the
/// argv when the boundary starts emitting a new option.
const KNOWN: [KnownOption; 14] = [
    option("--die-with-parent", 0, 0),
    option("--unshare-all", 0, 0),
    option("--new-session", 0, 0),
    option("--size", 1, 0),
    option("--perms", 1, 0),
    option("--info-fd", 1, 0),
    option("--chdir", 1, 0),
    option("--proc", 1, 1),
    option("--dev", 1, 1),
    option("--tmpfs", 1, 1),
    option("--dir", 1, 1),
    option("--remount-ro", 1, 1),
    option("--bind", 2, 2),
    option("--ro-bind", 2, 2),
];

/// Whether `path`, as given or resolved, lies at or below [`BOUNDARY_TMPDIR`].
fn under_boundary_tmpdir(path: &Path) -> bool {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.starts_with(BOUNDARY_TMPDIR) || resolved.starts_with(BOUNDARY_TMPDIR)
}

/// Refuse a plan whose worktree, Codex home or host directory (each named
/// for the message) lies under [`BOUNDARY_TMPDIR`].
pub(super) fn refuse_exposed<'a>(
    paths: impl IntoIterator<Item = (&'a str, &'a Path)>,
) -> Result<()> {
    for (name, path) in paths {
        if under_boundary_tmpdir(path) {
            bail!(
                "{name} {} lies under {BOUNDARY_TMPDIR}, the boundary's writable TMPDIR; \
                 native Codex workers need it elsewhere",
                path.display()
            );
        }
    }
    Ok(())
}

/// Refuse `command` when a mount it makes before `--` targets a path under
/// [`BOUNDARY_TMPDIR`], other than creating that directory: this also covers
/// the derived mounts (Git metadata, map stores, read-only roots, the Cargo
/// scratch) no plan names. An option outside [`KNOWN`], or one missing its
/// arguments, is refused.
pub(super) fn refuse_mount_targets(command: &Command) -> Result<()> {
    let args: Vec<&OsStr> = command.get_args().collect();
    let mut index = 0;
    while let Some(flag) = args.get(index) {
        if *flag == "--" {
            return Ok(());
        }
        let Some(known) = KNOWN.iter().find(|known| *flag == known.name) else {
            bail!(
                "the boundary would pass bubblewrap {}, an option its mount check does not know",
                flag.to_string_lossy()
            );
        };
        let Some(values) = args.get(index + 1..index + 1 + known.arity) else {
            bail!("bubblewrap option {} is missing its arguments", known.name);
        };
        index += 1 + known.arity;
        let Some(target) = known.target.checked_sub(1).map(|at| Path::new(values[at])) else {
            continue;
        };
        if known.name == "--dir" && target == Path::new(BOUNDARY_TMPDIR) {
            continue;
        }
        if under_boundary_tmpdir(target) {
            bail!(
                "the boundary would mount {} under {BOUNDARY_TMPDIR}, its writable TMPDIR",
                target.display()
            );
        }
    }
    bail!("the boundary's bubblewrap arguments name no command after --")
}
