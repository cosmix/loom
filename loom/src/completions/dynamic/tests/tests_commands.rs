//! Tests for command, subcommand, and flag completions

use crate::completions::dynamic::commands::*;

#[test]
fn test_complete_commands_all() {
    let results = complete_commands("").unwrap();
    assert!(results.len() >= 20); // At least 20 top-level commands
    assert!(results.contains(&"init".to_string()));
    assert!(results.contains(&"run".to_string()));
    assert!(results.contains(&"stage".to_string()));
    assert!(results.contains(&"status".to_string()));
    assert!(results.contains(&"completions".to_string()));
}

#[test]
fn test_complete_commands_prefix() {
    let results = complete_commands("st").unwrap();
    assert!(results.contains(&"stage".to_string()));
    assert!(results.contains(&"status".to_string()));
    assert!(results.contains(&"stop".to_string()));
    assert!(!results.contains(&"init".to_string()));
}

#[test]
fn test_complete_commands_no_match() {
    let results = complete_commands("xyz").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_complete_subcommands_stage() {
    let results = complete_subcommands("stage", "").unwrap();
    assert!(results.contains(&"complete".to_string()));
    assert!(results.contains(&"block".to_string()));
    assert!(results.contains(&"reset".to_string()));
    assert!(results.contains(&"human-review".to_string()));
    assert!(results.contains(&"dispute-criteria".to_string()));
    assert!(results.contains(&"output".to_string()));
}

#[test]
fn test_complete_subcommands_stage_prefix() {
    let results = complete_subcommands("stage", "com").unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"complete".to_string()));
}

#[test]
fn test_complete_subcommands_knowledge() {
    let results = complete_subcommands("knowledge", "").unwrap();
    assert!(results.contains(&"show".to_string()));
    assert!(results.contains(&"update".to_string()));
    assert!(results.contains(&"bootstrap".to_string()));
    assert!(results.contains(&"check".to_string()));
    assert!(results.contains(&"gc".to_string()));
}

#[test]
fn test_complete_subcommands_memory() {
    let results = complete_subcommands("memory", "").unwrap();
    assert!(results.contains(&"note".to_string()));
    assert!(results.contains(&"decision".to_string()));
    assert!(results.contains(&"list".to_string()));
    assert!(results.contains(&"show".to_string()));
}

#[test]
fn test_complete_subcommands_sessions() {
    let results = complete_subcommands("sessions", "").unwrap();
    assert!(results.contains(&"list".to_string()));
    assert!(results.contains(&"kill".to_string()));
}

#[test]
fn test_complete_subcommands_worktree() {
    let results = complete_subcommands("worktree", "").unwrap();
    assert!(results.contains(&"list".to_string()));
    assert!(results.contains(&"remove".to_string()));
}

#[test]
fn test_complete_subcommands_output() {
    let results = complete_subcommands("output", "").unwrap();
    assert!(results.contains(&"set".to_string()));
    assert!(results.contains(&"get".to_string()));
    assert!(results.contains(&"list".to_string()));
    assert!(results.contains(&"remove".to_string()));
}

#[test]
fn test_complete_subcommands_unknown() {
    let results = complete_subcommands("unknown", "").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_complete_flags_run() {
    let results = complete_flags(&["run"], "--").unwrap();
    assert!(results.contains(&"--manual".to_string()));
    assert!(results.contains(&"--watch".to_string()));
    assert!(results.contains(&"--foreground".to_string()));
    assert!(results.contains(&"--max-parallel".to_string()));
    assert!(results.contains(&"--no-merge".to_string()));
}

#[test]
fn test_complete_flags_run_prefix() {
    let results = complete_flags(&["run"], "--m").unwrap();
    assert!(results.contains(&"--manual".to_string()));
    assert!(results.contains(&"--max-parallel".to_string()));
    assert!(!results.contains(&"--watch".to_string()));
}

#[test]
fn test_complete_flags_status() {
    let results = complete_flags(&["status"], "--").unwrap();
    assert!(results.contains(&"--live".to_string()));
    assert!(results.contains(&"--compact".to_string()));
    assert!(results.contains(&"--verbose".to_string()));
}

#[test]
fn test_complete_flags_stage_complete() {
    let results = complete_flags(&["stage", "complete"], "--").unwrap();
    assert!(results.contains(&"--no-verify".to_string()));
    assert!(results.contains(&"--force-unsafe".to_string()));
    assert!(results.contains(&"--session".to_string()));
}

#[test]
fn test_complete_flags_stage_retry() {
    let results = complete_flags(&["stage", "retry"], "--").unwrap();
    assert!(results.contains(&"--force".to_string()));
    assert!(results.contains(&"--context".to_string()));
}

#[test]
fn test_complete_flags_unknown_command() {
    let results = complete_flags(&["nonexistent"], "--").unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_complete_shell_types_all() {
    let results = complete_shell_types("").unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.contains(&"bash".to_string()));
    assert!(results.contains(&"zsh".to_string()));
    assert!(results.contains(&"fish".to_string()));
}

#[test]
fn test_complete_shell_types_prefix() {
    let results = complete_shell_types("b").unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"bash".to_string()));
}

#[test]
fn test_complete_model_names_all() {
    let results = complete_model_names("").unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.contains(&"sonnet".to_string()));
    assert!(results.contains(&"opus".to_string()));
    assert!(results.contains(&"haiku".to_string()));
}

#[test]
fn test_complete_trigger_types_all() {
    let results = complete_trigger_types("").unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.contains(&"manual".to_string()));
    assert!(results.contains(&"precompact".to_string()));
    assert!(results.contains(&"session_end".to_string()));
}

#[test]
fn test_has_subcommands() {
    assert!(has_subcommands("stage"));
    assert!(has_subcommands("sessions"));
    assert!(has_subcommands("worktree"));
    assert!(has_subcommands("knowledge"));
    assert!(has_subcommands("memory"));
    assert!(!has_subcommands("init"));
    assert!(!has_subcommands("run"));
    assert!(!has_subcommands("status"));
}
