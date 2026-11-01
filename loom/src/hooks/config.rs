//! Hooks configuration types and definitions.
//!
//! Defines the structure for Claude Code hooks that loom uses for
//! session lifecycle management.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

use crate::plan::schema::PermissionMode;

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
    /// Called when session is stopping
    Stop,
    /// Called when a Task-tool subagent starts (spawn-type ledger)
    SubagentStart,
    /// Called when a Task-tool subagent finishes (completion signal + heartbeat refresh)
    SubagentStop,
    /// Called when an agent-team teammate becomes idle (nonterminal lifecycle evidence)
    TeammateIdle,
}

impl fmt::Display for HookEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookEvent::SessionStart => write!(f, "SessionStart"),
            HookEvent::PostToolUse => write!(f, "PostToolUse"),
            HookEvent::PreCompact => write!(f, "PreCompact"),
            HookEvent::SessionEnd => write!(f, "SessionEnd"),
            HookEvent::Stop => write!(f, "Stop"),
            HookEvent::SubagentStart => write!(f, "SubagentStart"),
            HookEvent::SubagentStop => write!(f, "SubagentStop"),
            HookEvent::TeammateIdle => write!(f, "TeammateIdle"),
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
            HookEvent::SubagentStart => "subagent-start.sh",
            HookEvent::SubagentStop => "subagent-stop.sh",
            HookEvent::TeammateIdle => "teammate-idle.sh",
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
            HookEvent::SubagentStart,
            HookEvent::SubagentStop,
            HookEvent::TeammateIdle,
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

/// Legacy configuration retained for library compatibility.
///
/// Production session capsules use their own emitter, which includes guard
/// registrations and interpreter handling absent from this structure.
///
/// Deliberately carries NO per-session identity (stage ID, session ID): those
/// env vars are exported by the session wrapper script only, never persisted
/// in settings files, so they can never go stale across sessions.
#[derive(Debug, Clone)]
#[deprecated(note = "legacy emitter differs from production session capsules")]
pub struct HooksConfig {
    /// Path to the loom hooks directory
    pub hooks_dir: PathBuf,
    /// Path to the .loom/work directory
    pub work_dir: PathBuf,
    /// Resolved Claude Code permission mode for this session.
    pub permission_mode: PermissionMode,
}

#[allow(deprecated)]
impl HooksConfig {
    /// Create a new hooks configuration.
    pub fn new(hooks_dir: PathBuf, work_dir: PathBuf, permission_mode: PermissionMode) -> Self {
        Self {
            hooks_dir,
            work_dir,
            permission_mode,
        }
    }

    /// Get the full path to a hook script.
    ///
    /// Sessions see host-absolute paths — the hooks are installed at
    /// `<claude_dir>/hooks/loom/...` on the host (`assets::install::claude_hooks_dir`).
    pub fn script_path(&self, event: HookEvent) -> PathBuf {
        self.hooks_dir.join(event.script_name())
    }

    /// Build the legacy unwrapped script path for an event.
    ///
    /// Production capsules may wrap this path with an interpreter.
    pub fn build_command(&self, event: HookEvent) -> String {
        let script = self.script_path(event);
        script.display().to_string()
    }

    /// Build the single hook rule for `event`: a `"*"` matcher running
    /// `self.build_command(event)` as the sole command.
    fn hook_rule(&self, event: HookEvent) -> HookRule {
        HookRule {
            matcher: "*".to_string(),
            hooks: vec![HookCommand {
                hook_type: "command".to_string(),
                command: self.build_command(event),
            }],
        }
    }

    /// Produce the legacy session-hook map for compatibility callers.
    ///
    /// This map omits production guard, relay and interpreter registrations;
    /// session capsules are built by `orchestrator::terminal::native::session_settings`.
    pub fn to_settings_hooks(&self) -> std::collections::HashMap<String, Vec<HookRule>> {
        use std::collections::HashMap;
        let mut hooks_map: HashMap<String, Vec<HookRule>> = HashMap::new();

        for &event in HookEvent::all() {
            hooks_map
                .entry(event.to_string())
                .or_default()
                .push(self.hook_rule(event));
        }

        hooks_map
    }
}
