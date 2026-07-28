//! Tests for `commands/knowledge/audit.rs`.

use super::*;
use crate::commands::knowledge::tests::{make_legacy, setup_test_env};
use crate::fs::knowledge::INDEX_FILENAME;
use serial_test::serial;

#[test]
#[serial]
fn test_audit_clean() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    crate::commands::knowledge::update(
        "architecture".to_string(),
        Some("## Overview\n\nSmall content".to_string()),
    )
    .expect("Failed to update");

    let result = audit(200, 800, true);
    assert!(result.is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_audit_large_file() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");

    let mut big_content = String::from("## Big Section\n\n");
    for i in 0..250 {
        big_content.push_str(&format!("- Line {}\n", i));
    }
    crate::commands::knowledge::update("architecture".to_string(), Some(big_content))
        .expect("Failed to update");

    let result = audit(200, 800, true);
    assert!(result.is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_audit_reports_topics() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    crate::commands::knowledge::update(
        "architecture/merge-flow".to_string(),
        Some("## Merge Flow\n\nDetails about the merge flow.".to_string()),
    )
    .expect("Failed to update topic");

    let result = audit(200, 800, true);
    assert!(result.is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}

#[test]
#[serial]
fn test_audit_legacy_dir_does_not_auto_migrate() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    make_legacy(&test_dir);

    crate::commands::knowledge::update(
        "architecture".to_string(),
        Some("## Overview\n\nSmall content".to_string()),
    )
    .expect("Failed to update");

    let result = audit(200, 800, true);
    assert!(
        result.is_ok(),
        "audit() must behave the same on a legacy dir: {result:?}"
    );

    let knowledge_root = test_dir.join("doc/loom/knowledge");
    assert!(!knowledge_root.join(INDEX_FILENAME).exists());
    assert!(!knowledge_root.join("architecture").exists());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}
