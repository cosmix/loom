//! Tests for other command completions (diagnose, worktree, knowledge)

use super::super::*;
use super::setup_test_workspace;

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

#[test]
fn test_complete_knowledge_files_all() {
    let results = complete_knowledge_files("").unwrap();
    // 7 canonical + 4 aliases (deps, tech, debt, issues)
    assert_eq!(results.len(), 11);
    assert!(results.contains(&"architecture".to_string()));
    assert!(results.contains(&"entry-points".to_string()));
    assert!(results.contains(&"patterns".to_string()));
    assert!(results.contains(&"conventions".to_string()));
    assert!(results.contains(&"mistakes".to_string()));
    assert!(results.contains(&"stack".to_string()));
    assert!(results.contains(&"concerns".to_string()));
    assert!(results.contains(&"deps".to_string()));
    assert!(results.contains(&"tech".to_string()));
    assert!(results.contains(&"debt".to_string()));
    assert!(results.contains(&"issues".to_string()));
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

// Memory entry type completion tests

#[test]
fn test_complete_memory_entry_types_all() {
    let results = complete_memory_entry_types("").unwrap();
    assert_eq!(results.len(), 3);
    assert!(results.contains(&"note".to_string()));
    assert!(results.contains(&"decision".to_string()));
    assert!(results.contains(&"question".to_string()));
}

#[test]
fn test_complete_memory_entry_types_with_prefix() {
    let results = complete_memory_entry_types("de").unwrap();
    assert_eq!(results.len(), 1);
    assert!(results.contains(&"decision".to_string()));
}

#[test]
fn test_complete_memory_entry_types_no_match() {
    let results = complete_memory_entry_types("xyz").unwrap();
    assert_eq!(results.len(), 0);
}

// Integration tests for complete_dynamic routing

#[test]
fn test_complete_dynamic_memory_stage_flag() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom memory note --stage".to_string(),
        current_word: "stage".to_string(),
        prev_word: "--stage".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_memory_list_entry_type() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();

    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom memory list --entry-type".to_string(),
        current_word: "".to_string(),
        prev_word: "--entry-type".to_string(),
    };

    assert!(complete_dynamic(&ctx).is_ok());
}

// Dynamic routing tests

#[test]
fn test_complete_dynamic_top_level_commands() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();
    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom ".to_string(),
        current_word: "".to_string(),
        prev_word: "loom".to_string(),
    };
    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_stage_subcommands() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();
    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom stage ".to_string(),
        current_word: "".to_string(),
        prev_word: "stage".to_string(),
    };
    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_completions_shell() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();
    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom completions ".to_string(),
        current_word: "".to_string(),
        prev_word: "completions".to_string(),
    };
    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_flag_completion() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();
    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom run --".to_string(),
        current_word: "--".to_string(),
        prev_word: "run".to_string(),
    };
    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_model_flag_value() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();
    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom knowledge bootstrap --model ".to_string(),
        current_word: "".to_string(),
        prev_word: "--model".to_string(),
    };
    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_dynamic_trigger_flag_value() {
    let temp_dir = setup_test_workspace();
    let root = temp_dir.path();
    let ctx = CompletionContext {
        cwd: root.to_string_lossy().to_string(),
        shell: "bash".to_string(),
        cmdline: "loom handoff --trigger ".to_string(),
        current_word: "".to_string(),
        prev_word: "--trigger".to_string(),
    };
    assert!(complete_dynamic(&ctx).is_ok());
}

#[test]
fn test_complete_knowledge_files_aliases() {
    let results = complete_knowledge_files("d").unwrap();
    assert!(results.contains(&"deps".to_string()));
    assert!(results.contains(&"debt".to_string()));
}
