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
    /// Called after each tool use (heartbeat update)
    PostToolUse,
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
            HookEvent::PostToolUse => write!(f, "PostToolUse"),
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
            HookEvent::PostToolUse => "post-tool-use.sh",
            HookEvent::PreCompact => "pre-compact.sh",
            HookEvent::SessionEnd => "session-end.sh",
            HookEvent::Stop => "learning-validator.sh",
            HookEvent::SubagentStop => "subagent-stop.sh",
        }
    }

    /// Get all hook events
    pub fn all() -> &'static [HookEvent] {
        &[
            HookEvent::SessionStart,
            HookEvent::PostToolUse,
            HookEvent::PreCompact,
            HookEvent::SessionEnd,
            HookEvent::Stop,
            HookEvent::SubagentStop,
        ]
    }
}

/// A single hook rule for Claude Code settings.json (new format)
///
/// New format structure:
/// ```json
/// {
///   "matcher": "Bash",  // String pattern: tool name, "Edit|Write", or "*" for all
///   "hooks": [{"type": "command", "command": "..."}]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRule {
    /// The matcher pattern (e.g., "Bash", "Edit|Write", or "*" for all)
    pub matcher: String,
    /// Array of hook command objects
    pub hooks: Vec<HookCommand>,
}

/// A single hook command in the new format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookCommand {
    /// Type of hook (always "command" for shell commands)
    #[serde(rename = "type")]
    pub hook_type: String,
    /// The shell command to execute
    pub command: String,
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
    /// Returns just the script path. Environment variables (LOOM_STAGE_ID,
    /// LOOM_SESSION_ID, LOOM_WORK_DIR) are set via the `env` section in
    /// settings.json and automatically passed by Claude Code to hooks.
    pub fn build_command(&self, event: HookEvent) -> String {
        let script = self.script_path(event);
        script.display().to_string()
    }

    /// Generate the hooks record for Claude Code settings.json (new format)
    ///
    /// This creates the hooks configuration in the format expected by Claude Code:
    /// ```json
    /// {
    ///   "hooks": {
    ///     "PreToolUse": [
    ///       {"matcher": "Bash", "hooks": [{"type": "command", "command": "..."}]}
    ///     ],
    ///     "PostToolUse": [
    ///       {"matcher": "*", "hooks": [{"type": "command", "command": "..."}]}
    ///     ]
    ///   }
    /// }
    /// ```
    ///
    /// Returns a map of event type to hook rules.
    pub fn to_settings_hooks(&self) -> std::collections::HashMap<String, Vec<HookRule>> {
        use std::collections::HashMap;
        let mut hooks_map: HashMap<String, Vec<HookRule>> = HashMap::new();

        // SessionStart hook - runs on first Bash tool use (PreToolUse event)
        hooks_map
            .entry("PreToolUse".to_string())
            .or_default()
            .push(HookRule {
                matcher: "Bash".to_string(),
                hooks: vec![HookCommand {
                    hook_type: "command".to_string(),
                    command: self.build_command(HookEvent::SessionStart),
                }],
            });

        // PostToolUse hook - runs after any tool use to update heartbeat
        // "*" matcher to catch all tools
        hooks_map
            .entry("PostToolUse".to_string())
            .or_default()
            .push(HookRule {
                matcher: "*".to_string(),
                hooks: vec![HookCommand {
                    hook_type: "command".to_string(),
                    command: self.build_command(HookEvent::PostToolUse),
                }],
            });

        // PreCompact - runs before context compaction
        hooks_map
            .entry("PreCompact".to_string())
            .or_default()
            .push(HookRule {
                matcher: "*".to_string(),
                hooks: vec![HookCommand {
                    hook_type: "command".to_string(),
                    command: self.build_command(HookEvent::PreCompact),
                }],
            });

        // Stop hook - runs when session is stopping
        hooks_map
            .entry("Stop".to_string())
            .or_default()
            .push(HookRule {
                matcher: "*".to_string(),
                hooks: vec![HookCommand {
                    hook_type: "command".to_string(),
                    command: self.build_command(HookEvent::Stop),
                }],
            });

        // SubagentStop - runs when subagent completes
        hooks_map
            .entry("SubagentStop".to_string())
            .or_default()
            .push(HookRule {
                matcher: "*".to_string(),
                hooks: vec![HookCommand {
                    hook_type: "command".to_string(),
                    command: self.build_command(HookEvent::SubagentStop),
                }],
            });

        hooks_map
    }
}
