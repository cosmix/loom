//! Tests for hooks infrastructure

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

mod config_tests {
    use super::*;

    #[test]
    fn test_hook_event_display() {
        assert_eq!(HookEvent::SessionStart.to_string(), "SessionStart");
        assert_eq!(HookEvent::PostToolUse.to_string(), "PostToolUse");
        assert_eq!(HookEvent::PreCompact.to_string(), "PreCompact");
        assert_eq!(HookEvent::SessionEnd.to_string(), "SessionEnd");
        assert_eq!(HookEvent::Stop.to_string(), "Stop");
        assert_eq!(HookEvent::SubagentStop.to_string(), "SubagentStop");
        assert_eq!(
            HookEvent::PreferModernTools.to_string(),
            "PreferModernTools"
        );
    }

    #[test]
    fn test_hook_event_script_name() {
        assert_eq!(HookEvent::SessionStart.script_name(), "session-start.sh");
        assert_eq!(HookEvent::PostToolUse.script_name(), "post-tool-use.sh");
        assert_eq!(HookEvent::PreCompact.script_name(), "pre-compact.sh");
        assert_eq!(HookEvent::SessionEnd.script_name(), "session-end.sh");
        assert_eq!(HookEvent::Stop.script_name(), "learning-validator.sh");
        assert_eq!(HookEvent::SubagentStop.script_name(), "subagent-stop.sh");
        assert_eq!(
            HookEvent::PreferModernTools.script_name(),
            "prefer-modern-tools.sh"
        );
    }

    #[test]
    fn test_hook_event_all() {
        let all = HookEvent::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&HookEvent::SessionStart));
        assert!(all.contains(&HookEvent::PostToolUse));
        assert!(all.contains(&HookEvent::PreCompact));
        assert!(all.contains(&HookEvent::SessionEnd));
        assert!(all.contains(&HookEvent::Stop));
        assert!(all.contains(&HookEvent::SubagentStop));
        assert!(all.contains(&HookEvent::PreferModernTools));
    }

    #[test]
    fn test_hooks_config_new() {
        let config = HooksConfig::new(
            PathBuf::from("/path/to/hooks"),
            "my-stage".to_string(),
            "session-123".to_string(),
            PathBuf::from("/path/to/.work"),
        );

        assert_eq!(config.hooks_dir, PathBuf::from("/path/to/hooks"));
        assert_eq!(config.stage_id, "my-stage");
        assert_eq!(config.session_id, "session-123");
        assert_eq!(config.work_dir, PathBuf::from("/path/to/.work"));
    }

    #[test]
    fn test_hooks_config_script_path() {
        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "stage".to_string(),
            "session".to_string(),
            PathBuf::from("/work"),
        );

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
        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "test-stage".to_string(),
            "test-session".to_string(),
            PathBuf::from("/work"),
        );

        // build_command now returns just the script path
        // Environment variables are set via env section in settings.json
        let cmd = config.build_command(HookEvent::SessionStart);
        assert_eq!(cmd, "/hooks/session-start.sh");

        let cmd = config.build_command(HookEvent::PostToolUse);
        assert_eq!(cmd, "/hooks/post-tool-use.sh");
    }

    #[test]
    fn test_hooks_config_to_settings_hooks() {
        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "stage".to_string(),
            "session".to_string(),
            PathBuf::from("/work"),
        );

        let hooks = config.to_settings_hooks();
        // Should have hook events: SessionStart, PostToolUse, PreCompact, SessionEnd, Stop, SubagentStop
        assert!(hooks.len() >= 6);

        // Check PreCompact hook exists
        assert!(hooks.contains_key("PreCompact"));
        let pre_compact_rules = &hooks["PreCompact"];
        assert!(!pre_compact_rules.is_empty());
        assert_eq!(pre_compact_rules[0].matcher, "*");

        // Check Stop hook exists
        assert!(hooks.contains_key("Stop"));
        let stop_rules = &hooks["Stop"];
        assert!(!stop_rules.is_empty());

        // Check PostToolUse has * matcher
        assert!(hooks.contains_key("PostToolUse"));
        let post_tool_rules = &hooks["PostToolUse"];
        assert!(post_tool_rules.iter().any(|r| r.matcher == "*"));
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
            context_percent: Some(75.5),
            handoff_file: Some("stage-1-handoff-001.md".to_string()),
        };
        let event =
            HookEventLog::with_payload("stage-1", "session-abc", HookEvent::PreCompact, payload);

        assert_eq!(event.event, "PreCompact");
        assert!(event.payload.is_some());

        if let Some(HookEventPayload::PreCompact {
            context_percent,
            handoff_file,
        }) = &event.payload
        {
            assert_eq!(*context_percent, Some(75.5));
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
    use serde_json::json;

    #[test]
    fn test_generate_hooks_settings_new() {
        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "stage".to_string(),
            "session".to_string(),
            PathBuf::from("/work"),
        );

        let settings = generate_hooks_settings(&config, None).unwrap();

        // Check trust dialog
        assert_eq!(settings["hasTrustDialogAccepted"], json!(true));

        // Check permissions
        assert_eq!(settings["permissions"]["defaultMode"], json!("acceptEdits"));

        // Check hooks is a record (object) not an array
        assert!(settings["hooks"].is_object());
        assert!(settings["hooks"]["SessionStart"].is_array());
        assert!(settings["hooks"]["PostToolUse"].is_array());
        assert!(settings["hooks"]["PreCompact"].is_array());
        assert!(settings["hooks"]["SessionEnd"].is_array());
        assert!(settings["hooks"]["Stop"].is_array());
        assert!(settings["hooks"]["SubagentStop"].is_array());

        // Check environment variables
        assert_eq!(settings["env"]["LOOM_STAGE_ID"], json!("stage"));
        assert_eq!(settings["env"]["LOOM_SESSION_ID"], json!("session"));
    }

    #[test]
    fn test_generate_hooks_settings_merge_existing() {
        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "stage".to_string(),
            "session".to_string(),
            PathBuf::from("/work"),
        );

        let existing = json!({
            "someCustomSetting": true,
            "permissions": {
                "allowedTools": ["Bash", "Read"]
            }
        });

        let settings = generate_hooks_settings(&config, Some(&existing)).unwrap();

        // Check custom setting preserved
        assert_eq!(settings["someCustomSetting"], json!(true));

        // Check permissions merged
        assert_eq!(settings["permissions"]["defaultMode"], json!("acceptEdits"));
        assert_eq!(
            settings["permissions"]["allowedTools"],
            json!(["Bash", "Read"])
        );
    }

    #[test]
    fn test_setup_hooks_for_worktree() {
        let temp_dir = TempDir::new().unwrap();
        let worktree_path = temp_dir.path();

        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "test-stage".to_string(),
            "test-session".to_string(),
            PathBuf::from("/work"),
        );

        setup_hooks_for_worktree(worktree_path, &config).unwrap();

        // Check .claude directory created
        let claude_dir = worktree_path.join(".claude");
        assert!(claude_dir.exists());

        // Check settings.json created
        let settings_path = claude_dir.join("settings.json");
        assert!(settings_path.exists());

        // Parse and validate settings
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let settings: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(settings["env"]["LOOM_STAGE_ID"], json!("test-stage"));
        assert!(settings["hooks"].is_object());
        assert!(settings["hooks"]["PostToolUse"].is_array());
    }

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

    #[test]
    fn test_generate_hooks_merges_with_global_hooks() {
        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "stage".to_string(),
            "session".to_string(),
            PathBuf::from("/work"),
        );

        // Existing settings with global hooks
        let existing = json!({
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "AskUserQuestion",
                        "hooks": [{"type": "command", "command": "/global/ask-user-pre.sh"}]
                    }
                ],
                "PostToolUse": [
                    {
                        "matcher": "AskUserQuestion",
                        "hooks": [{"type": "command", "command": "/global/ask-user-post.sh"}]
                    }
                ],
                "Stop": [
                    {
                        "matcher": "*",
                        "hooks": [{"type": "command", "command": "/global/commit-guard.sh"}]
                    }
                ]
            }
        });

        let settings = generate_hooks_settings(&config, Some(&existing)).unwrap();

        // Check that global hooks are preserved in PreToolUse
        let pre_tool_hooks = settings["hooks"]["PreToolUse"].as_array().unwrap();
        assert!(pre_tool_hooks.iter().any(|h| {
            h["matcher"] == "AskUserQuestion"
                && h["hooks"][0]["command"] == "/global/ask-user-pre.sh"
        }), "Global ask-user-pre hook should be preserved");

        // Check that session hooks are added (SessionStart uses PostToolUse, not PreToolUse)
        let session_start_hooks = settings["hooks"]["SessionStart"].as_array().unwrap();
        assert!(session_start_hooks.iter().any(|h| {
            h["hooks"][0]["command"].as_str().unwrap().contains("session-start.sh")
        }), "Session start hook should be added");

        // Check Stop event has both global and session hooks
        let stop_hooks = settings["hooks"]["Stop"].as_array().unwrap();
        assert!(stop_hooks.iter().any(|h| {
            h["hooks"][0]["command"] == "/global/commit-guard.sh"
        }), "Global commit-guard hook should be preserved");
        assert!(stop_hooks.iter().any(|h| {
            h["hooks"][0]["command"].as_str().unwrap().contains("learning-validator.sh")
        }), "Session stop hook should be added");

        // Verify we have both hooks on Stop event
        assert!(stop_hooks.len() >= 2, "Stop event should have at least 2 hooks (commit-guard + stop hook)");
    }

    #[test]
    fn test_generate_hooks_no_duplication() {
        let config = HooksConfig::new(
            PathBuf::from("/hooks"),
            "stage".to_string(),
            "session".to_string(),
            PathBuf::from("/work"),
        );

        // First generation
        let settings1 = generate_hooks_settings(&config, None).unwrap();

        // Second generation with first as input (simulating re-run)
        let settings2 = generate_hooks_settings(&config, Some(&settings1)).unwrap();

        // Hooks should not be duplicated
        let hooks1 = settings1["hooks"]["PostToolUse"].as_array().unwrap();
        let hooks2 = settings2["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(hooks1.len(), hooks2.len(), "Re-running should not duplicate hooks");

        let stop_hooks1 = settings1["hooks"]["Stop"].as_array().unwrap();
        let stop_hooks2 = settings2["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop_hooks1.len(), stop_hooks2.len(), "Re-running should not duplicate Stop hooks");
    }
}
