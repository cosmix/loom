//! Tests for `commands/knowledge/mod.rs`.

use super::*;
use crate::fs::knowledge::{KnowledgeFile, INDEX_FILENAME};
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
fn test_knowledge_update() {
    let (_temp_dir, test_dir) = setup_test_env();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");

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

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");

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

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");

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
fn test_sync_upgrades_legacy_dir() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");
    make_legacy(&test_dir);

    let result = sync::sync(false, false);
    let index_path = test_dir.join("doc/loom/knowledge").join(INDEX_FILENAME);
    let created = index_path.exists();

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");

    assert!(result.is_ok(), "sync() failed: {result:?}");
    assert!(
        created,
        "loom knowledge sync must create INDEX.md on a legacy dir"
    );
}

#[test]
fn test_normalize_heading_strips_markdown_prefix() {
    assert_eq!(normalize_heading("## Merge Flow").unwrap(), "Merge Flow");
    assert_eq!(normalize_heading("  Merge Flow  ").unwrap(), "Merge Flow");
    assert!(normalize_heading("   ").is_err());
    assert!(normalize_heading("## Two\nLines").is_err());
}

#[test]
fn test_strip_repeated_heading() {
    assert_eq!(
        strip_repeated_heading("## Merge Flow\n\nBody text\n", "Merge Flow"),
        "Body text"
    );
    assert_eq!(
        strip_repeated_heading("Body text", "Merge Flow"),
        "Body text"
    );
    // A heading that is a prefix of the body's own heading must not be stripped.
    assert_eq!(
        strip_repeated_heading("## Merge Flow Details\n\nBody", "Merge Flow"),
        "## Merge Flow Details\n\nBody"
    );
}

#[test]
#[serial]
fn test_replace_section_corrects_in_place() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");

    update(
        "patterns".to_string(),
        Some("## Merge Flow\n\nStale claim".to_string()),
    )
    .expect("seed append failed");

    let result = replace_section(
        "patterns".to_string(),
        "## Merge Flow".to_string(),
        Some("## Merge Flow\n\nCorrected claim".to_string()),
    );
    assert!(result.is_ok(), "replace_section() failed: {result:?}");

    let content = fs::read_to_string(test_dir.join("doc/loom/knowledge/patterns.md")).unwrap();
    assert!(content.contains("Corrected claim"));
    assert!(
        !content.contains("Stale claim"),
        "stale text must be gone, not appended below the fix"
    );
    assert_eq!(
        content.matches("## Merge Flow").count(),
        1,
        "the heading must not be duplicated: {content}"
    );

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_replace_section_appends_when_heading_absent() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");

    let result = replace_section(
        "conventions".to_string(),
        "Brand New".to_string(),
        Some("Fresh body".to_string()),
    );
    assert!(result.is_ok(), "replace_section() failed: {result:?}");

    let content = fs::read_to_string(test_dir.join("doc/loom/knowledge/conventions.md")).unwrap();
    assert!(content.contains("## Brand New"));
    assert!(content.contains("Fresh body"));

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_replace_section_rejects_heading_only_body() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");

    let result = replace_section(
        "patterns".to_string(),
        "Merge Flow".to_string(),
        Some("## Merge Flow".to_string()),
    );
    assert!(result.is_err(), "heading-only body must be rejected");

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}
