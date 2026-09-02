//! Unit tests for `native/session_settings.rs`, declared as a sibling module
//! the way `tests_capsule.rs` and `tests_wrapper_env.rs` are (CLAUDE.md
//! Rule 17 keeps test files split out of the module they cover).

use super::*;
use crate::orchestrator::terminal::native::session_settings::{
    resolve_settings_file, with_post_tool_use_hook, write_session_settings,
};
use serde_json::json;
use serial_test::serial;
use std::path::Path;
use tempfile::TempDir;

/// Sets `LOOM_HOOKS_DIR` for the duration of the guard and restores it on
/// drop (including on panic), the same discipline `hooks/tests.rs`'s env-var
/// test uses for `find_hooks_dir`'s override.
struct HooksDirGuard {
    original: Option<std::ffi::OsString>,
}

impl HooksDirGuard {
    fn set(dir: &Path) -> Self {
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

fn post_tool_use_commands(settings: &serde_json::Value) -> Vec<String> {
    settings["hooks"]["PostToolUse"]
        .as_array()
        .expect("PostToolUse must be an array")
        .iter()
        .filter_map(|entry| entry["hooks"][0]["command"].as_str().map(String::from))
        .collect()
}

#[test]
fn with_post_tool_use_hook_adds_it_when_there_is_no_base() {
    let hooks_dir = Path::new("/home/user/.claude/hooks/loom");
    let settings = with_post_tool_use_hook(None, hooks_dir);

    let commands = post_tool_use_commands(&settings);
    assert_eq!(commands.len(), 1);
    assert_eq!(
        commands[0],
        hooks_dir.join("post-tool-use.sh").to_string_lossy()
    );
}

#[test]
fn with_post_tool_use_hook_does_not_duplicate_an_existing_entry() {
    let hooks_dir = Path::new("/home/user/.claude/hooks/loom");
    let base = json!({
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "*",
                    "hooks": [{"type": "command", "command": "/home/user/.claude/hooks/loom/post-tool-use.sh"}]
                },
                {
                    "matcher": "Bash",
                    "hooks": [{"type": "command", "command": "/home/user/.claude/hooks/loom/loom-control-complete.sh"}]
                }
            ]
        }
    });

    let settings = with_post_tool_use_hook(Some(&base), hooks_dir);

    let commands = post_tool_use_commands(&settings);
    assert_eq!(
        commands.len(),
        2,
        "an already-registered heartbeat hook must not be duplicated: {commands:?}"
    );
    assert!(
        commands
            .iter()
            .any(|c| c.ends_with("loom-control-complete.sh")),
        "the base's other PostToolUse entries must survive: {commands:?}"
    );
}

#[test]
fn with_post_tool_use_hook_scrubs_session_identity_env() {
    let hooks_dir = Path::new("/home/user/.claude/hooks/loom");
    let base = json!({
        "env": {
            "LOOM_SESSION_ID": "old-session",
            "LOOM_STAGE_ID": "old-stage",
            "SOME_OTHER_VAR": "keep-me",
        }
    });

    let settings = with_post_tool_use_hook(Some(&base), hooks_dir);

    let env = settings["env"].as_object().unwrap();
    assert!(!env.contains_key("LOOM_SESSION_ID"));
    assert!(!env.contains_key("LOOM_STAGE_ID"));
    assert_eq!(env["SOME_OTHER_VAR"], json!("keep-me"));
}

#[test]
fn write_session_settings_writes_under_capsules_dir_and_overwrites_cleanly() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    let hooks_dir = temp.path().join("hooks");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&hooks_dir).unwrap();

    let path = write_session_settings(&work_dir, "session-abc123", &cwd, &hooks_dir).unwrap();

    assert_eq!(
        path,
        work_dir
            .join("capsules")
            .join("session-abc123.settings.json")
    );
    assert!(path.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "capsules/ must be private, not umask-default"
        );
    }

    let content: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(post_tool_use_commands(&content).len(), 1);

    // A second write for the same session must overwrite cleanly, not error
    // or leave a stray `.tmp` sibling behind.
    let path2 = write_session_settings(&work_dir, "session-abc123", &cwd, &hooks_dir).unwrap();
    assert_eq!(path2, path);
    assert!(path2.exists());
}

#[test]
fn write_session_settings_layers_onto_cwds_existing_settings_local_json() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    let hooks_dir = temp.path().join("hooks");
    std::fs::create_dir_all(cwd.join(".claude")).unwrap();
    std::fs::create_dir_all(&hooks_dir).unwrap();
    std::fs::write(
        cwd.join(".claude").join("settings.local.json"),
        json!({"hasTrustDialogAccepted": true}).to_string(),
    )
    .unwrap();

    let path = write_session_settings(&work_dir, "session-def456", &cwd, &hooks_dir).unwrap();

    let content: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(content["hasTrustDialogAccepted"], json!(true));
    assert_eq!(post_tool_use_commands(&content).len(), 1);
}

#[test]
fn write_session_settings_rejects_an_invalid_session_id() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    let hooks_dir = temp.path().join("hooks");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&hooks_dir).unwrap();

    let result = write_session_settings(&work_dir, "../etc/passwd", &cwd, &hooks_dir);
    assert!(result.is_err());
    assert!(
        !work_dir.join("capsules").exists(),
        "a rejected session id must not create the capsules directory"
    );
}

#[test]
fn cleanup_session_settings_removes_the_file_and_tolerates_a_missing_one() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    let hooks_dir = temp.path().join("hooks");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&hooks_dir).unwrap();
    let path = write_session_settings(&work_dir, "session-cleanup1", &cwd, &hooks_dir).unwrap();
    assert!(path.exists());

    cleanup_session_settings(&work_dir, "session-cleanup1");
    assert!(!path.exists());

    // Idempotent: a second cleanup against the now-missing file must not panic
    // or error, since the daemon's own close path is best-effort.
    cleanup_session_settings(&work_dir, "session-cleanup1");
}

#[test]
#[serial]
fn resolve_settings_file_for_adjudication_writes_and_points_at_a_generated_capsule() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    let hooks_dir = temp.path().join("hooks");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&hooks_dir).unwrap();
    let _hooks_guard = HooksDirGuard::set(&hooks_dir);

    let settings_file = resolve_settings_file(
        SessionType::Adjudication,
        &cwd,
        &work_dir,
        "session-resolve1",
    )
    .unwrap();

    let resolved = settings_file.expect("adjudication must resolve a generated settings file");
    let expected = work_dir
        .join("capsules")
        .join("session-resolve1.settings.json");
    assert_eq!(Path::new(&resolved), expected);
    assert!(expected.exists());
}

#[test]
fn resolve_settings_file_for_other_kinds_never_writes_a_capsule() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join("work");
    let cwd = temp.path().join("repo");
    std::fs::create_dir_all(cwd.join(".claude")).unwrap();
    std::fs::write(cwd.join(".claude").join("settings.local.json"), "{}").unwrap();

    let settings_file =
        resolve_settings_file(SessionType::Stage, &cwd, &work_dir, "session-resolve2").unwrap();

    let resolved = settings_file.expect("cwd has a settings.local.json to resolve");
    assert_eq!(
        Path::new(&resolved).canonicalize().unwrap(),
        cwd.join(".claude")
            .join("settings.local.json")
            .canonicalize()
            .unwrap()
    );
    assert!(
        !work_dir.join("capsules").exists(),
        "non-adjudication kinds must never write a generated settings capsule"
    );
}
