//! Command, subcommand, flag, and static value completions.

use anyhow::Result;

/// All top-level loom commands.
const TOP_LEVEL_COMMANDS: &[&str] = &[
    "check",
    "clean",
    "completions",
    "container",
    "diagnose",
    "graph",
    "handoff",
    "init",
    "knowledge",
    "map",
    "memory",
    "plan",
    "repair",
    "resume",
    "review",
    "run",
    "self-update",
    "sessions",
    "skill-index",
    "stage",
    "status",
    "stop",
    "worktree",
];

/// Complete top-level command names.
pub fn complete_commands(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(TOP_LEVEL_COMMANDS, prefix))
}

/// Complete subcommands for a parent command.
pub fn complete_subcommands(parent: &str, prefix: &str) -> Result<Vec<String>> {
    let subs: &[&str] = match parent {
        "stage" => &[
            "block",
            "complete",
            "dispute-criteria",
            "hold",
            "human-review",
            "merge",
            "output",
            "release",
            "reset",
            "resume",
            "retry",
            "skip",
            "verify",
            "waiting",
        ],
        "sessions" => &["kill", "list"],
        "worktree" => &["list", "remove"],
        "container" => &["build", "doctor", "list", "logs", "rebuild", "shell"],
        "knowledge" => &[
            "audit",
            "bootstrap",
            "check",
            "gc",
            "init",
            "list",
            "show",
            "update",
        ],
        "memory" => &[
            "change", "decision", "list", "note", "query", "question", "show",
        ],
        "output" => &["get", "list", "remove", "set"],
        "plan" => &["verify"],
        _ => return Ok(Vec::new()),
    };
    Ok(filter_prefix(subs, prefix))
}

/// Complete flags for a given command path.
///
/// `command_path` is a slice of command words, e.g. `["stage", "complete"]`.
pub fn complete_flags(command_path: &[&str], prefix: &str) -> Result<Vec<String>> {
    let flags: &[&str] = match command_path {
        ["run"] => &[
            "--foreground",
            "--manual",
            "--max-parallel",
            "--no-merge",
            "--watch",
        ],
        ["status"] => &["--compact", "--live", "--verbose"],
        ["init"] => &["--backend", "--clean", "--no-build"],
        ["container", "build"] => &["--fingerprint"],
        ["container", "list"] => &["--all", "--json"],
        ["container", "logs"] => &[
            "--follow",
            "--format",
            "--show-thinking",
            "--tail",
            "--verbose",
        ],
        ["container", "rebuild"] => &["--all", "--fingerprint"],
        ["clean"] => &["--all", "--sessions", "--state", "--worktrees"],
        ["repair"] => &["--fix"],
        ["map"] => &["--deep", "--focus", "--overwrite"],
        ["check"] => &["--suggest"],
        ["handoff"] => &["--message", "--session", "--stage", "--trigger"],
        ["stage", "complete"] => &[
            "--assume-merged",
            "--force-unsafe",
            "--no-verify",
            "--session",
        ],
        ["stage", "reset"] => &["--hard", "--kill-session"],
        ["stage", "skip"] => &["--reason"],
        ["stage", "retry"] => &["--context", "--force"],
        ["stage", "merge"] => &["--resolved"],
        ["stage", "verify"] => &["--dry-run", "--no-reload"],
        ["stage", "human-review"] => &["--approve", "--force-complete", "--reject"],
        ["sessions", "kill"] => &["--stage"],
        ["knowledge", "check"] => &["--min-coverage", "--quiet", "--src-path"],
        ["knowledge", "audit"] => &["--max-file-lines", "--max-total-lines", "--quiet"],
        ["knowledge", "gc"] => &["--dry-run", "--model", "--quick"],
        ["knowledge", "bootstrap"] => &["--model", "--quick", "--skip-map"],
        ["memory", "note"]
        | ["memory", "decision"]
        | ["memory", "question"]
        | ["memory", "change"]
        | ["memory", "query"] => &["--stage"],
        ["memory", "list"] => &["--entry-type", "--stage"],
        ["memory", "show"] => &["--all", "--stage"],
        ["plan", "verify"] => &["--json", "--no-color", "--strict"],
        _ => return Ok(Vec::new()),
    };
    Ok(filter_prefix(flags, prefix))
}

/// Complete shell type names (bash, zsh, fish).
pub fn complete_shell_types(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(&["bash", "fish", "zsh"], prefix))
}

/// Complete model names for --model flag.
pub fn complete_model_names(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(&["haiku", "opus", "sonnet"], prefix))
}

/// Complete handoff trigger types for --trigger flag.
pub fn complete_trigger_types(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(
        &["manual", "precompact", "session_end"],
        prefix,
    ))
}

/// Commands that have subcommands (and thus should not get value completions).
pub fn has_subcommands(command: &str) -> bool {
    matches!(
        command,
        "stage" | "sessions" | "worktree" | "knowledge" | "memory" | "plan" | "container"
    )
}

/// Filter a list of candidates by prefix.
fn filter_prefix(candidates: &[&str], prefix: &str) -> Vec<String> {
    candidates
        .iter()
        .filter(|c| prefix.is_empty() || c.starts_with(prefix))
        .map(|s| s.to_string())
        .collect()
}
