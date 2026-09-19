//! Tests for `loom knowledge delete-section`'s fs-layer function, plus the
//! CLI-level `delete_section` wrapper in `commands/knowledge/mod.rs`.

use super::tests::setup_test_env;
use super::*;
use crate::fs::knowledge::{KnowledgeDir, KnowledgeTarget};
use serial_test::serial;
use std::fs;
use tempfile::TempDir;

fn knowledge_with_topic(content: &str) -> (TempDir, KnowledgeDir, KnowledgeTarget) {
    let temp = TempDir::new().expect("temp dir");
    let knowledge = KnowledgeDir::from_root(temp.path());
    let target = KnowledgeTarget::parse("architecture/topic").expect("target");
    let path = knowledge.target_path(&target);
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    fs::write(&path, content).expect("write topic");
    (temp, knowledge, target)
}

fn topic_text(knowledge: &KnowledgeDir, target: &KnowledgeTarget) -> String {
    fs::read_to_string(knowledge.target_path(target)).expect("read topic")
}

#[test]
fn deletes_an_h2_section_and_keeps_its_neighbours() {
    let (_temp, knowledge, target) =
        knowledge_with_topic("# T\n\n## Keep one\n\na\n\n## Drop\n\nb\n\n## Keep two\n\nc\n");

    let level = knowledge
        .delete_section_target(&target, "Drop")
        .expect("delete");

    assert_eq!(level, 2);
    assert_eq!(
        topic_text(&knowledge, &target),
        "# T\n\n## Keep one\n\na\n\n## Keep two\n\nc\n"
    );
}

#[test]
fn deleting_an_h2_takes_its_nested_h3_with_it() {
    let (_temp, knowledge, target) =
        knowledge_with_topic("# T\n\n## Drop\n\n### Nested\n\nx\n\n## Keep\n\ny\n");

    knowledge
        .delete_section_target(&target, "Drop")
        .expect("delete");

    assert_eq!(topic_text(&knowledge, &target), "# T\n\n## Keep\n\ny\n");
}

#[test]
fn deletes_a_nested_h3_and_the_last_section_of_a_file() {
    let (_temp, knowledge, target) =
        knowledge_with_topic("# T\n\n## Group\n\n### Keep\n\nx\n\n### Drop\n\ny\n");

    let level = knowledge
        .delete_section_target(&target, "Drop")
        .expect("delete");

    assert_eq!(level, 3);
    assert_eq!(
        topic_text(&knowledge, &target),
        "# T\n\n## Group\n\n### Keep\n\nx\n"
    );
}

#[test]
fn a_heading_that_matches_nothing_is_an_error_and_leaves_the_file_alone() {
    let original = "# T\n\n```markdown\n## Drop\n```\n\n## Keep\n\nx\n";
    let (_temp, knowledge, target) = knowledge_with_topic(original);

    let error = knowledge
        .delete_section_target(&target, "Drop")
        .expect_err("a fenced heading is not a section");

    assert!(error.to_string().contains("No \"Drop\" section"), "{error}");
    assert_eq!(topic_text(&knowledge, &target), original);
}

#[test]
fn a_missing_target_file_is_an_error_and_is_not_created() {
    let temp = TempDir::new().expect("temp dir");
    let knowledge = KnowledgeDir::from_root(temp.path());
    let target = KnowledgeTarget::parse("architecture/absent").expect("target");

    assert!(knowledge.delete_section_target(&target, "Any").is_err());
    assert!(!knowledge.target_path(&target).exists());
}

#[test]
fn deletes_a_section_from_a_crlf_file() {
    let (_temp, knowledge, target) =
        knowledge_with_topic("# T\r\n\r\n## Drop\r\n\r\nb\r\n\r\n## Keep\r\n\r\nc\r\n");

    knowledge
        .delete_section_target(&target, "Drop")
        .expect("delete");

    let text = topic_text(&knowledge, &target);
    assert!(!text.contains("Drop"), "{text:?}");
    assert!(text.contains("## Keep") && text.contains('c'), "{text:?}");
}

// --- CLI-level `delete_section` wrapper (`commands/knowledge/mod.rs`). ---

#[test]
#[serial]
fn cli_delete_section_removes_the_section_and_regenerates_the_index() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");
    update(
        "patterns".to_string(),
        Some("## Doomed Section\n\nGoing away".to_string()),
    )
    .expect("seed append failed");

    let index_before =
        fs::read_to_string(test_dir.join("doc/loom/knowledge/INDEX.md")).expect("read INDEX.md");

    let result = delete_section("patterns".to_string(), "## Doomed Section".to_string());

    let content = fs::read_to_string(test_dir.join("doc/loom/knowledge/patterns.md"))
        .expect("read patterns.md");
    let index_after =
        fs::read_to_string(test_dir.join("doc/loom/knowledge/INDEX.md")).expect("read INDEX.md");

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");

    assert!(result.is_ok(), "delete_section() failed: {result:?}");
    assert!(
        !content.contains("Doomed Section"),
        "deleted section must be gone: {content}"
    );
    assert_ne!(
        index_before, index_after,
        "INDEX.md must be regenerated after the section it summarized is deleted"
    );
}

#[test]
#[serial]
fn cli_delete_section_on_an_unknown_heading_errors_and_leaves_the_file_alone() {
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    KnowledgeDir::new(".")
        .initialize()
        .expect("Failed to initialize knowledge");
    update(
        "patterns".to_string(),
        Some("## Keep This\n\nStays put".to_string()),
    )
    .expect("seed append failed");
    let before =
        fs::read_to_string(test_dir.join("doc/loom/knowledge/patterns.md")).expect("read before");

    let result = delete_section("patterns".to_string(), "No Such Section".to_string());

    let after =
        fs::read_to_string(test_dir.join("doc/loom/knowledge/patterns.md")).expect("read after");

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");

    assert!(result.is_err(), "an unknown heading must be an error");
    assert_eq!(before, after, "a failed delete must leave the file alone");
}
