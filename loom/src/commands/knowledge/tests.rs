//! Tests for `commands/knowledge/mod.rs`.

use super::*;
use crate::fs::knowledge::INDEX_FILENAME;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

pub(super) fn setup_test_env() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();
    (temp_dir, test_dir)
}

/// Downgrade a just-initialized (hierarchical) knowledge dir to the flat
/// legacy layout that pre-hierarchy projects have on disk.
pub(super) fn make_legacy(test_dir: &std::path::Path) {
    let index_path = test_dir.join("doc/loom/knowledge").join(INDEX_FILENAME);
    fs::remove_file(&index_path).expect("Failed to remove INDEX.md to simulate a legacy dir");
}

#[test]
fn test_parse_file_type() {
    assert_eq!(
        KnowledgeFile::parse("entry-points.md").unwrap(),
        KnowledgeFile::EntryPoints
    );
    assert_eq!(
        KnowledgeFile::parse("entry-points").unwrap(),
        KnowledgeFile::EntryPoints
    );
    assert_eq!(
        KnowledgeFile::parse("patterns").unwrap(),
        KnowledgeFile::Patterns
    );
    assert_eq!(
        KnowledgeFile::parse("conventions").unwrap(),
        KnowledgeFile::Conventions
    );
    assert_eq!(
        KnowledgeFile::parse("entry").unwrap(),
        KnowledgeFile::EntryPoints
    );
    assert_eq!(
        KnowledgeFile::parse("mistakes").unwrap(),
        KnowledgeFile::Mistakes
    );
    assert_eq!(
        KnowledgeFile::parse("mistakes.md").unwrap(),
        KnowledgeFile::Mistakes
    );
    assert_eq!(
        KnowledgeFile::parse("mistake").unwrap(),
        KnowledgeFile::Mistakes
    );
    assert_eq!(
        KnowledgeFile::parse("lessons").unwrap(),
        KnowledgeFile::Mistakes
    );
    assert_eq!(
        KnowledgeFile::parse("lesson").unwrap(),
        KnowledgeFile::Mistakes
    );
    assert_eq!(KnowledgeFile::parse("stack").unwrap(), KnowledgeFile::Stack);
    assert_eq!(
        KnowledgeFile::parse("stack.md").unwrap(),
        KnowledgeFile::Stack
    );
    assert_eq!(KnowledgeFile::parse("deps").unwrap(), KnowledgeFile::Stack);
    assert_eq!(
        KnowledgeFile::parse("dependencies").unwrap(),
        KnowledgeFile::Stack
    );
    assert_eq!(KnowledgeFile::parse("tech").unwrap(), KnowledgeFile::Stack);
    assert_eq!(
        KnowledgeFile::parse("concerns").unwrap(),
        KnowledgeFile::Concerns
    );
    assert_eq!(
        KnowledgeFile::parse("concerns.md").unwrap(),
        KnowledgeFile::Concerns
    );
    assert_eq!(
        KnowledgeFile::parse("debt").unwrap(),
        KnowledgeFile::Concerns
    );
    assert_eq!(
        KnowledgeFile::parse("issues").unwrap(),
        KnowledgeFile::Concerns
    );
    assert!(KnowledgeFile::parse("unknown").is_none());
}

#[test]
#[serial]
fn test_knowledge_init() {
    let (_temp_dir, test_dir) = setup_test_env();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    let result = init();
    assert!(result.is_ok());

    let knowledge_dir = test_dir.join("doc/loom/knowledge");
    assert!(knowledge_dir.exists());
    assert!(knowledge_dir.join("entry-points.md").exists());
    assert!(knowledge_dir.join("patterns.md").exists());
    assert!(knowledge_dir.join("conventions.md").exists());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_init_creates_index() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");

    let index_path = test_dir.join("doc/loom/knowledge").join(INDEX_FILENAME);
    assert!(index_path.exists(), "init() must create INDEX.md");

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_knowledge_update() {
    let (_temp_dir, test_dir) = setup_test_env();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");

    let result = update(
        "entry-points".to_string(),
        Some("## New Section\n\n- New entry".to_string()),
    );
    assert!(result.is_ok());

    let content = fs::read_to_string(test_dir.join("doc/loom/knowledge/entry-points.md")).unwrap();
    assert!(content.contains("## New Section"));
    assert!(content.contains("- New entry"));

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
#[cfg(unix)]
fn test_knowledge_update_in_worktree_writes_to_worktree() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let base = temp_dir.path();

    let main_repo = base.join("main-repo");
    let main_work = main_repo.join(".work");
    fs::create_dir_all(&main_work).expect("Failed to create main .work dir");

    for subdir in &[
        "runners",
        "tracks",
        "signals",
        "handoffs",
        "archive",
        "stages",
        "sessions",
        "logs",
        "crashes",
        "checkpoints",
        "task-state",
    ] {
        fs::create_dir(main_work.join(subdir)).expect("Failed to create subdir");
    }

    let worktree = main_repo.join(".worktrees").join("my-worktree");
    fs::create_dir_all(&worktree).expect("Failed to create worktree dir");

    let worktree_work = worktree.join(".work");
    #[cfg(unix)]
    {
        let target = std::path::PathBuf::from("..").join("..").join(".work");
        std::os::unix::fs::symlink(&target, &worktree_work).expect("Failed to create symlink");
    }

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&worktree).expect("Failed to change dir to worktree");

    let result = init();
    assert!(result.is_ok(), "init() failed: {result:?}");

    let main_knowledge_dir = main_repo.join("doc/loom/knowledge");
    let worktree_knowledge_dir = worktree.join("doc/loom/knowledge");

    assert!(
        worktree_knowledge_dir.exists(),
        "Knowledge dir should exist in worktree at {worktree_knowledge_dir:?}"
    );
    assert!(worktree_knowledge_dir.join("entry-points.md").exists());

    let result = update(
        "entry-points".to_string(),
        Some("## Test Entry\n\n- test/file.rs - Test description".to_string()),
    );
    assert!(result.is_ok(), "update() failed: {result:?}");

    let content = fs::read_to_string(worktree_knowledge_dir.join("entry-points.md")).unwrap();
    assert!(
        content.contains("## Test Entry"),
        "Content should be in worktree"
    );

    assert!(
        !main_knowledge_dir.exists(),
        "Main repo should not have knowledge dir written by worktree"
    );

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_update_with_explicit_content() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");

    let result = update(
        "patterns".to_string(),
        Some("## Test Pattern\n\nExplicit content".to_string()),
    );
    assert!(result.is_ok());

    let content = fs::read_to_string(test_dir.join("doc/loom/knowledge/patterns.md")).unwrap();
    assert!(content.contains("## Test Pattern"));
    assert!(content.contains("Explicit content"));

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_list_with_topics_does_not_crash() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");
    update(
        "architecture/merge-flow".to_string(),
        Some("## Merge Flow\n\nDetails".to_string()),
    )
    .expect("update() failed");

    let result = list();
    assert!(result.is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_index_upgrades_legacy_dir() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");
    make_legacy(&test_dir);

    let result = index();
    assert!(result.is_ok());

    let index_path = test_dir.join("doc/loom/knowledge").join(INDEX_FILENAME);
    assert!(
        index_path.exists(),
        "index() must create INDEX.md on a legacy dir"
    );

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_index_errors_when_knowledge_dir_missing() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    let result = index();
    assert!(result.is_err());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}
