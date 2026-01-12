//! Hooks configuration types and definitions.
//!
//! Defines the structure for Claude Code hooks that loom uses for
//! session lifecycle management and learning protection.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Claude Code hook event types supported by loom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    /// Called when a Claude Code session starts
    SessionStart,
    /// Called before context compaction (triggers handoff)
    PreCompact,
    /// Called when a session ends normally
    SessionEnd,
    /// Called when session is stopping (validates learnings)
    Stop,
    /// Called when a subagent stops (extracts learnings)
    SubagentStop,
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookEvent::SessionStart => write!(f, "SessionStart"),
            HookEvent::PreCompact => write!(f, "PreCompact"),
            HookEvent::SessionEnd => write!(f, "SessionEnd"),
            HookEvent::Stop => write!(f, "Stop"),
            HookEvent::SubagentStop => write!(f, "SubagentStop"),
        }
    }
}

impl HookEvent {
    /// Get the script filename for this hook event
    pub fn script_name(&self) -> &'static str {
        match self {
            HookEvent::SessionStart => "session-start.sh",
            HookEvent::PreCompact => "pre-compact.sh",
            HookEvent::SessionEnd => "session-end.sh",
            HookEvent::Stop => "stop.sh",
            HookEvent::SubagentStop => "subagent-stop.sh",
        }
    }

    /// Get all hook events
    pub fn all() -> &'static [HookEvent] {
        &[
            HookEvent::SessionStart,
            HookEvent::PreCompact,
            HookEvent::SessionEnd,
            HookEvent::Stop,
            HookEvent::SubagentStop,
        ]
    }
}

/// A single hook definition for Claude Code settings.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    /// The hook command matcher (typically ".*" for all commands)
    pub matcher: String,
    /// The hooks configuration
    pub hooks: HookCommands,
}

/// Hook commands for a matcher
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookCommands {
    /// Commands to run before the matched command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_tool_use: Option<Vec<String>>,
    /// Commands to run after the matched command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_tool_use: Option<Vec<String>>,
}

/// Configuration for loom hooks.
///
/// This structure defines all hooks that loom sets up for Claude Code sessions.
#[derive(Debug, Clone)]
pub struct HooksConfig {
    /// Path to the loom hooks directory
    pub hooks_dir: PathBuf,
    /// Stage ID for this session
    pub stage_id: String,
    /// Session ID for this session
    pub session_id: String,
    /// Path to the .work directory
    pub work_dir: PathBuf,
}

impl HooksConfig {
    /// Create a new hooks configuration
    pub fn new(
        hooks_dir: PathBuf,
        stage_id: String,
        session_id: String,
        work_dir: PathBuf,
    ) -> Self {
        Self {
            hooks_dir,
            stage_id,
            session_id,
            work_dir,
        }
    }

    /// Get the full path to a hook script
    pub fn script_path(&self, event: HookEvent) -> PathBuf {
        self.hooks_dir.join(event.script_name())
    }

    /// Build the command string for a hook event
    ///
    /// The command sets environment variables and calls the hook script:
    /// LOOM_STAGE_ID=<stage> LOOM_SESSION_ID=<session> LOOM_WORK_DIR=<work> <script>
    pub fn build_command(&self, event: HookEvent) -> String {
        let script = self.script_path(event);
        format!(
            "LOOM_STAGE_ID='{}' LOOM_SESSION_ID='{}' LOOM_WORK_DIR='{}' '{}'",
            self.stage_id,
            self.session_id,
            self.work_dir.display(),
            script.display()
        )
    }

    /// Generate the hooks array for Claude Code settings.json
    ///
    /// This creates the hooks configuration in the format expected by Claude Code:
    /// ```json
    /// {
    ///   "hooks": [
    ///     {
    ///       "matcher": ".*",
    ///       "hooks": {
    ///         "preToolUse": [...],
    ///         "postToolUse": [...]
    ///       }
    ///     }
    ///   ]
    /// }
    /// ```
    pub fn to_settings_hooks(&self) -> Vec<HookDefinition> {
        // Create event-specific hook definitions
        let mut hooks = Vec::new();

        // SessionStart hook - runs on any tool use start
        // We use a notification hook that runs once at start
        hooks.push(HookDefinition {
            matcher: "Bash".to_string(),
            hooks: HookCommands {
                pre_tool_use: Some(vec![self.build_command(HookEvent::SessionStart)]),
                post_tool_use: None,
            },
        });

        // PreCompact - runs before context compaction
        // Claude Code calls this before truncating context
        hooks.push(HookDefinition {
            matcher: "PreCompact".to_string(),
            hooks: HookCommands {
                pre_tool_use: Some(vec![self.build_command(HookEvent::PreCompact)]),
                post_tool_use: None,
            },
        });

        // Stop hook - runs when session is stopping
        hooks.push(HookDefinition {
            matcher: "Stop".to_string(),
            hooks: HookCommands {
                pre_tool_use: Some(vec![self.build_command(HookEvent::Stop)]),
                post_tool_use: None,
            },
        });

        // SubagentStop - runs when subagent completes
        hooks.push(HookDefinition {
            matcher: "SubagentStop".to_string(),
            hooks: HookCommands {
                pre_tool_use: None,
                post_tool_use: Some(vec![self.build_command(HookEvent::SubagentStop)]),
            },
        });

        hooks
    }
}
