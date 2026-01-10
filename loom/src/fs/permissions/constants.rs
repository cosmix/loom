//! Permission constants for loom

/// Embedded loom stop hook script
/// This script enforces commit and stage completion in loom worktrees
pub const LOOM_STOP_HOOK: &str = include_str!("../../../resources/hooks/loom-stop.sh");

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
    // Tmux for session management
    "Bash(tmux:*)",
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
    // Tmux for session management
    "Bash(tmux:*)",
];
