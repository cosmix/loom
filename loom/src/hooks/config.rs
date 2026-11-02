//! Hooks configuration types and definitions.
//!
//! Defines the structure for Claude Code hooks that loom uses for
//! session lifecycle management.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

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

/// The event registrations in a session capsule. The capsule builder adds guard
/// and relay hooks and interpreter wrapping around these rules, and carries no
/// per-session identity: the session wrapper script exports those env vars.
/// Claude Code runs a hook's command through a shell, so each command is one
/// shell word however the hooks directory is spelled.
pub(crate) fn session_event_rules(
    hooks_dir: &Path,
    events: &[HookEvent],
) -> HashMap<String, Vec<HookRule>> {
    let mut hooks_map: HashMap<String, Vec<HookRule>> = HashMap::new();
    for &event in events {
        let script = hooks_dir.join(event.script_name()).display().to_string();
        hooks_map
            .entry(event.to_string())
            .or_default()
            .push(HookRule {
                matcher: "*".to_string(),
                hooks: vec![HookCommand {
                    hook_type: "command".to_string(),
                    command: shell_escape::escape(script.into()).into_owned(),
                }],
            });
    }
    hooks_map
}
