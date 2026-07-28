//! Tests for `commands/knowledge/check.rs`.

use super::*;
use crate::commands::knowledge::tests::make_legacy;
use crate::fs::knowledge::INDEX_FILENAME;
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_count_content_sections() {
    let content = r#"# Architecture

> This file is append-only

## Component A

Description here

## Component B

More description

(Add patterns as you discover them)
"#;
    let (has_content, count) = count_content_sections(content);
    assert!(has_content);
    assert_eq!(count, 2);
}

#[test]
fn test_count_content_sections_empty() {
    let content = r#"# Architecture

> This file is append-only

(Add patterns as you discover them)
"#;
    let (has_content, count) = count_content_sections(content);
    assert!(!has_content);
    assert_eq!(count, 0);
}

#[test]
fn test_is_directory_mentioned() {
    let content = r#"
## Directory Structure

- commands/ - CLI command implementations
- daemon/ - Background daemon
- orchestrator/ - Core orchestration
"#;
    assert!(is_directory_mentioned("commands", content));
    assert!(is_directory_mentioned("daemon", content));
    assert!(is_directory_mentioned("orchestrator", content));
    assert!(!is_directory_mentioned("nonexistent", content));
}

#[test]
fn test_is_directory_mentioned_various_formats() {
    assert!(is_directory_mentioned("src", "located at src/lib.rs"));
    assert!(is_directory_mentioned("models", "the `models` directory"));
    assert!(is_directory_mentioned("utils", "the **utils** module"));
    assert!(is_directory_mentioned("api", "## api\n\nAPI routes"));
}

#[test]
#[serial]
fn test_check_missing_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    let result = check(50, None, true);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("does not exist"));

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_check_empty_architecture() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");

    let result = check(50, None, true);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("architecture.md is empty"));

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_check_passes_with_content() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    crate::commands::knowledge::update(
        "architecture".to_string(),
        Some("## Overview\n\nProject architecture here".to_string()),
    )
    .expect("Failed to update architecture");

    let result = check(50, None, true);
    assert!(result.is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_check_coverage_calculation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();

    let src_dir = test_dir.join("src");
    fs::create_dir_all(src_dir.join("commands")).unwrap();
    fs::create_dir_all(src_dir.join("models")).unwrap();
    fs::create_dir_all(src_dir.join("utils")).unwrap();
    fs::create_dir_all(src_dir.join("daemon")).unwrap();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    crate::commands::knowledge::update(
        "architecture".to_string(),
        Some("## Overview\n\n- commands/ - CLI\n- models/ - Data".to_string()),
    )
    .expect("Failed to update architecture");

    let result = check(50, None, true);
    assert!(result.is_ok());

    let result = check(75, None, true);
    assert!(result.is_err());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
fn test_get_src_subdirectories() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();

    let src_dir = test_dir.join("src");
    fs::create_dir_all(src_dir.join("commands")).unwrap();
    fs::create_dir_all(src_dir.join("models")).unwrap();
    fs::create_dir_all(src_dir.join(".hidden")).unwrap();
    fs::create_dir_all(src_dir.join("target")).unwrap();

    let dirs = get_src_subdirectories(&test_dir, None).unwrap();
    assert!(dirs.contains(&"commands".to_string()));
    assert!(dirs.contains(&"models".to_string()));
    assert!(!dirs.contains(&".hidden".to_string()));
    assert!(!dirs.contains(&"target".to_string()));
}

#[test]
#[serial]
fn test_check_includes_gc_analysis() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    crate::commands::knowledge::update(
        "architecture".to_string(),
        Some("## Overview\n\nProject architecture here".to_string()),
    )
    .expect("Failed to update architecture");

    let result = check(50, None, false);
    assert!(result.is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_check_coverage_counts_tier2_architecture_topic() {
    // architecture.md itself says nothing about "widgets" — only a tier-2
    // topic does. Coverage must still count it as documented.
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();

    let src_dir = test_dir.join("src");
    fs::create_dir_all(src_dir.join("widgets")).unwrap();

    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    crate::commands::knowledge::update(
        "architecture".to_string(),
        Some("## Overview\n\nTop-level summary with no directory mentions.".to_string()),
    )
    .expect("Failed to update architecture");
    crate::commands::knowledge::update(
        "architecture/widgets-flow".to_string(),
        Some("## Widgets Flow\n\nThe widgets/ directory handles widget rendering.".to_string()),
    )
    .expect("Failed to update topic");

    let result = check(100, None, true);
    assert!(
        result.is_ok(),
        "coverage should count a directory mentioned only in a tier-2 topic: {result:?}"
    );

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_check_legacy_dir_never_auto_migrates() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_dir = temp_dir.path().to_path_buf();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    make_legacy(&test_dir);
    crate::commands::knowledge::update(
        "architecture".to_string(),
        Some("## Overview\n\nProject architecture here".to_string()),
    )
    .expect("Failed to update architecture");

    let result = check(50, None, true);
    assert!(
        result.is_ok(),
        "check() must behave the same on a legacy dir: {result:?}"
    );

    let knowledge_root = test_dir.join("doc/loom/knowledge");
    assert!(!knowledge_root.join(INDEX_FILENAME).exists());
    assert!(!knowledge_root.join("architecture").exists());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}
