//! Tests for hooks infrastructure

use super::*;
use crate::plan::schema::PermissionMode;
use std::path::PathBuf;
use tempfile::TempDir;

fn test_config(hooks: PathBuf, work: PathBuf) -> HooksConfig {
    HooksConfig::new(hooks, work, PermissionMode::AcceptEdits)
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

    #[test]
    fn test_hooks_config_new() {
        let config = super::test_config(
            PathBuf::from("/path/to/hooks"),
            PathBuf::from("/path/to/.loom/work"),
        );

        assert_eq!(config.hooks_dir, PathBuf::from("/path/to/hooks"));
        assert_eq!(config.work_dir, PathBuf::from("/path/to/.loom/work"));
        assert_eq!(config.permission_mode, PermissionMode::AcceptEdits);
    }

    #[test]
    fn test_hooks_config_script_path() {
        let config = super::test_config(PathBuf::from("/hooks"), PathBuf::from("/work"));

        assert_eq!(
            config.script_path(HookEvent::SessionStart),
            PathBuf::from("/hooks/session-start.sh")
        );
        assert_eq!(
            config.script_path(HookEvent::PreCompact),
            PathBuf::from("/hooks/pre-compact.sh")
        );
    }

    #[test]
    fn test_hooks_config_build_command() {
        let config = super::test_config(PathBuf::from("/hooks"), PathBuf::from("/work"));

        let cmd = config.build_command(HookEvent::SessionStart);
        assert_eq!(cmd, "/hooks/session-start.sh");

        let cmd = config.build_command(HookEvent::PostToolUse);
        assert_eq!(cmd, "/hooks/post-tool-use.sh");
    }

    #[test]
    fn test_hooks_config_to_settings_hooks() {
        let config = super::test_config(PathBuf::from("/hooks"), PathBuf::from("/work"));

        let hooks = config.to_settings_hooks();
        assert_eq!(hooks.len(), HookEvent::all().len());

        for &event in HookEvent::all() {
            let rules = hooks
                .get(&event.to_string())
                .unwrap_or_else(|| panic!("missing hook rules for {event}"));
            assert!(!rules.is_empty(), "{event} has no hook rules");
            assert_eq!(rules[0].matcher, "*");
            assert!(rules[0].hooks[0].command.ends_with(event.script_name()));
        }
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

    #[test]
    fn test_find_hooks_dir_env_var() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_dir = temp_dir.path().join("hooks");
        std::fs::create_dir(&hooks_dir).unwrap();

        std::env::set_var("LOOM_HOOKS_DIR", hooks_dir.to_str().unwrap());
        let found = generator::find_hooks_dir();
        std::env::remove_var("LOOM_HOOKS_DIR");

        assert!(found.is_some());
        assert_eq!(found.unwrap(), hooks_dir);
    }
}
