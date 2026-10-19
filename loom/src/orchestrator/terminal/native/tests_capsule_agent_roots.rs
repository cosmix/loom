//! Unit tests for `session_settings/contents.rs` and `session_denies` with
//! relocated Claude and Codex install roots: the capsule carries the roots'
//! read rules and codex write grant, and denies writing their surfaces.

use super::contents::{capsule_settings, CapsuleInputs};
use super::tests_contents::{sandbox, strings, HOOKS_DIR};
use crate::models::session::SessionType;
use crate::sandbox::control_surfaces::{session_denies, AgentRoots, DenyInputs};
use serde_json::Value;
use std::path::{Path, PathBuf};

const CLAUDE_ROOT: &str = "/opt/agents/claude";
const CODEX_ROOT: &str = "/opt/agents/codex";

fn relocated() -> AgentRoots {
    AgentRoots::relocated(
        Path::new(CLAUDE_ROOT),
        Path::new(CODEX_ROOT),
        Some(Path::new("/home/op")),
    )
}

/// A Stage capsule in the checkout under `roots`, its denies built from them.
fn build(roots: &AgentRoots, codex: bool) -> Value {
    let denies = session_denies(&DenyInputs {
        repo_root: Path::new("/repo"),
        state_root: Path::new("/repo/.loom/work"),
        worktree: None,
        executable_dirs: &[PathBuf::from(HOOKS_DIR)],
        plugin_entries: None,
        writable_roots: &[],
        agent_roots: roots,
    })
    .unwrap();
    capsule_settings(&CapsuleInputs {
        kind: SessionType::Stage,
        sandbox: &sandbox(codex),
        worktree_rooted: false,
        cwd: Path::new("/repo"),
        state_root: Path::new("/repo/.loom/work"),
        repo_root: Path::new("/repo"),
        hooks_dir: Path::new(HOOKS_DIR),
        scratch_dir: Path::new("/scratch/session-1"),
        approved: &[],
        checkout_settings: None,
        denies: &denies,
        python3: None,
        python_hooks: &[],
        hidden_sockets: &[],
        agent_roots: roots,
    })
    .unwrap()
}

#[test]
fn a_relocated_claude_root_gets_the_committed_home_reads() {
    let allow = strings(&build(&relocated(), false), "/permissions/allow");
    for rest in [
        "CLAUDE.md",
        "hooks/loom/**",
        "loom-skill-catalog/**",
        "skills/**",
    ] {
        let rule = format!("Read(/{CLAUDE_ROOT}/{rest})");
        assert!(allow.contains(&rule), "missing {rule}: {allow:?}");
    }
    let default = strings(&build(&AgentRoots::default(), false), "/permissions/allow");
    assert!(
        !default.iter().any(|rule| rule.contains("/opt/agents")),
        "{default:?}"
    );
}

#[test]
fn a_relocated_codex_root_is_writable_only_with_the_codex_lane() {
    let with_codex = strings(&build(&relocated(), true), "/sandbox/filesystem/allowWrite");
    assert!(
        with_codex.contains(&CODEX_ROOT.to_string()),
        "{with_codex:?}"
    );
    let without = strings(
        &build(&relocated(), false),
        "/sandbox/filesystem/allowWrite",
    );
    assert!(!without.contains(&CODEX_ROOT.to_string()), "{without:?}");
}

/// The security regression: with a relocated root, the skills, agents,
/// commands, catalog, settings and Codex hook configuration there must be
/// write-denied in both layers, beside the unchanged `~/` home surfaces.
#[test]
fn relocated_root_surfaces_are_denied_in_both_layers() {
    let settings = build(&relocated(), true);
    let deny_write = strings(&settings, "/sandbox/filesystem/denyWrite");
    let edit = strings(&settings, "/permissions/deny");
    let surfaces = [
        (CLAUDE_ROOT, "hooks", true),
        (CLAUDE_ROOT, "settings.json", false),
        (CLAUDE_ROOT, "agents", true),
        (CLAUDE_ROOT, "skills", true),
        (CLAUDE_ROOT, "commands", true),
        (CLAUDE_ROOT, "loom-skill-catalog", true),
        (CODEX_ROOT, "hooks", true),
        (CODEX_ROOT, "hooks.json", false),
        (CODEX_ROOT, "config.toml", false),
    ];
    for (root, surface, dir) in surfaces {
        let path = format!("{root}/{surface}");
        assert!(deny_write.contains(&path), "denyWrite lacks {path}");
        let rule = if dir {
            format!("Edit(/{path}/**)")
        } else {
            format!("Edit(/{path})")
        };
        assert!(edit.contains(&rule), "permissions.deny lacks {rule}");
    }
    assert!(deny_write.contains(&"~/.claude/skills".to_string()));
    assert!(edit.contains(&"Edit(~/.codex/hooks.json)".to_string()));
}
