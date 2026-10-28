//! `launch`: the sandbox files and `launch.sh` it writes, the claude command line that script
//! runs (through a fake `claude`), and real tmux launches that skip where tmux cannot run.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde_json::Value;
use serial_test::serial;
use tempfile::TempDir;

use super::*;
use crate::orchestrator::terminal::tmux::socket::tests::TmuxServerGuard;
use crate::orchestrator::terminal::tmux::socket_path_for;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::tmux_io::tests::{real_tmux, wait_until};
use crate::planning::runtime::{create_runtime_dir, runtime_paths_under};
use crate::process::{verify_process_identity, IdentityStatus};

/// Text that must never reach `launch.sh` or claude's argv (px-security-04).
const OPENING: &str = "OPENING-c41d quoting a document";

/// A fake `claude` that records its argv (NUL-separated), cwd and environment in `OUT`.
const RECORDING_CLAUDE: &str = "printf '%s\\0' \"$@\" > 'OUT/argv'
{
  printf 'cwd=%s\\n' \"$(pwd -P)\"
  printf 'session=%s\\n' \"${LOOM_PLAN_SESSION-}\"
  printf 'state=%s\\n' \"${LOOM_PLAN_STATE-}\"
  printf 'extra=%s\\n' \"${LOOM_PLAN_TEST_EXTRA-}\"
  printf 'stage=%s%s%s\\n' \"${LOOM_STAGE_ID+id}\" \"${LOOM_SESSION_ID+session}\" \"${LOOM_WORK_DIR+work}\"
} > 'OUT/env'
";

struct Fixture {
    _temp: TempDir,
    base: PathBuf,
    spec: LaunchSpec,
}

/// A project, a runtime directory, a session directory and a fake `claude` running `body`.
fn fixture(display_name: &str, body: &str) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("project");
    fs::create_dir_all(root.join("doc/plans")).unwrap();
    fs::write(root.join("README.md"), "# Fixture\n").unwrap();
    let home = base.join("home");
    fs::create_dir(&home).unwrap();
    let runtime = runtime_paths_under(&base.join("loomhome"), &root).unwrap();
    create_runtime_dir(&runtime).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let dir = SessionDir::new(runtime.sessions.join(&id));
    fs::create_dir_all(dir.answers()).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let claude = write_claude(&base, body);
    let guidance = LaunchGuidance {
        brief_path: dir.brief(),
        plugin_dir: dir.plugin(),
        opening_message: OPENING.to_owned(),
        model: "opus".to_owned(),
        effort: "medium".to_owned(),
        display_name: display_name.to_owned(),
        opening_on_argv: false,
    };
    let hex: String = id.chars().filter(|c| *c != '-').take(12).collect();
    let spec = LaunchSpec {
        socket: format!("loom-plan-{hex}"),
        id,
        kind: SessionKind::Authoring,
        dir,
        tmux_session: "plan-authoring".to_owned(),
        guidance,
        extra_env: vec![("LOOM_PLAN_TEST_EXTRA".to_owned(), "extra value".to_owned())],
        opening_argv: None,
        project_root: root,
        runtime,
        loom_bin: PathBuf::from("/opt/loom"),
        claude_bin: claude,
        home,
    };
    Fixture {
        _temp: temp,
        base,
        spec,
    }
}

/// `base/claude`: a shell script running `body` with `OUT` replaced by `base`.
fn write_claude(base: &std::path::Path, body: &str) -> PathBuf {
    let claude = base.join("claude");
    let script = format!("#!/bin/sh\n{}", body.replace("OUT", base.to_str().unwrap()));
    fs::write(&claude, script).unwrap();
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    claude
}

/// The exact argv the launch line must give claude: every flag once, no positional prompt,
/// no `--resume` or `--continue`.
fn expected_argv(spec: &LaunchSpec) -> Vec<String> {
    let path = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let words = [
        "--session-id".to_owned(),
        spec.id.clone(),
        "--name".to_owned(),
        spec.guidance.display_name.clone(),
        "--settings".to_owned(),
        path(&spec.dir.settings()),
        "--setting-sources".to_owned(),
        "user,project".to_owned(),
        "--model".to_owned(),
        "opus".to_owned(),
        "--effort".to_owned(),
        "medium".to_owned(),
        "--permission-mode".to_owned(),
        "default".to_owned(),
        "--plugin-dir".to_owned(),
        path(&spec.dir.plugin()),
        "--strict-mcp-config".to_owned(),
        "--append-system-prompt-file".to_owned(),
        path(&spec.dir.brief()),
    ];
    Vec::from(words)
}

/// Run `launch.sh` with `sh` from `/` under a stage identity it must drop; returns claude's
/// argv and the recorded environment.
fn run_script(f: &Fixture, script: &std::path::Path) -> (Vec<String>, String) {
    let status = Command::new("sh")
        .arg(script)
        .current_dir("/")
        .env("LOOM_STAGE_ID", "bridge-runtime")
        .env("LOOM_SESSION_ID", "session-parent")
        .env("LOOM_WORK_DIR", "/nonexistent/work")
        .status()
        .unwrap();
    assert!(status.success(), "launch.sh failed: {status}");
    let raw = fs::read(f.base.join("argv")).unwrap();
    let raw = raw.strip_suffix(&[0]).unwrap_or(&raw);
    let argv = raw.split(|b| *b == 0);
    let argv = argv
        .map(|a| String::from_utf8(a.to_vec()).unwrap())
        .collect();
    (argv, fs::read_to_string(f.base.join("env")).unwrap())
}

#[test]
fn launch_sh_runs_claude_with_the_planning_flags_from_the_project_root() {
    let names = [
        "loom plan: it's \"quoted\"",
        "$(touch pwned) `touch pwned` $HOME ${PATH}",
        "first line\nsecond line",
        "-leading dash; touch pwned",
    ];
    for name in names {
        let f = fixture(name, RECORDING_CLAUDE);
        let script = prepare(&f.spec).unwrap();
        let syntax = Command::new("sh").arg("-n").arg(&script).status().unwrap();
        assert!(syntax.success(), "sh -n refused launch.sh for {name:?}");
        let (argv, env) = run_script(&f, &script);
        assert_eq!(argv, expected_argv(&f.spec), "name {name:?}");
        for dir in [&f.base, &f.spec.project_root] {
            assert!(!dir.join("pwned").exists(), "{name:?} ran a substitution");
        }
        let expected = format!(
            "cwd={}\nsession={}\nstate={}\nextra=extra value\nstage=\n",
            f.spec.project_root.display(),
            f.spec.id,
            f.spec.runtime.dir.display()
        );
        assert_eq!(env, expected, "planning variables or a stage identity");
        let text = fs::read_to_string(&script).unwrap();
        assert!(
            !text.contains("OPENING-c41d"),
            "the opening is in launch.sh"
        );
        assert!(!argv.iter().any(|a| a.contains("OPENING-c41d")), "on argv");
    }
}

#[test]
fn launch_sh_is_private_and_settings_register_the_spool_hooks() {
    let f = fixture("loom plan: hooks", RECORDING_CLAUDE);
    let script = prepare(&f.spec).unwrap();
    assert_eq!(script, f.spec.dir.launch_script());
    let mode = fs::metadata(&script).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "launch.sh mode {mode:o}");

    let settings = fs::read_to_string(f.spec.dir.settings()).unwrap();
    let settings: Value = serde_json::from_str(&settings).unwrap();
    let hooks = &settings["hooks"];
    let guard = &hooks["PreToolUse"][0];
    assert_eq!(guard["hooks"][0]["timeout"], 10, "plan 6's guard: {guard}");
    for spec in SPOOL_EVENTS {
        let entries = hooks[spec.event].as_array();
        let entries = entries.unwrap_or_else(|| panic!("{} not registered", spec.event));
        assert_eq!(entries.len(), 1, "{}", spec.event);
        let hook = &entries[0]["hooks"][0];
        let command = hook["command"].as_str().unwrap_or_default();
        let call = format!("plan hook {} --session-dir", spec.event);
        assert!(command.contains(&call), "{}: {command}", spec.event);
        let timeout = spec.timeout_secs.map_or(Value::Null, Value::from);
        assert_eq!(hook["timeout"], timeout, "{}", spec.event);
    }
    let permission = &hooks["PermissionRequest"][0];
    assert_eq!(permission["hooks"][0]["timeout"], 3600);
    assert_eq!(permission["matcher"], "*");
}

#[test]
#[serial]
fn launch_starts_claude_in_its_own_server_and_identifies_it() {
    let test = "launch_starts_claude_in_its_own_server_and_identifies_it";
    let Some(_tmux) = real_tmux(test) else {
        return;
    };
    let body = format!("{RECORDING_CLAUDE}exec sleep 30\n");
    let f = fixture("loom plan: real tmux", &body);
    let _server = TmuxServerGuard::new(&f.spec.socket);

    let spawned = launch(&f.spec).unwrap();
    assert_eq!(
        verify_process_identity(spawned.identity),
        IdentityStatus::VerifiedAlive
    );
    let env = f.base.join("env");
    let recorded = || fs::read_to_string(&env).is_ok_and(|text| text.contains("stage="));
    assert!(
        wait_until(Duration::from_secs(10), recorded),
        "claude never ran"
    );
    let cwd = format!("cwd={}\n", f.spec.project_root.display());
    assert!(fs::read_to_string(&env).unwrap().starts_with(&cwd));

    teardown_socket(&f.spec.socket).unwrap();
    assert!(!socket_path_for(&f.spec.socket).exists(), "socket survived");
}

#[test]
#[serial]
fn a_launch_whose_claude_exits_at_once_is_torn_down_and_reported() {
    let test = "a_launch_whose_claude_exits_at_once_is_torn_down_and_reported";
    let Some(_tmux) = real_tmux(test) else {
        return;
    };
    let f = fixture("loom plan: exits", "exit 3\n");
    let _server = TmuxServerGuard::new(&f.spec.socket);

    // The pane's process is gone before `spawn_in_tmux`'s probe and option calls return, so
    // the identity read sees a dead pane (`remain-on-exit on`) or no start time.
    let error = launch(&f.spec).expect_err("a claude that exits at once must fail the launch");
    let message = format!("{error:#}");
    assert!(
        message.contains("already exited") || message.contains("start time"),
        "the failure must come from the identity read: {message}"
    );
    assert!(
        !message.contains("also failed"),
        "teardown failed: {message}"
    );
    assert!(
        !socket_path_for(&f.spec.socket).exists(),
        "the half-started server's socket survived"
    );
}
