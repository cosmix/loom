//! Tests for dynamic shell completions.

use super::*;
use std::fs;
use tempfile::TempDir;

fn setup_test_workspace() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create doc/plans with sample files
    let plans_dir = root.join("doc/plans");
    fs::create_dir_all(&plans_dir).unwrap();
    fs::write(plans_dir.join("PLAN-0001-feature-a.md"), "content").unwrap();
    fs::write(plans_dir.join("PLAN-0002-feature-b.md"), "content").unwrap();
    fs::write(plans_dir.join("PLAN-0010-bugfix.md"), "content").unwrap();

    // Create .work/stages with sample files
    let stages_dir = root.join(".work/stages");
    fs::create_dir_all(&stages_dir).unwrap();
    fs::write(stages_dir.join("01-core-architecture.md"), "content").unwrap();
    fs::write(stages_dir.join("02-math-core.md"), "content").unwrap();
    fs::write(stages_dir.join("02-ui-framework.md"), "content").unwrap();
    fs::write(stages_dir.join("03-integration.md"), "content").unwrap();

    // Create .work/sessions with sample files
    let sessions_dir = root.join(".work/sessions");
    fs::create_dir_all(&sessions_dir).unwrap();
    fs::write(sessions_dir.join("session-001.md"), "content").unwrap();
    fs::write(sessions_dir.join("session-002.md"), "content").unwrap();
    fs::write(sessions_dir.join("session-abc.md"), "content").unwrap();

    temp_dir
}

#[test]
fn test_complete_plan_files_all() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_plan_files(root, "").unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.contains(&"doc/plans/PLAN-0001-feature-a.md".to_string()));
    assert!(results.contains(&"doc/plans/PLAN-0002-feature-b.md".to_string()));
    assert!(results.contains(&"doc/plans/PLAN-0010-bugfix.md".to_string()));
}

#[test]
fn test_complete_plan_files_with_prefix() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_plan_files(root, "PLAN-000").unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.contains(&"doc/plans/PLAN-0001-feature-a.md".to_string()));
    assert!(results.contains(&"doc/plans/PLAN-0002-feature-b.md".to_string()));
}

#[test]
fn test_complete_plan_files_no_match() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_plan_files(root, "PLAN-9999").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_complete_plan_files_missing_dir() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let results = complete_plan_files(root, "").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_complete_stage_ids_all() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_stage_ids(root, "").unwrap();
    assert_eq!(results.len(), 4);
    assert!(results.contains(&"core-architecture".to_string()));
    assert!(results.contains(&"math-core".to_string()));
    assert!(results.contains(&"ui-framework".to_string()));
    assert!(results.contains(&"integration".to_string()));
}

#[test]
fn test_complete_stage_ids_with_prefix() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_stage_ids(root, "core").unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"core-architecture".to_string()));
}

#[test]
fn test_complete_stage_ids_missing_dir() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let results = complete_stage_ids(root, "").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_complete_session_ids_all() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_session_ids(root, "").unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.contains(&"session-001".to_string()));
    assert!(results.contains(&"session-002".to_string()));
    assert!(results.contains(&"session-abc".to_string()));
}

#[test]
fn test_complete_session_ids_with_prefix() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_session_ids(root, "session-00").unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.contains(&"session-001".to_string()));
    assert!(results.contains(&"session-002".to_string()));
}

#[test]
fn test_complete_session_ids_missing_dir() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let results = complete_session_ids(root, "").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_complete_stage_or_session_ids() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let results = complete_stage_or_session_ids(root, "").unwrap();
    // 4 stages + 3 sessions = 7 total
    assert_eq!(results.len(), 7);
    assert!(results.contains(&"core-architecture".to_string()));
    assert!(results.contains(&"session-001".to_string()));
}

#[test]
fn test_completion_context_from_args() {
    let args = vec![
        "/home/user/project".to_string(),
        "loom init PLAN".to_string(),
        "PLAN-001".to_string(),
        "init".to_string(),
    ];

    let ctx = CompletionContext::from_args("bash", &args);

    assert_eq!(ctx.cwd, "/home/user/project");
    assert_eq!(ctx.shell, "bash");
    assert_eq!(ctx.cmdline, "loom init PLAN");
    assert_eq!(ctx.current_word, "PLAN-001");
    assert_eq!(ctx.prev_word, "init");
}

#[test]
fn test_completion_context_from_args_empty() {
    let args: Vec<String> = Vec::new();
    let ctx = CompletionContext::from_args("zsh", &args);

    assert_eq!(ctx.cwd, ".");
    assert_eq!(ctx.shell, "zsh");
    assert_eq!(ctx.cmdline, "");
    assert_eq!(ctx.current_word, "");
    assert_eq!(ctx.prev_word, "");
}

#[test]
fn test_complete_dynamic_init() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom init".to_string(),
        current_word: "PLAN".to_string(),
        prev_word: "init".to_string(),
    };

    // complete_dynamic prints to stdout, so we can't easily test the output
    // but we can verify it doesn't error
    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_verify() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom verify".to_string(),
        current_word: "core".to_string(),
        prev_word: "verify".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_sessions_kill() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom sessions kill".to_string(),
        current_word: "".to_string(),
        prev_word: "kill".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_complete() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage complete".to_string(),
        current_word: "".to_string(),
        prev_word: "complete".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_no_match() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom status".to_string(),
        current_word: "".to_string(),
        prev_word: "status".to_string(),
    };

    // Should not error, just return empty results
    assert!(complete_dynamic(&ctx).is_ok());
}

// ============ Tests for new stage subcommands ============

#[test]
fn test_complete_dynamic_stage_hold() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage hold".to_string(),
        current_word: "core".to_string(),
        prev_word: "hold".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_release() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage release".to_string(),
        current_word: "".to_string(),
        prev_word: "release".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_skip() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage skip".to_string(),
        current_word: "ui".to_string(),
        prev_word: "skip".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_retry() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage retry".to_string(),
        current_word: "".to_string(),
        prev_word: "retry".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_recover() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage recover".to_string(),
        current_word: "math".to_string(),
        prev_word: "recover".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_verify() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage verify".to_string(),
        current_word: "".to_string(),
        prev_word: "verify".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_merge_complete() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage merge-complete".to_string(),
        current_word: "".to_string(),
        prev_word: "merge-complete".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_resume() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage resume".to_string(),
        current_word: "".to_string(),
        prev_word: "resume".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

// ============ Tests for diagnose command ============

#[test]
fn test_complete_dynamic_diagnose() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom diagnose".to_string(),
        current_word: "core".to_string(),
        prev_word: "diagnose".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

// ============ Tests for worktree remove ============

#[test]
fn test_complete_dynamic_worktree_remove() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom worktree remove".to_string(),
        current_word: "".to_string(),
        prev_word: "remove".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

// ============ Tests for --stage flag completion ============

#[test]
fn test_complete_dynamic_sessions_kill_stage_flag() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom sessions kill --stage".to_string(),
        current_word: "core".to_string(),
        prev_word: "--stage".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

// ============ Tests for stage output subcommands ============

#[test]
fn test_complete_dynamic_stage_output_set() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output set".to_string(),
        current_word: "".to_string(),
        prev_word: "set".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_output_get() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output get".to_string(),
        current_word: "math".to_string(),
        prev_word: "get".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_output_list() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output list".to_string(),
        current_word: "".to_string(),
        prev_word: "list".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_output_remove() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage output remove".to_string(),
        current_word: "ui".to_string(),
        prev_word: "remove".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

// ============ Tests for knowledge file completion ============

#[test]
fn test_complete_knowledge_files_all() {
    let results = complete_knowledge_files("").unwrap();
    assert_eq!(results.len(), 4);
    assert!(results.contains(&"entry-points".to_string()));
    assert!(results.contains(&"patterns".to_string()));
    assert!(results.contains(&"conventions".to_string()));
    assert!(results.contains(&"mistakes".to_string()));
}

#[test]
fn test_complete_knowledge_files_with_prefix() {
    let results = complete_knowledge_files("pa").unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"patterns".to_string()));
}

#[test]
fn test_complete_knowledge_files_prefix_e() {
    let results = complete_knowledge_files("e").unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"entry-points".to_string()));
}

#[test]
fn test_complete_knowledge_files_no_match() {
    let results = complete_knowledge_files("xyz").unwrap();
    assert_eq!(results.len(), 0);
}

#[test]
fn test_complete_dynamic_knowledge_show() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom knowledge show".to_string(),
        current_word: "pa".to_string(),
        prev_word: "show".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_knowledge_update() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom knowledge update".to_string(),
        current_word: "".to_string(),
        prev_word: "update".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}
