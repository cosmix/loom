//! Assertion-free fixture helpers of the per-kind sandbox tests: the session kinds and the
//! expected settings that `tests_kinds` and `tests_assert` compare against.

use std::path::PathBuf;

use serde_json::{json, Value};

use super::tests_kinds::Fixture;
use crate::plan_session::kind::SessionKind;

pub(super) fn kinds() -> [SessionKind; 3] {
    [
        SessionKind::Authoring,
        SessionKind::Summary,
        SessionKind::Pressure {
            plan: PathBuf::from("doc/plans/PLAN-a.md"),
        },
    ]
}

fn expected_read_denies(f: &Fixture) -> Value {
    let state = f.root.join(".loom/work");
    json!([
        "~/.ssh/**",
        "~/.aws/**",
        "~/.config/gcloud/**",
        "~/.gnupg/**",
        "~/.claude/.credentials.json",
        "~/.config/gh/hosts.yml",
        "~/.netrc",
        "~/.npmrc",
        "~/.git-credentials",
        "~/.docker/config.json",
        "~/.kube/config",
        "~/.pypirc",
        "~/.cargo/credentials.toml",
        "~/.cargo/credentials",
        "~/.codex/auth.json",
        f.runtime.planning_root,
        f.home.join(".loom/plan"),
        format!("/{}/admin.token", state.display()),
        format!("/{}/user.token", state.display()),
        format!("/{}/completion-attestation.key", state.display()),
        state.join("terminal-history")
    ])
}

pub(super) fn expected_common(f: &Fixture) -> Value {
    json!({
        "sandbox": {
            "enabled": true, "failIfUnavailable": true,
            "autoAllowBashIfSandboxed": false, "allowUnsandboxedCommands": false,
            "network": {"strictAllowlist": true},
            "filesystem": {
                "denyWrite": [], "allowWrite": [], "denyRead": expected_read_denies(f),
                "allowRead": [f.runtime.drafts, f.runtime.claims, f.runtime.summaries_lock]
            }
        },
        "permissions": {
            "defaultMode": "default", "disableBypassPermissionsMode": "disable",
            "allow": [], "ask": [], "deny": []
        },
        "worktree": {"bgIsolation": "none"},
        "hooks": {"PreToolUse": [{
            "matcher": "Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob",
            "hooks": [{"type": "command", "command": f.command("PreToolUse"), "timeout": 10}]
        }]}
    })
}
