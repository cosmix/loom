//! Gated warning and worker-brief cases for `hooks_spawn_guard.rs`.

use super::*;

const NONCE: &str = "0123456789abcdef0123456789abcdef";
const BRIEF: &str = "## Scoped worker brief\n\n<!-- loom-worker-brief nonce=0123456789abcdef0123456789abcdef -->\n\nScoped fact only.";

fn install_worker_brief_stub(root: &Path) -> std::path::PathBuf {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("create stub bin");
    let stub = bin.join("loom");
    fs::write(
        &stub,
        r#"#!/usr/bin/env bash
payload=$(cat)
printf '%s' "$payload" >"$LOOM_STDIN"
printf '%s\n' "$*" >>"$LOOM_CALLS"
[[ "$1" == "hook" && "$2" == "worker-brief" ]] || exit 0
[[ "$WORKER_BRIEF_RESPONSE" != "__FAIL__" ]] || exit 1
printf '%s\n' "$WORKER_BRIEF_RESPONSE"
"#,
    )
    .expect("write loom stub");
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod loom stub");
    bin
}

fn brief_response() -> String {
    json!({"nonce": NONCE, "brief": BRIEF}).to_string()
}

fn run_with_worker_brief(
    hook: &Path,
    tool_input: Value,
    cwd: &Path,
    home: &Path,
    work: &Path,
    stub_bin: &Path,
    response: &str,
) -> HookOutput {
    let pid = std::process::id().to_string();
    let path = format!("{}:/usr/bin:/bin", stub_bin.display());
    let work = work.to_string_lossy().into_owned();
    let calls = stub_bin.join("calls").to_string_lossy().into_owned();
    let stdin = stub_bin.join("stdin").to_string_lossy().into_owned();
    run_hook(
        hook,
        "Task",
        tool_input,
        cwd,
        home,
        &[
            ("LOOM_STAGE_ID", "test-stage"),
            ("LOOM_MAIN_AGENT_PID", pid.as_str()),
            ("LOOM_WORK_DIR", work.as_str()),
            ("LOOM_SESSION_ID", "test-session"),
            ("LOOM_CALLS", calls.as_str()),
            ("LOOM_STDIN", stdin.as_str()),
            ("WORKER_BRIEF_RESPONSE", response),
            ("PATH", path.as_str()),
        ],
    )
}

fn assert_typed_spawn_updated_input(value: &Value, expected_prompt: &str) {
    assert_eq!(value.as_object().map(|v| v.len()), Some(1));
    assert_eq!(
        get_str(value, &["hookSpecificOutput", "permissionDecision"]),
        Some("allow")
    );
    assert_eq!(
        get_str(value, &["hookSpecificOutput", "updatedInput", "model"]),
        Some("sonnet")
    );
    assert_eq!(
        get_str(value, &["hookSpecificOutput", "updatedInput", "prompt"]),
        Some(expected_prompt)
    );
}

#[test]
fn missing_rule5_preamble_warns_except_for_codex_forwarder() {
    if skip_unless_gate_visible("gate::missing_rule5_preamble_warns_except_for_codex_forwarder") {
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
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);
}

#[test]
fn explore_without_definition_resolves_builtin_sonnet() {
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
    let value: Value = serde_json::from_str(out.stdout.trim()).expect("parse hook output");
    assert_eq!(
        get_str(&value, &["hookSpecificOutput", "updatedInput", "model"]),
        Some("sonnet")
    );
}

#[test]
fn typed_spawn_combines_brief_and_model_in_one_updated_input() {
    let (fixture, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let stub_bin = install_worker_brief_stub(fixture.path());
    let prompt = format!("{PREAMBLE_LINE}\npreserve quotes: ' \" and unicode λ\n");
    let tool_input = json!({
        "subagent_type": "loom-software-engineer",
        "description": "scoped task",
        "prompt": prompt,
    });

    let out = run_with_worker_brief(
        &hook,
        tool_input.clone(),
        cwd.path(),
        home.path(),
        work.path(),
        &stub_bin,
        &brief_response(),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert_eq!(out.stdout.lines().count(), 1, "stdout={}", out.stdout);
    let value: Value = serde_json::from_str(out.stdout.trim()).expect("parse hook output");
    let expected_prompt = format!("{prompt}\n\n{BRIEF}");
    assert_typed_spawn_updated_input(&value, &expected_prompt);
    assert_eq!(
        fs::read_to_string(stub_bin.join("calls")).expect("read calls"),
        "hook worker-brief\n"
    );
    let forwarded: Value = serde_json::from_str(
        &fs::read_to_string(stub_bin.join("stdin")).expect("read forwarded stdin"),
    )
    .expect("parse forwarded stdin");
    assert_eq!(
        forwarded,
        json!({"tool_name": "Task", "tool_input": tool_input})
    );
}

#[test]
fn brief_without_model_rewrite_updates_only_the_prompt() {
    let (fixture, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    let stub_bin = install_worker_brief_stub(fixture.path());
    let prompt = "preserve this prompt exactly";
    let tool_input = json!({"subagent_type": "Explore", "model": "sonnet", "prompt": prompt});

    let out = run_with_worker_brief(
        &hook,
        tool_input,
        cwd.path(),
        home.path(),
        work.path(),
        &stub_bin,
        &brief_response(),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert_eq!(out.stdout.lines().count(), 1, "stdout={}", out.stdout);
    let value: Value = serde_json::from_str(out.stdout.trim()).expect("parse hook output");
    let expected_prompt = format!("{prompt}\n\n{BRIEF}");
    assert_eq!(
        get_str(&value, &["hookSpecificOutput", "permissionDecision"]),
        None
    );
    assert_eq!(
        get_str(&value, &["hookSpecificOutput", "updatedInput", "prompt"]),
        Some(expected_prompt.as_str())
    );
}

#[test]
fn typed_spawn_without_definition_or_model_receives_brief_without_model_key() {
    if skip_unless_gate_visible(
        "gate::typed_spawn_without_definition_or_model_receives_brief_without_model_key",
    ) {
        return;
    }
    let (fixture, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    let stub_bin = install_worker_brief_stub(fixture.path());
    let prompt = "preserve this unknown-worker prompt";

    let out = run_with_worker_brief(
        &hook,
        json!({"subagent_type": "unknown-worker", "prompt": prompt}),
        cwd.path(),
        home.path(),
        work.path(),
        &stub_bin,
        &brief_response(),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let value: Value = serde_json::from_str(out.stdout.trim()).expect("parse hook output");
    let updated = &value["hookSpecificOutput"]["updatedInput"];
    assert_eq!(updated["prompt"], format!("{prompt}\n\n{BRIEF}"));
    assert!(updated.get("model").is_none(), "updated={updated}");
    assert_eq!(
        fs::read_to_string(stub_bin.join("calls")).expect("read calls"),
        "hook worker-brief\n"
    );
}

#[test]
fn brief_rewrite_keeps_existing_warning_context() {
    let (fixture, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let stub_bin = install_worker_brief_stub(fixture.path());
    let prompt = format!("{PREAMBLE_LINE}\nescalated task");
    let tool_input = json!({
        "subagent_type": "loom-software-engineer",
        "model": "opus",
        "prompt": prompt,
    });

    let out = run_with_worker_brief(
        &hook,
        tool_input,
        cwd.path(),
        home.path(),
        work.path(),
        &stub_bin,
        &brief_response(),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let value: Value = serde_json::from_str(out.stdout.trim()).expect("parse hook output");
    let context =
        get_str(&value, &["hookSpecificOutput", "additionalContext"]).expect("warning context");
    assert!(context.contains("LOOM_HOOK_WARN:") && context.contains("escalation"));
    let expected_prompt = format!("{prompt}\n\n{BRIEF}");
    assert_eq!(
        get_str(&value, &["hookSpecificOutput", "updatedInput", "prompt"]),
        Some(expected_prompt.as_str())
    );
}

#[test]
fn empty_brief_envelope_preserves_existing_output() {
    let (fixture, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());
    write_agent_def(cwd.path(), "loom-software-engineer", "sonnet");
    let stub_bin = install_worker_brief_stub(fixture.path());
    let prompt = format!("{PREAMBLE_LINE}\nno scoped selection");
    let tool_input = json!({"subagent_type": "loom-software-engineer", "prompt": prompt});
    let empty_work = temp();
    let failed_work = temp();

    let empty = run_with_worker_brief(
        &hook,
        tool_input.clone(),
        cwd.path(),
        home.path(),
        empty_work.path(),
        &stub_bin,
        "{}",
    );
    let failed = run_with_worker_brief(
        &hook,
        tool_input,
        cwd.path(),
        home.path(),
        failed_work.path(),
        &stub_bin,
        "__FAIL__",
    );

    assert_eq!(empty.code, 0, "stderr={}", empty.stderr);
    assert_eq!(empty.stdout, failed.stdout);
    let value: Value = serde_json::from_str(empty.stdout.trim()).expect("parse hook output");
    assert_eq!(
        get_str(&value, &["hookSpecificOutput", "updatedInput", "prompt"]),
        Some(prompt.as_str())
    );
}

#[test]
fn untyped_denied_spawn_never_invokes_worker_brief() {
    if skip_unless_gate_visible("gate::untyped_denied_spawn_never_invokes_worker_brief") {
        return;
    }
    let (fixture, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    let stub_bin = install_worker_brief_stub(fixture.path());

    let out = run_with_worker_brief(
        &hook,
        json!({"prompt": "untyped"}),
        cwd.path(),
        home.path(),
        work.path(),
        &stub_bin,
        &brief_response(),
    );

    assert_eq!(out.code, 2, "stderr={}", out.stderr);
    assert!(!stub_bin.join("calls").exists());
}

#[test]
fn codex_forwarder_keeps_marker_first_without_navigation_kit() {
    let (fixture, hook) = setup_hook();
    let (home, cwd, work) = (temp(), temp(), temp());
    let stub_bin = install_worker_brief_stub(fixture.path());
    let prompt = "LOOM-CODEX-FORWARD-ONLY\n--model gpt-5.6-sol --effort xhigh\ntask text";
    let tool_input = json!({
        "subagent_type": "loom-codex-forwarder",
        "model": "sonnet",
        "prompt": prompt,
    });

    let out = run_with_worker_brief(
        &hook,
        tool_input,
        cwd.path(),
        home.path(),
        work.path(),
        &stub_bin,
        &brief_response(),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let value: Value = serde_json::from_str(out.stdout.trim()).expect("parse hook output");
    let updated =
        get_str(&value, &["hookSpecificOutput", "updatedInput", "prompt"]).expect("updated prompt");
    assert_eq!(updated.lines().next(), Some("LOOM-CODEX-FORWARD-ONLY"));
    assert!(!updated.contains("=== LOOM CONTEXT"));
    assert!(!updated.contains("NAVIGATE WITH THE SOURCE GRAPH"));
    assert!(
        get_str(&value, &["hookSpecificOutput", "additionalContext"]).is_none(),
        "stdout={}",
        out.stdout
    );
}
