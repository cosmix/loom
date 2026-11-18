//! Runs a `launch.sh` produced by `launch_script` with `sh` against a fake `claude` that writes
//! its argv (NUL-separated) and the planning and stage variables it received.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use loom::plan_session::bridge::host::LaunchSpec;
use loom::plan_session::bridge::LaunchGuidance;
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::plan_session::launch::script::launch_script;
use loom::planning::runtime::{create_runtime_dir, runtime_paths_under};

/// What one run of the launch script left behind.
pub struct LaunchRun {
    _tmp: tempfile::TempDir,
    pub tmp: PathBuf,
    pub root: PathBuf,
    pub id: String,
    pub state: PathBuf,
    pub script: String,
    pub argv: Vec<String>,
    /// `session=<LOOM_PLAN_SESSION>\nstate=<LOOM_PLAN_STATE>\nstage=<which stage variables were set>\n`
    pub env: String,
}

const FAKE_CLAUDE: &str = "#!/bin/sh
printf '%s\\0' \"$@\" > 'OUT/argv'
{
  printf 'session=%s\\n' \"${LOOM_PLAN_SESSION-}\"
  printf 'state=%s\\n' \"${LOOM_PLAN_STATE-}\"
  printf 'stage=%s%s%s\\n' \"${LOOM_STAGE_ID+id}\" \"${LOOM_SESSION_ID+session}\" \"${LOOM_WORK_DIR+work}\"
} > 'OUT/env'
";

/// The environment handed to `launch_script`, with a stage identity it must drop.
pub fn parent_env() -> Vec<(OsString, OsString)> {
    let pairs = [
        ("PATH", "/usr/bin:/bin"),
        ("HOME", "/nonexistent/home"),
        ("LOOM_STAGE_ID", "bridge-runtime"),
        ("LOOM_SESSION_ID", "session-583cc737"),
        ("LOOM_WORK_DIR", "/nonexistent/work"),
    ];
    pairs
        .iter()
        .map(|(k, v)| (OsString::from(k), OsString::from(v)))
        .collect()
}

pub fn run_launch_script(
    display_name: &str,
    opening: &str,
    env: &[(OsString, OsString)],
) -> LaunchRun {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().to_path_buf();
    let spec = spec(&base, display_name, opening);
    let script = launch_script(&spec, &spec.dir.settings(), env);
    let script_path = base.join("launch.sh");
    fs::write(&script_path, &script).unwrap();
    let status = Command::new("sh")
        .arg(&script_path)
        .current_dir(&base)
        .env("LOOM_STAGE_ID", "bridge-runtime")
        .env("LOOM_SESSION_ID", "session-parent")
        .env("LOOM_WORK_DIR", "/nonexistent/work")
        .status()
        .unwrap();
    assert!(status.success(), "launch.sh failed: {status}\n{script}");
    let argv = fs::read(base.join("argv")).unwrap();
    let argv = argv.split(|b| *b == 0).filter(|a| !a.is_empty());
    let argv = argv
        .map(|a| String::from_utf8(a.to_vec()).unwrap())
        .collect();
    let env = fs::read_to_string(base.join("env")).unwrap();
    LaunchRun {
        _tmp: tmp,
        tmp: base,
        root: spec.project_root,
        id: spec.id,
        state: spec.runtime.dir,
        script,
        argv,
        env,
    }
}

/// A project, a runtime directory, a session directory and the fake `claude` under `base`.
fn spec(base: &Path, display_name: &str, opening: &str) -> LaunchSpec {
    let root = base.join("project");
    fs::create_dir_all(&root).unwrap();
    let runtime = runtime_paths_under(&base.join("loom-home"), &root).unwrap();
    create_runtime_dir(&runtime).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let dir = SessionDir::new(runtime.sessions.join(&id));
    fs::create_dir_all(dir.path()).unwrap();
    let claude = base.join("claude");
    fs::write(&claude, FAKE_CLAUDE.replace("OUT", base.to_str().unwrap())).unwrap();
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    let guidance = LaunchGuidance {
        brief_path: dir.brief(),
        plugin_dir: dir.plugin(),
        opening_message: opening.to_owned(),
        model: "opus".to_owned(),
        effort: "medium".to_owned(),
        display_name: display_name.to_owned(),
        opening_on_argv: false,
    };
    LaunchSpec {
        socket: format!("loom-plan-{}", &id.replace('-', "")[..12]),
        id,
        kind: SessionKind::Authoring,
        dir,
        tmux_session: "plan-authoring".to_owned(),
        guidance,
        extra_env: Vec::new(),
        opening_argv: None,
        project_root: root,
        runtime,
        loom_bin: PathBuf::from("/nonexistent/loom"),
        claude_bin: claude,
        home: base.join("home"),
    }
}
