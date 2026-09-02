//! Rule 5 preamble warning case for `hooks_spawn_guard.rs`.
//!
//! Split out purely for size: this test - process-tree-gated like the parent's untyped-spawn
//! denial case - pushed the parent file over the 400-line cap, so it lives here instead, sharing
//! the parent's harness (hook installation, `gated_task`, `skip_unless_gate_visible`) via
//! `use super::*` - read the parent's module docs first.

use super::*;

// 8. Missing Rule 5 preamble warns for a loom-* agent; loom-codex-forwarder
//    is exempt (it reads AGENTS.md, never CLAUDE.md).
#[test]
fn missing_rule5_preamble_warns_except_for_codex_forwarder() {
    if skip_unless_gate_visible("missing_rule5_preamble_warns_except_for_codex_forwarder") {
        return;
    }
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    let no_preamble = "just do the task, no preamble here";

    let warned = json!({"subagent_type": "loom-software-engineer", "prompt": no_preamble});
    let out = gated_task(&hook, warned, cwd.path(), home.path(), work.path());
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.contains("LOOM_HOOK_WARN:") && out.stdout.contains("Rule 5 preamble"),
        "stdout={}",
        out.stdout
    );

    let exempt = json!({"subagent_type": "loom-codex-forwarder", "prompt": no_preamble});
    let out = gated_task(&hook, exempt, cwd.path(), home.path(), work.path());
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty(),
        "loom-codex-forwarder must never get a Rule 5 preamble warning: stdout={}",
        out.stdout
    );
}
