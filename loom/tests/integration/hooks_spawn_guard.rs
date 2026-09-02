//! Integration tests for the spawn-guard PreToolUse:Task/Agent hook.
//!
//! An untyped `Task`/`Agent` spawn (no `subagent_type`, or a generic placeholder like
//! `general-purpose`) inherits the SPAWNING session's model, which on an opus stage silently
//! makes every worker opus - defeating CLAUDE.md's cheapest-capable-tier delegation rule.
//! spawn-guard.sh denies that outright, fills in a typed spawn's model from the agent's own
//! definition file (or a built-in table) when `model` is omitted, warns (never denies) on an
//! explicit escalation above the defined tier or a missing Rule 5 preamble, and records every
//! typed spawn to `.loom/work/subagents/<stage-id>/spawns.jsonl`.
//!
//! Regression pinned: the hook installs globally at `~/.claude/hooks/loom/` and runs in EVERY
//! Claude Code session, loom stage or not - it must DENY only with a LIVE LOOM_MAIN_AGENT_PID
//! process ancestor and degrade every would-be denial to a warning otherwise, or it hard-blocks
//! ordinary, non-loom sessions with no escape hatch. Second regression: the ledger line's key
//! order (`ts, stage_id, session_id, caller, agent_type, model, model_source, description`) is a
//! frozen contract `loom subagents` depends on - tests assert the ORDER, not just presence.
//! Runs the hook script directly with bash - no loom invocation.

use loom::fs::permissions::constants::{HOOK_COMMON, HOOK_SPAWN_GUARD};
use loom::process::sandbox_probe::{process_tree_visible, skip_unless};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Exact text of spawn-guard.sh's `PREAMBLE_LINE` (hooks/spawn-guard.sh:64).
const PREAMBLE_LINE: &str = "CLAUDE.md is already in your context; the rules below are the ones that bind you as a subagent. The knowledge you need for this task is quoted in this brief - do not open doc/loom/knowledge/ unless the brief says a pull came back empty.";

/// Contract C1 key order (hooks/spawn-guard.sh:262).
const SPAWN_KEYS: &[&str] = &[
    "\"ts\"",
    "\"stage_id\"",
    "\"session_id\"",
    "\"caller\"",
    "\"agent_type\"",
    "\"model\"",
    "\"model_source\"",
    "\"description\"",
];

/// Install the guard and its `_common.sh` dependency into a temp dir.
fn setup_hook() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().expect("create temp dir");

    let common_path = temp.path().join("_common.sh");
    fs::write(&common_path, HOOK_COMMON).expect("write _common.sh");
    fs::set_permissions(&common_path, fs::Permissions::from_mode(0o755)).expect("chmod");

    let hook_path = temp.path().join("spawn-guard.sh");
    fs::write(&hook_path, HOOK_SPAWN_GUARD).expect("write hook");
    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).expect("chmod");

    (temp, hook_path)
}

fn temp() -> TempDir {
    TempDir::new().expect("create temp dir")
}

/// Every gated test below needs the SAME probe: whether this sandbox can see
/// its own process tree, which is what `is_ancestor` (`hooks/_common.sh`)
/// depends on to confirm a claimed main-agent pid is a live ancestor. `test`
/// is the bare function name; this adds the `hooks_spawn_guard::` prefix so
/// the printed SKIP line names the test the way `cargo test` does.
fn skip_unless_gate_visible(test: &str) -> bool {
    skip_unless(
        process_tree_visible(),
        &format!("hooks_spawn_guard::{test}"),
        "the enforcement gate needs a visible process tree",
    )
}

/// Write `<base>/.claude/agents/<agent_type>.md` with a `model:` frontmatter
/// key, under either a cwd or HOME resolution root.
fn write_agent_def(base: &Path, agent_type: &str, model: &str) {
    let dir = base.join(".claude/agents");
    fs::create_dir_all(&dir).expect("create agents dir");
    let content = format!("---\nname: {agent_type}\nmodel: {model}\n---\n\nBody.\n");
    fs::write(dir.join(format!("{agent_type}.md")), content).expect("write agent def");
}

struct HookOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Spawn the hook with a Claude-Code-shaped payload on stdin, `cwd` as its
/// working directory, and `home` as `HOME` (never the real `~/.claude`).
fn run_hook(
    hook: &Path,
    tool_name: &str,
    tool_input: Value,
    cwd: &Path,
    home: &Path,
    extra_env: &[(&str, &str)],
) -> HookOutput {
    let payload = json!({"tool_name": tool_name, "tool_input": tool_input}).to_string();

    let mut cmd = Command::new("bash");
    cmd.arg(hook)
        .current_dir(cwd)
        .env_remove("LOOM_STAGE_ID")
        .env_remove("LOOM_MAIN_AGENT_PID")
        .env_remove("LOOM_WORK_DIR")
        .env_remove("LOOM_SESSION_ID")
        // Never let the developer's shell leak `loom_debug` output onto stderr.
        .env_remove("LOOM_HOOK_DEBUG")
        .env_remove("COMMIT_FILTER_DEBUG")
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    let mut child = cmd.spawn().expect("spawn hook");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(payload.as_bytes()).ok();
    }
    let output = child.wait_with_output().expect("wait for hook");
    HookOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Run a `Task` spawn as if inside a LIVE loom stage session:
/// LOOM_MAIN_AGENT_PID is this test binary's own pid (the hook's bash is a
/// direct child, so `is_ancestor` finds it in one hop) - satisfies the
/// ENFORCEMENT GATE. Every gated test in this file shares `stage_id ==
/// "test-stage"` unless it needs two independent ledger files.
fn gated_task(hook: &Path, tool_input: Value, cwd: &Path, home: &Path, work: &Path) -> HookOutput {
    let pid = std::process::id().to_string();
    let work_str = work.to_string_lossy().into_owned();
    run_hook(
        hook,
        "Task",
        tool_input,
        cwd,
        home,
        &[
            ("LOOM_STAGE_ID", "test-stage"),
            ("LOOM_MAIN_AGENT_PID", pid.as_str()),
            ("LOOM_WORK_DIR", work_str.as_str()),
        ],
    )
}

/// Walk a path of object keys and return the leaf as `&str`.
fn get_str<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    cur.as_str()
}

/// Read the single line written to `<work_dir>/subagents/test-stage/spawns.jsonl`.
fn read_spawn_line(work_dir: &Path) -> String {
    let path = work_dir.join("subagents/test-stage/spawns.jsonl");
    let content =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "expected one spawn record in {}: {content:?}",
        path.display()
    );
    lines[0].to_string()
}

/// Assert every SPAWN_KEYS key appears in `line` in order: searching each key
/// only in the remainder AFTER the previous match proves strictly increasing
/// byte offsets, since an out-of-order key would not be found there.
fn assert_spawn_key_order(line: &str) {
    let mut last = 0usize;
    for key in SPAWN_KEYS {
        let pos = line[last..]
            .find(key)
            .unwrap_or_else(|| panic!("{key} not found in order after offset {last}: {line}"));
        last += pos + key.len();
    }
}

// 1. Untyped/placeholder spawns are DENIED inside a live loom stage session.
#[test]
fn gated_untyped_or_placeholder_spawn_denied() {
    if skip_unless_gate_visible("gated_untyped_or_placeholder_spawn_denied") {
        return;
    }
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());

    for subagent_type in [None, Some("general-purpose"), Some("claude"), Some("Plan")] {
        let tool_input = match subagent_type {
            Some(t) => json!({"subagent_type": t}),
            None => json!({}),
        };
        let out = gated_task(&hook, tool_input, cwd.path(), home.path(), work.path());

        assert_eq!(
            out.code, 2,
            "subagent_type={subagent_type:?} stderr={}",
            out.stderr
        );
        assert!(
            out.stderr
                .contains("Untyped spawn inherits this session's model"),
            "subagent_type={subagent_type:?} stderr={}",
            out.stderr
        );
    }
}

// 2. Ungated: the same spawn is a warning, never a block.
#[test]
fn ungated_untyped_spawn_warns_instead_of_blocking() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());

    let out = run_hook(&hook, "Task", json!({}), cwd.path(), home.path(), &[]);

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    let ctx = get_str(&v, &["hookSpecificOutput", "additionalContext"])
        .expect("additionalContext present");
    assert!(ctx.starts_with("LOOM_HOOK_WARN:"), "ctx={ctx}");
}

// 3. Typed spawn without `model` gets updatedInput carrying the definition's
//    model, with every other tool_input key preserved unchanged.
#[test]
fn typed_spawn_without_model_fills_in_defined_tier_and_preserves_other_keys() {
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");

    let prompt = format!("{PREAMBLE_LINE}\ndo the xyz123 task");
    let description = "xyz123-description";
    let tool_input = json!({
        "subagent_type": "loom-software-engineer",
        "description": description,
        "prompt": prompt,
    });
    let out = gated_task(&hook, tool_input, cwd.path(), home.path(), work.path());

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "permissionDecision"]),
        Some("allow")
    );
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "updatedInput", "model"]),
        Some("sonnet")
    );
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "updatedInput", "description"]),
        Some(description)
    );
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "updatedInput", "prompt"]),
        Some(prompt.as_str())
    );
}

// 4. Escalation: an explicit model AT the defined tier is a silent allow; a
//    positive control proves an explicit model ABOVE it warns as escalation.
#[test]
fn explicit_model_escalation_only_warns_above_defined_tier() {
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    write_agent_def(cwd.path(), "loom-senior-software-engineer", "opus");
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let prompt = format!("{PREAMBLE_LINE}\ndo the task");

    let at_tier = json!({
        "subagent_type": "loom-senior-software-engineer", "model": "opus", "prompt": prompt,
    });
    let out = gated_task(&hook, at_tier, cwd.path(), home.path(), work.path());
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty(),
        "explicit model matching the defined tier must be a silent allow: stdout={}",
        out.stdout
    );

    let escalated = json!({
        "subagent_type": "loom-software-engineer", "model": "opus", "prompt": prompt,
    });
    let out = gated_task(&hook, escalated, cwd.path(), home.path(), work.path());
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    let ctx = get_str(&v, &["hookSpecificOutput", "additionalContext"])
        .expect("additionalContext present");
    assert!(
        ctx.contains("LOOM_HOOK_WARN:") && ctx.contains("escalation"),
        "ctx={ctx}"
    );
}

// 5. Explore with no definition file anywhere resolves sonnet from the
//    built-in table.
#[test]
fn explore_with_no_definition_file_resolves_sonnet_from_builtin_table() {
    let (_temp, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());

    let out = gated_task(
        &hook,
        json!({"subagent_type": "Explore"}),
        cwd.path(),
        home.path(),
        work.path(),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let v: Value = serde_json::from_str(out.stdout.trim()).expect("parse stdout json");
    assert_eq!(
        get_str(&v, &["hookSpecificOutput", "updatedInput", "model"]),
        Some("sonnet")
    );
}

// 6. The spawn ledger: key order is a frozen contract (C1); model_source
//    distinguishes a filled-in default from an explicit passthrough.
#[test]
fn spawn_ledger_records_ordered_fields() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let prompt = format!("{PREAMBLE_LINE}\ndo the ledger task");
    let description = "ledger-description-xyz";

    let work_a = temp();
    let tool_input_a = json!({
        "subagent_type": "loom-software-engineer", "description": description, "prompt": prompt,
    });
    let out_a = gated_task(&hook, tool_input_a, cwd.path(), home.path(), work_a.path());
    assert_eq!(out_a.code, 0, "stderr={}", out_a.stderr);

    let line_a = read_spawn_line(work_a.path());
    assert_spawn_key_order(&line_a);
    let v_a: Value = serde_json::from_str(&line_a).expect("parse spawn line a");
    assert_eq!(v_a["stage_id"], "test-stage");
    assert_eq!(v_a["caller"], "main");
    assert_eq!(v_a["agent_type"], "loom-software-engineer");
    assert_eq!(v_a["model"], "sonnet");
    assert_eq!(v_a["model_source"], "definition");
    assert_eq!(v_a["description"], description);
    assert!(v_a["ts"].as_str().is_some_and(|s| !s.is_empty()));

    let work_b = temp();
    let tool_input_b = json!({
        "subagent_type": "loom-software-engineer", "model": "sonnet",
        "description": description, "prompt": prompt,
    });
    let out_b = gated_task(&hook, tool_input_b, cwd.path(), home.path(), work_b.path());
    assert_eq!(out_b.code, 0, "stderr={}", out_b.stderr);

    let v_b: Value = serde_json::from_str(&read_spawn_line(work_b.path())).expect("parse line b");
    assert_eq!(v_b["model_source"], "explicit");
}

// 7. A non-spawn tool is ignored entirely (the tool_name switch exits before
//    the gate is ever consulted, so this holds regardless of gating).
#[test]
fn non_spawn_tool_is_ignored() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());

    let out = run_hook(
        &hook,
        "Bash",
        json!({"command": "echo hi"}),
        cwd.path(),
        home.path(),
        &[],
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);
}

// 8. Missing Rule 5 preamble warns for a loom-* agent; loom-codex-forwarder
//    is exempt (it reads AGENTS.md, never CLAUDE.md) - split out purely for
//    size, sharing this file's harness via `use super::*`.
#[path = "hooks_spawn_guard_gate.rs"]
mod gate;
