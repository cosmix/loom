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
///
/// Deliberately carries NO per-session identity (stage ID, session ID): those
/// env vars are exported by the session wrapper script only, never persisted
/// in settings files, so they can never go stale across sessions.
#[derive(Debug, Clone)]
pub struct HooksConfig {
    /// Path to the loom hooks directory
    pub hooks_dir: PathBuf,
    /// Path to the .loom/work directory
    pub work_dir: PathBuf,
    /// Resolved Claude Code permission mode for this session.
    pub permission_mode: PermissionMode,
}

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
    /// `~/.claude/hooks/loom/...` on the host.
    pub fn script_path(&self, event: HookEvent) -> PathBuf {
        self.hooks_dir.join(event.script_name())
    }

    /// Build the command string for a hook event
    ///
    /// Returns just the script path. Hooks read LOOM_STAGE_ID / LOOM_SESSION_ID /
    /// LOOM_WORK_DIR from the process environment, which the session wrapper
    /// script exports before `exec claude` (settings files only carry the
    /// stable LOOM_WORK_DIR — see `generate_hooks_settings`).
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

    /// Generate session-specific hooks for Claude Code settings.json (new format)
    ///
    /// This creates ONLY session-specific hooks that should be added to worktree settings.
    /// Global hooks (ask-user-pre, ask-user-post, prefer-modern-tools, commit-guard, skill-trigger)
    /// are already in the main settings.json and should NOT be duplicated here.
    ///
    /// Every `HookEvent` variant returned by `HookEvent::all()` is a session
    /// hook and belongs in this map — there is currently no variant that must
    /// be excluded. What each one does, and why:
    /// - SessionStart: writes the initial heartbeat.
    /// - PostToolUse: updates the heartbeat after each tool use ("*" matcher
    ///   catches every tool).
    /// - PreCompact: triggers a handoff before context compaction.
    /// - SessionEnd: runs cleanup when a session ends normally.
    /// - Stop: runs learning-validator.sh. commit-guard.sh is a separate,
    ///   global hook already present in the main settings.json, so it must
    ///   not be duplicated here.
    /// - SubagentStart: records a Task-tool subagent's spawn type in the ledger.
    /// - SubagentStop: runs in the PARENT session's own hook context when a
    ///   Task-tool subagent finishes. Writes a completion record and
    ///   refreshes the parent's heartbeat, since the parent runs no tools of
    ///   its own while blocked on the subagent, so PostToolUse cannot do that.
    ///
    /// Returns a map of event type to hook rules.
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
