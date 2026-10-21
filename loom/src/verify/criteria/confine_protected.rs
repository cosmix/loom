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
//! Under bubblewrap a secret is hidden by binding over the file, so one
//! created after the command started stays readable. The daemon writes its
//! tokens when it starts; the host child creates the completion attestation
//! key, else made on the first completion, before a knowledge stage's
//! criteria start (`host_verify::child::JobTree::resolve`).

use std::path::{Path, PathBuf};

use crate::fs::permissions::state_root::{resolve_state_root, STATE_ROOT_SECRET_FILES};
use crate::orchestrator::terminal::tmux::history_dir;

/// Entries of a tree holding its state root that stay read-only beside it.
const CHECKOUT_CONTROL_ENTRIES: [&str; 3] = [".git", ".worktrees", ".claude"];

/// Canonical paths inside a confined tree, each bound over it after the tree.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Protected {
    /// Read-only: the state root and [`CHECKOUT_CONTROL_ENTRIES`].
    pub(super) read_only: Vec<PathBuf>,
    /// Regular files whose content is hidden: the state root's secrets.
    pub(super) hidden_files: Vec<PathBuf>,
    /// Directories whose content is hidden: the terminal histories.
    pub(super) hidden_dirs: Vec<PathBuf>,
}

/// What the canonical `tree` protects: nothing unless it holds the state root
/// its project keeps at `.loom/work` (or the legacy `.work`). A stage
/// worktree's `.loom/work` is a symlink to the main checkout's, outside the
/// tree and so outside the confinement altogether.
pub(super) fn protected(tree: &Path) -> Protected {
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
        .collect();
    let hidden_files = STATE_ROOT_SECRET_FILES
        .iter()
        .map(|name| state_root.join(name))
        .filter(|path| path.symlink_metadata().is_ok_and(|meta| meta.is_file()))
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
        hidden_files,
        hidden_dirs,
    }
}

/// Bubblewrap arguments that apply `protected` over the tree's read-write
/// bind, so they must follow it: each read-only path bound onto itself, each
/// secret file covered by `/dev/null`, each hidden directory by an empty
/// read-only tmpfs.
#[cfg(target_os = "linux")]
pub(super) fn bind(command: &mut std::process::Command, protected: &Protected) {
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
/// the tree, since a later rule wins.
#[cfg(target_os = "macos")]
pub(super) fn seatbelt_rules(protected: &Protected, quote: impl Fn(&Path) -> String) -> String {
    let mut rules = String::new();
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

#[cfg(test)]
#[path = "confine_protected_tests.rs"]
mod tests;
