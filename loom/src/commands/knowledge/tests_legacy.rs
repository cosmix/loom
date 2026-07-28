//! Backwards-compatibility contract tests for flat (legacy) knowledge dirs.
//!
//! Loom runs against user repos whose `doc/loom/knowledge/` predates the tiered
//! hierarchy. These tests pin the contract: a flat dir behaves exactly as it did
//! before, and nothing auto-migrates it.

use super::tests::{make_legacy, setup_test_env};
use super::*;
use crate::fs::knowledge::INDEX_FILENAME;
use serial_test::serial;
use std::fs;

#[test]
#[serial]
fn test_update_tier1_byte_identical_legacy_and_hierarchical() {
    let (_temp_h, test_dir_h) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir_h).expect("Failed to change dir");
    init().expect("Failed to init knowledge");
    update(
        "patterns".to_string(),
        Some("## Shared\n\nSame content".to_string()),
    )
    .expect("Failed to update");
    let hierarchical_content =
        fs::read_to_string(test_dir_h.join("doc/loom/knowledge/patterns.md")).unwrap();
    std::env::set_current_dir(&original_dir).expect("Failed to restore dir");

    let (_temp_l, test_dir_l) = setup_test_env();
    std::env::set_current_dir(&test_dir_l).expect("Failed to change dir");
    init().expect("Failed to init knowledge");
    make_legacy(&test_dir_l);
    update(
        "patterns".to_string(),
        Some("## Shared\n\nSame content".to_string()),
    )
    .expect("Failed to update");
    let legacy_content =
        fs::read_to_string(test_dir_l.join("doc/loom/knowledge/patterns.md")).unwrap();
    std::env::set_current_dir(original_dir).expect("Failed to restore dir");

    assert_eq!(
        hierarchical_content, legacy_content,
        "tier-1 update() output must be byte-identical regardless of layout"
    );
}

#[test]
#[serial]
fn test_replace_section_legacy_and_hierarchical_identical() {
    let (_temp_h, test_dir_h) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir_h).expect("Failed to change dir");
    init().expect("Failed to init knowledge");
    update("patterns".to_string(), Some("## A\n\nOriginal".to_string())).expect("update failed");
    replace_section(
        "patterns".to_string(),
        "A".to_string(),
        Some("Updated".to_string()),
    )
    .expect("replace_section failed");
    let hierarchical_content =
        fs::read_to_string(test_dir_h.join("doc/loom/knowledge/patterns.md")).unwrap();
    std::env::set_current_dir(&original_dir).expect("Failed to restore dir");

    let (_temp_l, test_dir_l) = setup_test_env();
    std::env::set_current_dir(&test_dir_l).expect("Failed to change dir");
    init().expect("Failed to init knowledge");
    make_legacy(&test_dir_l);
    update("patterns".to_string(), Some("## A\n\nOriginal".to_string())).expect("update failed");
    replace_section(
        "patterns".to_string(),
        "A".to_string(),
        Some("Updated".to_string()),
    )
    .expect("replace_section failed");
    let legacy_content =
        fs::read_to_string(test_dir_l.join("doc/loom/knowledge/patterns.md")).unwrap();
    std::env::set_current_dir(original_dir).expect("Failed to restore dir");

    assert_eq!(
        hierarchical_content, legacy_content,
        "tier-1 replace_section() output must be byte-identical regardless of layout"
    );
}

#[test]
#[serial]
fn test_update_topic_target_writes_under_category_dir() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");
    let result = update(
        "architecture/merge-flow".to_string(),
        Some("## Merge Flow\n\nDetails about the merge flow.".to_string()),
    );
    assert!(result.is_ok(), "update() to a topic failed: {result:?}");

    let topic_path = test_dir.join("doc/loom/knowledge/architecture/merge-flow.md");
    assert!(topic_path.exists(), "topic file should be created");
    let content = fs::read_to_string(&topic_path).unwrap();
    assert!(content.contains("Details about the merge flow"));

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_show_legacy_no_arg_does_not_auto_migrate() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");
    make_legacy(&test_dir);

    let result = show(None);
    assert!(result.is_ok());

    let index_path = test_dir.join("doc/loom/knowledge").join(INDEX_FILENAME);
    assert!(
        !index_path.exists(),
        "show() must not auto-create INDEX.md on a legacy dir"
    );

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_show_hierarchical_no_arg_uses_index() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");
    assert!(show(None).is_ok());
    assert!(show(Some("patterns".to_string())).is_ok());
    assert!(show(Some("pattern".to_string())).is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_update_legacy_dir_never_creates_index_or_category_dir() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");
    make_legacy(&test_dir);

    update(
        "architecture".to_string(),
        Some("## Overview\n\nLegacy content".to_string()),
    )
    .expect("update() failed");

    let knowledge_root = test_dir.join("doc/loom/knowledge");
    assert!(!knowledge_root.join(INDEX_FILENAME).exists());
    assert!(!knowledge_root.join("architecture").exists());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_list_legacy_dir_never_auto_migrates() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    init().expect("Failed to init knowledge");
    make_legacy(&test_dir);

    let result = list();
    assert!(result.is_ok());

    let knowledge_root = test_dir.join("doc/loom/knowledge");
    assert!(!knowledge_root.join(INDEX_FILENAME).exists());
    assert!(!knowledge_root.join("architecture").exists());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}
