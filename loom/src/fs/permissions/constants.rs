//! Permission constants for loom

/// Commit guard hook - enforces commit and stage completion in loom worktrees
/// Runs as a global Stop hook, blocks exit if uncommitted changes or stage incomplete
pub const HOOK_COMMIT_GUARD: &str = include_str!("../../../../hooks/commit-guard.sh");

// Embedded hook scripts for loom worktree sessions
// These are installed to ~/.claude/hooks/loom/ for use by Claude Code

/// PostToolUse hook - updates heartbeat after each tool use
pub const HOOK_POST_TOOL_USE: &str = include_str!("../../../../hooks/post-tool-use.sh");

/// SessionStart hook - initializes heartbeat when session starts
pub const HOOK_SESSION_START: &str = include_str!("../../../../hooks/session-start.sh");

/// PreCompact hook - triggers handoff before context compaction
pub const HOOK_PRE_COMPACT: &str = include_str!("../../../../hooks/pre-compact.sh");

/// SessionEnd hook - handles session completion
pub const HOOK_SESSION_END: &str = include_str!("../../../../hooks/session-end.sh");

/// Learning validator hook - validates learning files weren't damaged during session
pub const HOOK_LEARNING_VALIDATOR: &str = include_str!("../../../../hooks/learning-validator.sh");

/// SubagentStop hook - extracts learnings from subagents
pub const HOOK_SUBAGENT_STOP: &str = include_str!("../../../../hooks/subagent-stop.sh");

/// AskUserQuestion pre hook - marks stage as waiting for input
pub const HOOK_ASK_USER_PRE: &str = include_str!("../../../../hooks/ask-user-pre.sh");

/// AskUserQuestion post hook - resumes stage after user input
pub const HOOK_ASK_USER_POST: &str = include_str!("../../../../hooks/ask-user-post.sh");

/// All worktree hook scripts with their filenames (installed to ~/.claude/hooks/loom/)
pub const LOOM_WORKTREE_HOOKS: &[(&str, &str)] = &[
    ("post-tool-use.sh", HOOK_POST_TOOL_USE),
    ("session-start.sh", HOOK_SESSION_START),
    ("pre-compact.sh", HOOK_PRE_COMPACT),
    ("session-end.sh", HOOK_SESSION_END),
    ("learning-validator.sh", HOOK_LEARNING_VALIDATOR),
    ("subagent-stop.sh", HOOK_SUBAGENT_STOP),
];

/// Global hook scripts with their filenames (installed to ~/.claude/hooks/)
pub const LOOM_GLOBAL_HOOKS: &[(&str, &str)] = &[
    ("commit-guard.sh", HOOK_COMMIT_GUARD),
    ("ask-user-pre.sh", HOOK_ASK_USER_PRE),
    ("ask-user-post.sh", HOOK_ASK_USER_POST),
];

/// Loom permissions for the MAIN REPO context
/// Includes worktree permissions so settings.local.json can be symlinked to worktrees
/// and all sessions share the same permission file (approvals propagate)
pub const LOOM_PERMISSIONS: &[&str] = &[
    // Read/write access via symlink path (for worktree sessions via symlink)
    "Read(.work/**)",
    "Write(.work/**)",
    // Read/write access via parent traversal (for worktree sessions via direct path)
    "Read(../../.work/**)",
    "Write(../../.work/**)",
    // Read access to CLAUDE.md files (subagents need to read these explicitly)
    "Read(.claude/**)",
    "Read(~/.claude/**)",
    // Loom CLI commands (use :* for prefix matching)
    "Bash(loom:*)",
];

/// Loom permissions for WORKTREE context
/// Includes both .work/** (symlink path as seen by Claude) and ../../.work/** (parent traversal)
/// The symlink at .worktrees/stage-X/.work -> ../../.work means Claude sees paths as .work/**
/// but the actual files are accessed via parent traversal
pub const LOOM_PERMISSIONS_WORKTREE: &[&str] = &[
    // Read/write access via symlink path (how Claude sees and requests the paths)
    "Read(.work/**)",
    "Write(.work/**)",
    // Read/write access via parent traversal (alternative direct access pattern)
    "Read(../../.work/**)",
    "Write(../../.work/**)",
    // Read access to CLAUDE.md files (subagents need to read these explicitly)
    "Read(.claude/**)",
    "Read(~/.claude/**)",
    // Loom CLI commands (use :* for prefix matching)
    "Bash(loom:*)",
];
