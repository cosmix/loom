//! Tests for hooks infrastructure

use super::*;
use tempfile::TempDir;

/// Historical main-settings fixture used to test cleanup, not a session emitter.
pub(crate) fn session_hook_fixture(hooks_dir: &std::path::Path) -> serde_json::Value {
    serde_json::to_value(super::session_event_rules(hooks_dir, HookEvent::all()))
        .expect("session hook rules are serializable")
}

mod config_tests {
    use super::*;

    #[test]
    fn test_hook_event_display() {
        let names: Vec<_> = HookEvent::all().iter().map(ToString::to_string).collect();
        assert_eq!(
            names.join(","),
            "SessionStart,PostToolUse,PreCompact,SessionEnd,Stop,SubagentStart,SubagentStop,TeammateIdle"
        );
    }

    #[test]
    fn test_hook_event_script_name() {
        let scripts: Vec<_> = HookEvent::all()
            .iter()
            .map(HookEvent::script_name)
            .collect();
        assert_eq!(
            scripts.join(","),
            "session-start.sh,post-tool-use.sh,pre-compact.sh,session-end.sh,learning-validator.sh,subagent-start.sh,subagent-stop.sh,teammate-idle.sh"
        );
    }

    #[test]
    fn test_hook_event_all() {
        assert_eq!(
            HookEvent::all(),
            &[
                HookEvent::SessionStart,
                HookEvent::PostToolUse,
                HookEvent::PreCompact,
                HookEvent::SessionEnd,
                HookEvent::Stop,
                HookEvent::SubagentStart,
                HookEvent::SubagentStop,
                HookEvent::TeammateIdle,
            ]
        );
    }

    /// One `*` rule per event, running that event's script unquoted when the
    /// hooks directory needs no quoting.
    #[test]
    fn session_event_rules_register_each_events_script() {
        let hooks = session_event_rules(std::path::Path::new("/hooks"), HookEvent::all());
        assert_eq!(hooks.len(), HookEvent::all().len());

        for &event in HookEvent::all() {
            let rules = &hooks[&event.to_string()];
            assert_eq!(rules.len(), 1, "{event}");
            assert_eq!(rules[0].matcher, "*", "{event}");
            assert_eq!(rules[0].hooks.len(), 1, "{event}");
            assert_eq!(rules[0].hooks[0].hook_type, "command", "{event}");
            assert_eq!(
                rules[0].hooks[0].command,
                format!("/hooks/{}", event.script_name())
            );
        }
    }

    /// Claude Code hands a hook's command to a shell, so a home directory with
    /// a space must still name one script.
    #[test]
    fn session_event_commands_run_from_a_hooks_dir_with_a_space() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join("home with space/it's hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let marker = dir.path().join("ran");
        let script = hooks_dir.join(HookEvent::Stop.script_name());
        std::fs::write(&script, format!("#!/bin/sh\n: >'{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let rules = session_event_rules(&hooks_dir, &[HookEvent::Stop]);
        let command = &rules["Stop"][0].hooks[0].command;
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .status()
            .unwrap();
        assert!(status.success(), "{command}");
        assert!(marker.exists(), "{command} did not run the script");
    }
}

mod events_tests {
    use super::*;

    #[test]
    fn test_hook_event_log_new() {
        let event = HookEventLog::new("stage-1", "session-abc", HookEvent::SessionStart);
        assert_eq!(event.stage_id, "stage-1");
        assert_eq!(event.session_id, "session-abc");
        assert_eq!(event.event, "SessionStart");
        assert!(event.payload.is_none());
    }

    #[test]
    fn test_hook_event_log_with_payload() {
        let payload = HookEventPayload::PreCompact {
            context_tokens: Some(75_000),
            transcript_path: Some(".loom/work/transcripts/stage-1.jsonl".to_string()),
            handoff_file: Some("stage-1-handoff-001.md".to_string()),
        };
        let event =
            HookEventLog::with_payload("stage-1", "session-abc", HookEvent::PreCompact, payload);
        assert_eq!(event.event, "PreCompact");
        assert!(event.payload.is_some());

        if let Some(HookEventPayload::PreCompact {
            context_tokens,
            transcript_path,
            handoff_file,
        }) = &event.payload
        {
            assert_eq!(*context_tokens, Some(75_000));
            assert_eq!(
                transcript_path.as_deref(),
                Some(".loom/work/transcripts/stage-1.jsonl")
            );
            assert_eq!(*handoff_file, Some("stage-1-handoff-001.md".to_string()));
        } else {
            panic!("Expected PreCompact payload");
        }
    }

    #[test]
    fn test_hook_event_log_to_json_line() {
        let event = HookEventLog::new("stage-1", "session-abc", HookEvent::Stop);
        let json = event.to_json_line().unwrap();
        assert!(json.contains("\"stage_id\":\"stage-1\""));
        assert!(json.contains("\"session_id\":\"session-abc\""));
        assert!(json.contains("\"event\":\"Stop\""));
        // Should be a single line
        assert!(!json.contains('\n'));
    }

    #[test]
    fn test_log_and_read_events() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        // Log some events
        let event1 = HookEventLog::new("stage-1", "session-1", HookEvent::SessionStart);
        let event2 = HookEventLog::new("stage-1", "session-1", HookEvent::PreCompact);
        let event3 = HookEventLog::new("stage-2", "session-2", HookEvent::Stop);

        log_hook_event(work_dir, event1).unwrap();
        log_hook_event(work_dir, event2).unwrap();
        log_hook_event(work_dir, event3).unwrap();

        // Read all events
        let events = events::read_recent_events(work_dir, None).unwrap();
        assert_eq!(events.len(), 3);

        // Read with limit
        let events = events::read_recent_events(work_dir, Some(2)).unwrap();
        assert_eq!(events.len(), 2);

        // Read session events
        let session_events = events::read_session_events(work_dir, "session-1").unwrap();
        assert_eq!(session_events.len(), 2);

        // Read stage events
        let stage_events = events::read_stage_events(work_dir, "stage-2").unwrap();
        assert_eq!(stage_events.len(), 1);
    }

    #[test]
    fn test_read_events_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let events = events::read_recent_events(temp_dir.path(), None).unwrap();
        assert!(events.is_empty());
    }
}

mod generator_tests {
    use super::*;
    use crate::assets::install::test_env::InstallRootsEnv;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_find_hooks_dir_env_var() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_dir = temp_dir.path().join("hooks");
        std::fs::create_dir(&hooks_dir).unwrap();

        let mut env = InstallRootsEnv::isolate(temp_dir.path());
        env.set("LOOM_HOOKS_DIR", &hooks_dir);
        let found = generator::find_hooks_dir();

        assert_eq!(found, Some(hooks_dir));
    }

    #[test]
    #[serial]
    fn find_hooks_dir_defaults_to_the_home_claude_root() {
        let temp = TempDir::new().unwrap();
        // Canonical, because the resolver canonicalizes (macOS `/var`).
        let home = temp.path().canonicalize().unwrap();
        let default = home.join(".claude/hooks/loom");
        std::fs::create_dir_all(&default).unwrap();

        let _env = InstallRootsEnv::isolate(&home);
        assert_eq!(generator::find_hooks_dir(), Some(default));
    }
}
