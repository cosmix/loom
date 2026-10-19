//! The install-roots record and the instruction files every Claude and Codex
//! session loads are control surfaces: both deny layers and the propagation
//! filter cover them, at home and under a relocated root, while the paths
//! codex writes at runtime stay open.

use super::*;

const HOME: &str = "/home/op";

fn denies(agent_roots: &AgentRoots) -> SessionDenies {
    session_denies(&DenyInputs {
        repo_root: Path::new("/repo"),
        state_root: Path::new("/repo/.loom/work"),
        worktree: None,
        executable_dirs: &[],
        plugin_entries: None,
        writable_roots: &[],
        agent_roots,
    })
    .unwrap()
}

fn relocated() -> AgentRoots {
    AgentRoots::relocated(
        Path::new("/opt/agents/claude"),
        Path::new("/opt/agents/codex"),
        Some(Path::new(HOME)),
    )
}

fn assert_denied(denies: &SessionDenies, path: &str, rule: &str) {
    assert!(denies.deny_write.contains(&path.to_string()), "{path}");
    assert!(denies.edit.contains(&rule.to_string()), "{rule}");
}

fn assert_open(denies: &SessionDenies, path: &str) {
    let rule_path = path
        .strip_prefix('/')
        .map_or(path.to_string(), |p| format!("//{p}"));
    assert!(!denies.deny_write.contains(&path.to_string()), "{path}");
    for rule in [
        format!("Edit({rule_path})"),
        format!("Edit({rule_path}/**)"),
    ] {
        assert!(!denies.edit.contains(&rule), "{rule}");
    }
}

#[test]
fn the_install_roots_record_is_denied_in_both_layers() {
    let denies = denies(&AgentRoots::default());
    assert_denied(
        &denies,
        "~/.config/loom/install-roots.toml",
        "Edit(~/.config/loom/install-roots.toml)",
    );
}

#[test]
fn a_rule_naming_the_install_roots_record_is_refused() {
    let surfaces = ControlSurfaces::new(
        Path::new("/repo/.loom/work"),
        None,
        &[],
        Some(Path::new(HOME)),
        &AgentRoots::default(),
    );
    for rule in [
        "Edit(~/.config/loom/install-roots.toml)",
        "Write(//home/op/.config/loom/install-roots.toml)",
        "Edit(~/.config/loom/**)",
        "Bash(cp evil.toml /home/op/.config/loom/install-roots.toml)",
    ] {
        assert!(surfaces.names(rule), "{rule} must name a control surface");
    }
    assert!(!surfaces.names("Edit(~/.config/other-tool/**)"));
}

#[test]
fn the_instruction_files_of_both_home_roots_are_denied() {
    let denies = denies(&AgentRoots::default());
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
    for roots in [AgentRoots::default(), relocated()] {
        let denies = denies(&roots);
        for path in [
            "~/.codex/skills",
            "~/.codex/skills/.system",
            "~/.codex/rules",
            "~/.codex/sessions",
            "~/.codex/log",
            "~/.codex/history.jsonl",
            "~/.codex/auth.json",
            "/opt/agents/codex/skills",
            "/opt/agents/codex/skills/.system",
            "/opt/agents/codex/rules",
            "/opt/agents/codex/sessions",
        ] {
            assert_open(&denies, path);
        }
    }
}

#[test]
fn a_relocated_root_denies_its_instruction_files_by_absolute_path() {
    let denies = denies(&relocated());
    assert_denied(
        &denies,
        "/opt/agents/claude/CLAUDE.md",
        "Edit(//opt/agents/claude/CLAUDE.md)",
    );
    assert_denied(
        &denies,
        "/opt/agents/codex/AGENTS.md",
        "Edit(//opt/agents/codex/AGENTS.md)",
    );
    for name in ["pressure", "loom-plan-writer"] {
        assert!(crate::assets::install::codex_skill_dir_names().contains(name));
        let path = format!("/opt/agents/codex/skills/{name}");
        assert_denied(&denies, &path, &format!("Edit(/{path}/**)"));
    }
}
