//! The instruction files every Claude and Codex session loads are control
//! surfaces: both deny layers cover them, while the paths codex writes at
//! runtime stay open.

use super::*;

fn denies() -> SessionDenies {
    session_denies(&DenyInputs {
        repo_root: Path::new("/repo"),
        state_root: Path::new("/repo/.loom/work"),
        worktree: None,
        executable_dirs: &[],
        plugin_entries: None,
        writable_roots: &[],
    })
    .unwrap()
}

fn assert_denied(denies: &SessionDenies, path: &str, rule: &str) {
    assert!(denies.deny_write.contains(&path.to_string()), "{path}");
    assert!(denies.edit.contains(&rule.to_string()), "{rule}");
}

fn assert_open(denies: &SessionDenies, path: &str) {
    assert!(!denies.deny_write.contains(&path.to_string()), "{path}");
    for rule in [format!("Edit({path})"), format!("Edit({path}/**)")] {
        assert!(!denies.edit.contains(&rule), "{rule}");
    }
}

#[test]
fn the_instruction_files_of_both_home_roots_are_denied() {
    let denies = denies();
    assert_denied(&denies, "~/.claude/CLAUDE.md", "Edit(~/.claude/CLAUDE.md)");
    assert_denied(&denies, "~/.codex/AGENTS.md", "Edit(~/.codex/AGENTS.md)");
    assert_denied(&denies, "~/.codex/prompts", "Edit(~/.codex/prompts/**)");
    assert_denied(
        &denies,
        "~/.codex/loom-skill-catalog",
        "Edit(~/.codex/loom-skill-catalog/**)",
    );
    for name in crate::assets::install::codex_skill_dir_names() {
        let path = format!("~/.codex/skills/{name}");
        assert_denied(&denies, &path, &format!("Edit({path}/**)"));
    }
}

/// Codex runs inside the session's sandbox and writes these itself: its
/// bundled `skills/.system`, exec-policy `rules/`, sessions, logs and auth.
#[test]
fn the_paths_codex_writes_at_runtime_stay_open() {
    let denies = denies();
    for path in [
        "~/.codex/skills",
        "~/.codex/skills/.system",
        "~/.codex/rules",
        "~/.codex/sessions",
        "~/.codex/log",
        "~/.codex/history.jsonl",
        "~/.codex/auth.json",
    ] {
        assert_open(&denies, path);
    }
}
