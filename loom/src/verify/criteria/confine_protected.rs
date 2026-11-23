//! What a confined command may neither change nor, for a secret, read inside
//! the tree it may write. Only a tree that holds its project's state root has
//! any: the repository root, where a knowledge stage's criteria run. There the
//! tree holds the stage records and job files the daemon acts on, its tokens
//! and terminal histories, the repository's `.git` (whose configuration and
//! hooks git runs as the operator), the stage worktrees and the operator's
//! `.claude` settings. A session in the checkout gets the writes denied by
//! its capsule (`sandbox::control_surfaces::session_denies`, which leaves the
//! rest of `.git` writable for commits) and the secrets by its `denyRead`; a
//! command loom runs on the host for it gets them denied here, `.git` whole,
//! since a criterion commits nothing. The rest of the state root stays
//! readable: knowledge criteria run `loom memory pending` and
//! `loom knowledge check`.
//!
//! Under bubblewrap a secret is hidden by binding `/dev/null` over the file
//! that exists when the command starts. A secret created later stays readable,
//! and so does one replaced by a rename while the command runs: the daemon
//! publishes a token by writing `.<name>.<uuid>.tmp` beside it and renaming
//! that over it (`daemon::server::storage::publish_private_file`), which
//! detaches the cover in the command's mount namespace. Seatbelt denies by
//! path, so a replaced token stays hidden there; the temporary file is hidden
//! on neither. The daemon writes its tokens when it starts; the host child
//! creates the completion attestation key, else made on the first completion,
//! before a knowledge stage's criteria start
//! (`host_verify::child::JobTree::resolve`).
//!
//! A stage worktree's `.loom/work` is a symlink to the main checkout's state
//! root, outside the tree, so none of it is in reach. A knowledge-distill
//! stage's criteria get the one directory they read there, the memory
//! journals (`verify::criteria::memory_journals`), read-only. No secret sits in it or below
//! it, so nothing in the grant is hidden. Bubblewrap mounts the journals at
//! their canonical path and where the worktree's link reaches them; that
//! second path assumes the main checkout's state root is not itself a symlink
//! out of the checkout, as `host_verify::child::JobTree::resolve` does when it
//! finds the checkout from it.

use std::path::{Path, PathBuf};

use crate::fs::permissions::state_root::{resolve_state_root, STATE_ROOT_SECRET_FILES};
use crate::orchestrator::terminal::tmux::history_dir;
#[cfg(target_os = "linux")]
use crate::verify::criteria::MemoryJournals;

/// Entries of a tree holding its state root that stay read-only beside it.
const CHECKOUT_CONTROL_ENTRIES: [&str; 3] = [".git", ".worktrees", ".claude"];

/// Canonical paths inside a confined tree, each bound over it after the tree.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::verify::criteria) struct Protected {
    /// Read-only: the state root and [`CHECKOUT_CONTROL_ENTRIES`].
    pub(in crate::verify::criteria) read_only: Vec<PathBuf>,
    /// Directories strictly between the tree and a read-only path, each bound
    /// onto itself, outermost first. The kernel refuses to rename a mount
    /// point (`EBUSY`), and a renamed ancestor would carry the read-only path
    /// out from under its bind.
    pub(in crate::verify::criteria) pinned: Vec<PathBuf>,
    /// Regular files whose content is hidden: the state root's secrets.
    pub(in crate::verify::criteria) hidden_files: Vec<PathBuf>,
    /// Directories whose content is hidden: the terminal histories.
    pub(in crate::verify::criteria) hidden_dirs: Vec<PathBuf>,
}

/// What the canonical `tree` protects: nothing unless it holds the state root
/// its project keeps at `.loom/work` (or the legacy `.work`). A stage
/// worktree's `.loom/work` is a symlink to the main checkout's, outside the
/// tree and so outside the confinement altogether.
pub(in crate::verify::criteria) fn protected(tree: &Path) -> Protected {
    let Some(state_root) = resolve_state_root(tree).filter(|root| root.starts_with(tree)) else {
        return Protected::default();
    };
    let read_only = std::iter::once(state_root.clone())
        .chain(
            CHECKOUT_CONTROL_ENTRIES
                .iter()
                .map(|entry| tree.join(entry)),
        )
        .filter(|path| path.symlink_metadata().is_ok_and(|meta| !meta.is_symlink()))
        .collect::<Vec<PathBuf>>();
    let pinned = pinned_parents(tree, &read_only);
    let hidden_files = STATE_ROOT_SECRET_FILES
        .iter()
        .map(|name| state_root.join(name))
        .filter(|path| crate::fs::safe_read::is_regular_no_follow(path))
        .collect();
    let history = history_dir(&state_root);
    let hidden_dirs = history
        .symlink_metadata()
        .is_ok_and(|meta| meta.is_dir())
        .then_some(history)
        .into_iter()
        .collect();
    Protected {
        read_only,
        pinned,
        hidden_files,
        hidden_dirs,
    }
}

/// The real directories strictly between `tree` and each of `read_only`,
/// outermost first, without duplicates: `<tree>/.loom` for `<tree>/.loom/work`.
fn pinned_parents(tree: &Path, read_only: &[PathBuf]) -> Vec<PathBuf> {
    let mut pinned: Vec<PathBuf> = Vec::new();
    for path in read_only {
        let mut parents: Vec<&Path> = path
            .ancestors()
            .skip(1)
            .take_while(|ancestor| *ancestor != tree)
            .filter(|ancestor| {
                ancestor
                    .symlink_metadata()
                    .is_ok_and(|meta| meta.is_dir() && !meta.is_symlink())
            })
            .collect();
        parents.reverse();
        for parent in parents {
            if !pinned.iter().any(|known| known == parent) {
                pinned.push(parent.to_path_buf());
            }
        }
    }
    pinned
}

/// Bubblewrap arguments that apply `protected` over the tree's read-write
/// bind, so they must follow it: each pinned directory bound writable onto
/// itself, before the read-only binds (a later self-bind would cover them),
/// each read-only path bound onto itself, each secret file covered by
/// `/dev/null`, each hidden directory by an empty read-only tmpfs.
#[cfg(target_os = "linux")]
pub(in crate::verify::criteria) fn bind(
    command: &mut std::process::Command,
    protected: &Protected,
) {
    for path in &protected.pinned {
        command.arg("--bind").arg(path).arg(path);
    }
    for path in &protected.read_only {
        command.arg("--ro-bind").arg(path).arg(path);
    }
    for file in &protected.hidden_files {
        command.arg("--ro-bind").arg("/dev/null").arg(file);
    }
    for dir in &protected.hidden_dirs {
        command.arg("--tmpfs").arg(dir).arg("--remount-ro").arg(dir);
    }
}

/// Seatbelt rules that apply `protected`; they must follow the rules allowing
/// the tree, since a later rule wins. A pinned directory is denied by
/// `literal`, which stops renaming it and leaves its other children writable.
/// Unmeasured on a Seatbelt host.
#[cfg(target_os = "macos")]
pub(in crate::verify::criteria) fn seatbelt_rules(
    protected: &Protected,
    quote: impl Fn(&Path) -> String,
) -> String {
    let mut rules = String::new();
    for path in &protected.pinned {
        rules.push_str(&format!("(deny file-write* (literal {}))", quote(path)));
    }
    for path in &protected.read_only {
        rules.push_str(&format!("(deny file-write* (subpath {}))", quote(path)));
    }
    for path in protected.hidden_files.iter().chain(&protected.hidden_dirs) {
        rules.push_str(&format!(
            "(deny file-read* file-write* (subpath {}))",
            quote(path)
        ));
    }
    rules
}

/// Bubblewrap arguments mounting `journals` read-only at their canonical path
/// and, when it differs, at the path the worktree's link reaches them.
#[cfg(target_os = "linux")]
pub(super) fn bind_memory_journals(command: &mut std::process::Command, journals: &MemoryJournals) {
    command
        .arg("--ro-bind")
        .arg(&journals.dir)
        .arg(&journals.dir);
    if journals.linked != journals.dir {
        command
            .arg("--ro-bind")
            .arg(&journals.dir)
            .arg(&journals.linked);
    }
}

/// The Seatbelt rule letting a command read the journals directory `quoted`
/// and write none of it. Seatbelt matches the resolved path, so the canonical
/// one covers every way of reaching it.
#[cfg(target_os = "macos")]
pub(super) fn memory_journals_rule(quoted: &str) -> String {
    format!("(allow file-read* (subpath {quoted}))(deny file-write* (subpath {quoted}))")
}

#[cfg(test)]
#[path = "confine_protected_tests.rs"]
mod tests;
