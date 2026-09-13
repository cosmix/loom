//! The relay state `loom clean` removes (`doc/plans/PLAN-loom-state-confinement.md`
//! section 8): each session's inbox under the state directory, and its
//! scratch directory under the operator-wide scratch root. That root is shared
//! by every repository the operator runs loom in, so only session ids this
//! state directory knows are ever touched there.

use std::collections::BTreeSet;
use std::path::Path;

use colored::Colorize;

use crate::models::session::{Session, SessionStatus};
use crate::parser::frontmatter::parse_from_markdown;

/// Which sessions' relay state an invocation removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RelayScope {
    /// `--sessions`: every session whose record is not Running (or missing).
    NotRunning,
    /// `--state` and `--all`: every session this state directory records,
    /// right before the state directory itself (inboxes included) goes.
    EverySession,
}

/// Remove the relay state `scope` selects; returns how many directories went.
pub(super) fn clean_relay_dirs(
    work_dir: &Path,
    scratch_root: Option<&Path>,
    scope: RelayScope,
) -> usize {
    let mut removed = 0;
    for sid in known_session_ids(work_dir) {
        if scope == RelayScope::NotRunning {
            if record_is_running(work_dir, &sid) {
                continue;
            }
            removed += remove(&work_dir.join("inbox").join(&sid));
        }
        if let Some(root) = scratch_root {
            removed += remove(&root.join(&sid));
        }
    }
    removed
}

pub(super) fn print_relay_cleanup(removed: usize) {
    if removed > 0 {
        println!(
            "  {} Removed {} relay director{}",
            "✓".green().bold(),
            removed,
            if removed == 1 { "y" } else { "ies" }
        );
    }
}

/// Every session id this state directory records or holds an inbox for.
fn known_session_ids(work_dir: &Path) -> BTreeSet<String> {
    let names = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    };
    let recorded = names(&work_dir.join("sessions"))
        .into_iter()
        .filter_map(|name| name.strip_suffix(".md").map(str::to_string));
    recorded
        .chain(names(&work_dir.join("inbox")))
        .filter(|id| crate::validation::validate_id(id).is_ok())
        .collect()
}

fn record_is_running(work_dir: &Path, sid: &str) -> bool {
    std::fs::read_to_string(work_dir.join("sessions").join(format!("{sid}.md")))
        .ok()
        .and_then(|content| parse_from_markdown::<Session>(&content, "Session").ok())
        .is_some_and(|session| session.status == SessionStatus::Running)
}

/// Remove `path` (a symlink by name, never followed); 1 if something went.
fn remove(path: &Path) -> usize {
    let result = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => return 0,
    };
    match result {
        Ok(()) => 1,
        Err(error) => {
            println!(
                "  {} Could not remove {}: {error}",
                "⚠".yellow().bold(),
                path.display()
            );
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::session_files::save_session;
    use std::path::PathBuf;

    struct Tree {
        _tmp: tempfile::TempDir,
        work_dir: PathBuf,
        scratch_root: PathBuf,
    }

    fn tree() -> Tree {
        let tmp = tempfile::tempdir().unwrap();
        let work_dir = tmp.path().join(".loom").join("work");
        std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
        let scratch_root = tmp.path().join("scratch");
        std::fs::create_dir_all(scratch_root.join("session-another-repo")).unwrap();
        Tree {
            _tmp: tmp,
            work_dir,
            scratch_root,
        }
    }

    fn session(tree: &Tree, status: SessionStatus) -> String {
        let mut session = Session::new();
        session.status = status;
        save_session(&session, &tree.work_dir).unwrap();
        std::fs::create_dir_all(tree.work_dir.join("inbox").join(&session.id)).unwrap();
        std::fs::write(
            tree.work_dir
                .join("inbox")
                .join(&session.id)
                .join("ledger.jsonl"),
            "",
        )
        .unwrap();
        std::fs::create_dir_all(tree.scratch_root.join(&session.id)).unwrap();
        session.id
    }

    #[test]
    fn sessions_removes_only_what_sessions_that_are_not_running_left() {
        let tree = tree();
        let running = session(&tree, SessionStatus::Running);
        let done = session(&tree, SessionStatus::Completed);

        let removed = clean_relay_dirs(
            &tree.work_dir,
            Some(&tree.scratch_root),
            RelayScope::NotRunning,
        );

        assert_eq!(removed, 2);
        assert!(tree.work_dir.join("inbox").join(&running).exists());
        assert!(tree.scratch_root.join(&running).exists());
        assert!(!tree.work_dir.join("inbox").join(&done).exists());
        assert!(!tree.scratch_root.join(&done).exists());
        assert!(tree.scratch_root.join("session-another-repo").exists());
    }

    #[test]
    fn state_removes_the_scratch_directory_of_every_recorded_session_only() {
        let tree = tree();
        let running = session(&tree, SessionStatus::Running);
        let done = session(&tree, SessionStatus::Completed);

        clean_relay_dirs(
            &tree.work_dir,
            Some(&tree.scratch_root),
            RelayScope::EverySession,
        );

        assert!(!tree.scratch_root.join(&running).exists());
        assert!(!tree.scratch_root.join(&done).exists());
        assert!(tree.scratch_root.join("session-another-repo").exists());
    }
}
