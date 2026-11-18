//! Forwarders own their prompt protocol and do not receive the generic preamble.
use super::*;

#[test]
fn codex_bound_types_never_receive_the_preamble() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());
    for subagent_type in ["loom-codex-forwarder", "codex:codex-rescue"] {
        let input = json!({"subagent_type": subagent_type, "model": "sonnet", "prompt": "task"});
        let out = run_hook(&hook, "Agent", input, cwd.path(), home.path(), &[]);
        assert_eq!(out.code, 0, "stderr={}", out.stderr);
        assert!(
            !out.stdout.contains("updatedInput") && !out.stdout.contains("LOOM_HOOK_WARN"),
            "{subagent_type}: stdout={}",
            out.stdout
        );
    }
}

const NATIVE_START: &str = "loom subagents start --provider codex --unit-id <unit> --brief <absolute brief path> --model <tier> --effort xhigh";

/// A `loom` on PATH whose `hook codex-backend` prints `$CODEX_BACKEND`, or
/// fails when it is `__FAIL__`.
fn install_backend_stub(root: &Path) -> std::path::PathBuf {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("create stub bin");
    let stub = bin.join("loom");
    fs::write(
        &stub,
        "#!/usr/bin/env bash\n[[ \"$1 $2\" == \"hook codex-backend\" ]] || exit 0\n\
         [[ \"$CODEX_BACKEND\" != __FAIL__ ]] || exit 1\nprintf '%s\\n' \"$CODEX_BACKEND\"\n",
    )
    .expect("write loom stub");
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).expect("chmod loom stub");
    bin
}

fn backend_task(hook: &Path, subagent_type: &str, backend: &str, gated: bool) -> HookOutput {
    let (home, cwd, work) = (temp(), temp(), temp());
    let bin = install_backend_stub(work.path());
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let pid = std::process::id().to_string();
    let work_str = work.path().to_string_lossy().into_owned();
    let mut env = vec![
        ("LOOM_STAGE_ID", "test-stage"),
        ("LOOM_WORK_DIR", work_str.as_str()),
        ("LOOM_SESSION_ID", "test-session"),
        ("CODEX_BACKEND", backend),
        ("PATH", path.as_str()),
    ];
    if gated {
        env.push(("LOOM_MAIN_AGENT_PID", pid.as_str()));
    }
    let input = json!({"subagent_type": subagent_type, "model": "sonnet", "prompt": "task"});
    run_hook(hook, "Agent", input, cwd.path(), home.path(), &env)
}

#[test]
fn a_native_backend_denies_forwarder_and_plugin_spawns() {
    if skip_unless_gate_visible("a_native_backend_denies_forwarder_and_plugin_spawns") {
        return;
    }
    let (_temp, hook) = setup_hook();
    for subagent_type in ["loom-codex-forwarder", "codex:codex-rescue"] {
        let out = backend_task(&hook, subagent_type, "native", true);
        assert_eq!(out.code, 2, "{subagent_type}: stdout={}", out.stdout);
        assert!(
            out.stderr.contains(NATIVE_START),
            "{subagent_type}: {}",
            out.stderr
        );
        assert!(out.stderr.contains(subagent_type), "{}", out.stderr);
    }
}

#[test]
fn a_native_backend_still_allows_claude_workers() {
    if skip_unless_gate_visible("a_native_backend_still_allows_claude_workers") {
        return;
    }
    let (_temp, hook) = setup_hook();
    let out = backend_task(&hook, "loom-software-engineer", "native", true);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
}

#[test]
fn legacy_unknown_or_ungated_backends_keep_forwarder_spawns() {
    let (_temp, hook) = setup_hook();
    for (backend, gated) in [
        ("legacy", true),
        ("__FAIL__", true),
        ("", true),
        ("native", false),
    ] {
        let out = backend_task(&hook, "loom-codex-forwarder", backend, gated);
        assert_eq!(
            out.code, 0,
            "backend={backend:?} gated={gated}: stderr={}",
            out.stderr
        );
        assert!(!out.stderr.contains(NATIVE_START), "{}", out.stderr);
    }
}
