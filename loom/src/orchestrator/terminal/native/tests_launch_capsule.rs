//! End-to-end `prepare_session_launch` tests for the settings-capsule wiring
//! added by `session_settings.rs`. Split out of `tests_launch.rs` to keep
//! that file under the 400-line ceiling (CLAUDE.md Rule 17), the same reason
//! `tests_capsule.rs` and `tests_wrapper_env.rs` are split out of `tests.rs`.

use super::*;
use crate::fs::work_dir::write_remote_control_config;
use crate::orchestrator::terminal::native::session_settings_path;
use crate::remote_control::{RemoteControlConfig, RemoteControlMode};
use serial_test::serial;
use tempfile::TempDir;

/// A minimal named stage. Duplicated from `tests_launch.rs`'s own
/// `stage_named` rather than shared: that one is private to the sibling
/// `tests` module nested under `launch`, not reachable from here.
fn stage_named(id: &str, name: &str) -> Stage {
    Stage {
        id: id.to_string(),
        name: name.to_string(),
        ..Stage::default()
    }
}

/// Puts a `claude` stub that always exits non-zero at the FRONT of `$PATH`,
/// restoring the original value on drop (including on panic). Mirrors
/// `tmux::tests_spawn::ClaudeOnPathGuard`: `prepare_session_launch` calls
/// `find_claude_path()`, which only needs SOME executable named `claude` to
/// be found — it never runs it — so a stub that fails `--help` is enough to
/// reach the capsule-resolution code under test below without ever executing
/// an unsupervised agent from a unit test.
struct ClaudeStubGuard {
    _dir: TempDir,
    original: Option<std::ffi::OsString>,
}

impl ClaudeStubGuard {
    fn install() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let stub = dir.path().join("claude");
        std::fs::write(&stub, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();

        let original = std::env::var_os("PATH");
        let mut entries = vec![dir.path().to_path_buf()];
        if let Some(path) = original.as_ref() {
            entries.extend(std::env::split_paths(path));
        }
        std::env::set_var("PATH", std::env::join_paths(entries).unwrap());

        Self {
            _dir: dir,
            original,
        }
    }
}

impl Drop for ClaudeStubGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

/// Sets `LOOM_HOOKS_DIR` for the duration of the guard, restoring it on drop.
struct HooksDirGuard {
    original: Option<std::ffi::OsString>,
}

impl HooksDirGuard {
    fn set(dir: &std::path::Path) -> Self {
        let original = std::env::var_os("LOOM_HOOKS_DIR");
        std::env::set_var("LOOM_HOOKS_DIR", dir);
        Self { original }
    }
}

impl Drop for HooksDirGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("LOOM_HOOKS_DIR", value),
            None => std::env::remove_var("LOOM_HOOKS_DIR"),
        }
    }
}

/// A signal file for `prepare_session_launch` to point at.
fn signal_file(work_dir: &std::path::Path) -> PathBuf {
    let path = work_dir.join("signals").join("sig.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "# Assignment\n").unwrap();
    path
}

/// Remote Control off, so `prepare_session_launch`'s `resolve_invocation`
/// call never runs a real `claude --version` preflight against the stub.
fn disable_remote_control(work_dir: &std::path::Path) {
    write_remote_control_config(
        work_dir,
        &RemoteControlConfig {
            mode: RemoteControlMode::Off,
        },
    )
    .unwrap();
}

/// THE WIRING THIS PINS: an adjudication launch must resolve its `--settings`
/// capsule through `session_settings::capsule_for` rather than the plain
/// `resolved_settings_file(cwd)` every other kind uses — see the module doc
/// comment on `session_settings.rs` for why a judge needs a generated capsule
/// at all. This checks the capsule the launch produced ON DISK, not the
/// `--settings` flag on the resulting command line: `native::capsule`'s
/// claude-support probe is memoized in a process-global `OnceLock`
/// (`capsule.rs::probed_capsule_support`), so whether the flag is EMITTED
/// depends on whichever test in the shared test binary happens to probe first
/// — the same reason `tests_capsule.rs` tests `capsule_from` directly instead
/// of `session_capsule`. The capsule file's presence and content do not
/// depend on that probe at all.
#[test]
#[serial]
fn adjudication_launch_writes_a_settings_capsule_with_the_heartbeat_hook() {
    let _claude = ClaudeStubGuard::install();
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    let hooks_dir = temp.path().join("hooks");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&hooks_dir).unwrap();
    let _hooks_guard = HooksDirGuard::set(&hooks_dir);
    disable_remote_control(&work_dir);

    let stage = stage_named("judge-stage", "Judge Stage");
    let session = Session::new_adjudication(&stage.id);
    let session_id = session.id.clone();
    let signal_path = signal_file(&work_dir);

    prepare_session_launch(
        &work_dir,
        SessionType::Adjudication,
        &stage,
        session,
        &signal_path,
        &cwd,
    )
    .expect("an adjudication launch must succeed with a stubbed claude on PATH");

    let capsule_path = session_settings_path(&work_dir, &session_id);
    assert!(
        capsule_path.exists(),
        "the judge must get a generated settings capsule at {}",
        capsule_path.display()
    );
    let content: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&capsule_path).unwrap()).unwrap();
    let post_tool_use = content["hooks"]["PostToolUse"]
        .as_array()
        .expect("PostToolUse must be an array");
    assert!(
        post_tool_use
            .iter()
            .any(|entry| entry["hooks"][0]["command"]
                .as_str()
                .is_some_and(|c| c.ends_with("post-tool-use.sh"))),
        "the capsule must carry the heartbeat hook: {content}"
    );
}

/// The counterpart to the adjudication case above: a stage launch must never
/// write a generated capsule, even when one could be — it keeps resolving
/// `cwd`'s own `.claude/settings.local.json`, exactly as before this module
/// existed.
#[test]
#[serial]
fn stage_launch_never_writes_a_settings_capsule() {
    let _claude = ClaudeStubGuard::install();
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    std::fs::create_dir_all(cwd.join(".claude")).unwrap();
    std::fs::write(cwd.join(".claude").join("settings.local.json"), "{}").unwrap();
    disable_remote_control(&work_dir);

    let stage = stage_named("stage-stage", "Stage Stage");
    let session = Session::new();
    let signal_path = signal_file(&work_dir);

    prepare_session_launch(
        &work_dir,
        SessionType::Stage,
        &stage,
        session,
        &signal_path,
        &cwd,
    )
    .expect("a stage launch must succeed with a stubbed claude on PATH");

    assert!(
        !work_dir.join("capsules").exists(),
        "a stage session must never get a generated settings capsule"
    );
}
