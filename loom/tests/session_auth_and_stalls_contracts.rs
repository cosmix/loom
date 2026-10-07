//! Frozen contracts for stage session-auth-and-stalls: the agent environment
//! keeps the login identity, the auth probe reads a logged-out status as
//! logged out, and approving a parked stage clears its stall counter.

#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;

use std::ffi::OsString;
use std::process::Command;

use loom::claude::auth::{parse_auth_status, AuthProbe};
use loom::fs::work_dir::WorkDir;
use loom::models::stage::{Stage, StageStatus};
use loom::process::{
    agent_session_environment_from, apply_stage_environment_from, AGENT_SESSION_ENV_NAMES,
};
use loom::verify::transitions::{load_stage, save_stage};
use tempfile::TempDir;

/// `claude auth status --json` from claude 2.1.291 with an empty HOME (exit 1).
const LOGGED_OUT_JSON: &str = r#"{"loggedIn": false, "authMethod": "none", "apiProvider": "firstParty", "analyticsDisabled": false, "projectsDirectory": "/home/u/.claude/projects", "configDirectory": "/home/u/.claude"}"#;

fn pair(key: &str, value: &str) -> (OsString, OsString) {
    (OsString::from(key), OsString::from(value))
}

#[test]
fn agent_environment_keeps_user_and_logname() {
    assert!(AGENT_SESSION_ENV_NAMES.contains(&"USER"));
    assert!(AGENT_SESSION_ENV_NAMES.contains(&"LOGNAME"));

    let source = [
        ("HOME", "/home/alice"),
        ("PATH", "/usr/local/bin:/usr/bin:/bin"),
        ("USER", "alice"),
        ("LOGNAME", "alice"),
        ("GITHUB_TOKEN", "canary"),
        ("ANTHROPIC_API_KEY", "canary"),
    ];
    let environment = agent_session_environment_from(source);

    assert!(environment.contains(&pair("HOME", "/home/alice")));
    assert!(environment.contains(&pair("PATH", "/usr/local/bin:/usr/bin:/bin")));
    assert!(environment.contains(&pair("USER", "alice")));
    assert!(environment.contains(&pair("LOGNAME", "alice")));
    for (key, value) in &environment {
        assert_ne!(value, "canary", "{key:?} leaked an ambient secret");
        assert_ne!(key, "GITHUB_TOKEN");
        assert_ne!(key, "ANTHROPIC_API_KEY");
    }
}

#[test]
fn stage_host_layer_keeps_user() {
    let source = [
        ("PATH", "/usr/bin:/bin"),
        ("HOME", "/home/alice"),
        ("USER", "alice"),
        ("LOGNAME", "alice"),
        ("GITHUB_TOKEN", "canary"),
    ];
    let mut command = Command::new("env");
    apply_stage_environment_from(&mut command, source);

    let output = command.output().expect("the system env tool should run");
    assert!(output.status.success(), "env exited {}", output.status);
    let stdout = String::from_utf8(output.stdout).expect("env prints UTF-8 here");
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(lines.contains(&"USER=alice"), "env printed: {stdout}");
    assert!(lines.contains(&"LOGNAME=alice"), "env printed: {stdout}");
    assert!(!stdout.contains("canary"), "env printed: {stdout}");
}

#[test]
fn logged_out_status_is_not_logged_in() {
    assert_eq!(
        parse_auth_status(LOGGED_OUT_JSON, false),
        AuthProbe::NotLoggedIn
    );
}

#[test]
fn approve_resets_stall_recoveries() {
    let tmp = TempDir::new().expect("create temp project");
    let wd = WorkDir::new(tmp.path()).expect("resolve work dir");
    wd.initialize().expect("initialize work dir");
    let stage = Stage {
        id: "parked".to_string(),
        name: "Parked stage".to_string(),
        status: StageStatus::NeedsHumanReview,
        stall_recoveries: 2,
        ..Stage::default()
    };
    save_stage(&stage, wd.root()).expect("save stage");

    let output = helpers::loom_cmd()
        .current_dir(tmp.path())
        .args(["stage", "human-review", "parked", "--approve"])
        .output()
        .expect("spawn loom");
    assert!(
        output.status.success(),
        "approve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let approved = load_stage("parked", wd.root()).expect("load stage");
    assert_eq!(approved.status, StageStatus::Queued);
    assert_eq!(approved.stall_recoveries, 0);
}
