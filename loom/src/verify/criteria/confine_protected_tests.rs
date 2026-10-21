//! A command confined to a tree that holds its project's state root (a
//! knowledge stage's criteria, run in the repository root) can neither change
//! the state root and the checkout's control entries nor read the daemon's
//! secrets, while the rest of the tree stays writable.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use tempfile::TempDir;

use super::{protected, Protected};
use crate::fs::permissions::state_root::STATE_ROOT_SECRET_FILES;
use crate::models::stage::CommandConfinement;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use crate::verify::criteria::{run_spec_in_optional_tree, CommandSpec};

const SECRET: &str = "secret-value";

/// A checkout at a canonical temporary path: a state root `.loom/work` with
/// `config.toml`, every secret, a terminal history; a `.git` with `config`;
/// `.worktrees` and `.claude`; and a knowledge directory.
fn checkout() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let state = root.join(".loom/work");
    fs::create_dir_all(state.join("terminal-history")).unwrap();
    fs::write(state.join("terminal-history/session.terminal"), SECRET).unwrap();
    fs::write(state.join("config.toml"), "[plan]\n").unwrap();
    for name in STATE_ROOT_SECRET_FILES {
        fs::write(state.join(name), SECRET).unwrap();
    }
    fs::create_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".git/config"), "[core]\n").unwrap();
    for dir in [".worktrees", ".claude", "doc/loom/knowledge"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    (tmp, root)
}

#[test]
fn a_tree_holding_its_state_root_protects_it_and_the_control_entries() {
    let (_tmp, root) = checkout();
    let state = root.join(".loom/work");

    let found = protected(&root);

    let control = [".git", ".worktrees", ".claude"].map(|entry| root.join(entry));
    let read_only: Vec<PathBuf> = std::iter::once(state.clone()).chain(control).collect();
    assert_eq!(found.read_only, read_only);
    let secrets: Vec<PathBuf> = STATE_ROOT_SECRET_FILES
        .iter()
        .map(|name| state.join(name))
        .collect();
    assert_eq!(found.hidden_files, secrets);
    assert_eq!(found.hidden_dirs, vec![state.join("terminal-history")]);
}

/// A stage worktree's `.loom/work` links to the main checkout's state root,
/// which lies outside the tree, and a package directory holds none.
#[test]
fn a_tree_without_its_own_state_root_protects_nothing() {
    let (_tmp, root) = checkout();
    let worktree = root.join(".worktrees/stage-a");
    fs::create_dir_all(worktree.join(".loom")).unwrap();
    std::os::unix::fs::symlink(root.join(".loom/work"), worktree.join(".loom/work")).unwrap();

    assert_eq!(protected(&worktree), Protected::default());
    assert_eq!(protected(&root.join("doc")), Protected::default());
}

/// Each check prints a word only when it got through; a refused write or
/// read prints nothing, so stdout lists exactly what the command could do.
fn canary() -> String {
    let secrets: Vec<String> = STATE_ROOT_SECRET_FILES
        .iter()
        .map(|name| format!(".loom/work/{name}"))
        .collect();
    format!(
        "touch .loom/work/planted 2>/dev/null && echo STATE_WRITTEN; \
         printf x >> .loom/work/config.toml 2>/dev/null && echo STATE_FILE_WRITTEN; \
         printf x >> .git/config 2>/dev/null && echo GIT_CONFIG_WRITTEN; \
         touch .worktrees/planted .claude/planted 2>/dev/null && echo CONTROL_WRITTEN; \
         echo \"SECRETS[$(cat {} 2>/dev/null)]\"; \
         echo \"HISTORY[$(cat .loom/work/terminal-history/* 2>/dev/null)]\"; \
         echo \"CONFIG[$(cat .loom/work/config.toml)]\"; \
         printf note > doc/loom/knowledge/note.md && echo KNOWLEDGE_WRITTEN",
        secrets.join(" ")
    )
}

#[test]
fn a_knowledge_criterion_cannot_change_the_state_root_or_read_its_secrets() {
    if skip_unless(
        confined_commands_runnable(),
        "a_knowledge_criterion_cannot_change_the_state_root_or_read_its_secrets",
        "loom's confinement cannot start here",
    ) {
        return;
    }
    let (_tmp, root) = checkout();

    // As a knowledge stage's job runs it: no worktree root, so the working
    // directory, the repository root, is the whole tree.
    let result = run_spec_in_optional_tree(
        &CommandSpec::shell(canary()),
        None,
        Some(&root),
        Duration::from_secs(15),
        CommandConfinement::Confined,
    )
    .unwrap();

    assert!(
        result.success,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    let lines: Vec<&str> = result.stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "SECRETS[]",
            "HISTORY[]",
            "CONFIG[[plan]]",
            "KNOWLEDGE_WRITTEN"
        ],
        "stderr={}",
        result.stderr
    );
    assert_only_knowledge_written(&root);
}

/// After the canary: the protected paths are as the fixture left them, and
/// the knowledge directory holds the note.
fn assert_only_knowledge_written(root: &std::path::Path) {
    let state = root.join(".loom/work");
    assert!(!state.join("planted").exists());
    assert_eq!(
        fs::read_to_string(state.join("config.toml")).unwrap(),
        "[plan]\n"
    );
    assert_eq!(
        fs::read_to_string(root.join(".git/config")).unwrap(),
        "[core]\n"
    );
    assert!(!root.join(".worktrees/planted").exists());
    assert!(!root.join(".claude/planted").exists());
    assert_eq!(
        fs::read_to_string(root.join("doc/loom/knowledge/note.md")).unwrap(),
        "note"
    );
}
