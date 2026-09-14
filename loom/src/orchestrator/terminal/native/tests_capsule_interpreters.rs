//! Unit tests for `session_settings/contents.rs`: the Python-interpreter
//! handling a capsule's hooks go through.

use super::contents::{capsule_settings, CapsuleInputs};
use super::tests_contents::{denies, hooks, sandbox, HOOKS_DIR};
use crate::models::session::SessionType;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// `capsule_settings` for `SessionType::Stage`, with `python_hooks` and
/// `python3` set explicitly, everything else at its plain default.
fn build_with_interpreters(python3: Option<&Path>, python_hooks: &[PathBuf]) -> Value {
    let denies = denies(false, false);
    capsule_settings(&CapsuleInputs {
        kind: SessionType::Stage,
        sandbox: &sandbox(false),
        worktree_rooted: false,
        state_root: Path::new("/repo/.loom/work"),
        repo_root: Path::new("/repo"),
        hooks_dir: Path::new(HOOKS_DIR),
        scratch_dir: Path::new("/scratch/session-1"),
        approved: &[],
        checkout_settings: None,
        denies: &denies,
        python3,
        python_hooks,
    })
    .unwrap()
}

#[test]
fn a_python_hook_runs_under_the_pinned_python3_while_others_keep_bash() {
    let skill_trigger = Path::new(HOOKS_DIR).join("skill-trigger.sh");
    let python3 = PathBuf::from("/usr/bin/python3");
    let settings = build_with_interpreters(Some(&python3), std::slice::from_ref(&skill_trigger));
    let registered = hooks(&settings);
    let expected = format!("/usr/bin/python3 {}", skill_trigger.display());
    assert!(
        registered
            .iter()
            .any(|(_, _, command)| *command == expected),
        "{registered:?}"
    );
    assert!(
        registered.iter().any(|(event, matcher, command)| {
            event == "PostToolUse" && matcher == "Bash" && command.starts_with("/bin/bash ")
        }),
        "{registered:?}"
    );
}

#[test]
fn a_python_hook_with_no_python3_is_dropped_and_the_bash_hooks_remain() {
    let skill_trigger = Path::new(HOOKS_DIR).join("skill-trigger.sh");
    let settings = build_with_interpreters(None, std::slice::from_ref(&skill_trigger));
    let registered = hooks(&settings);
    assert!(
        !registered
            .iter()
            .any(|(_, _, command)| command.ends_with("skill-trigger.sh")),
        "{registered:?}"
    );
    assert!(
        registered.iter().any(|(event, matcher, command)| {
            event == "PostToolUse" && matcher == "Bash" && command.starts_with("/bin/bash ")
        }),
        "{registered:?}"
    );
}
