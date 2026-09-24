//! Repository-backed `plan verify` lints: Rust test filters against a base
//! layer, and the knowledge check against a real tree.

use tempfile::TempDir;

use crate::plan::schema::StageType;

use super::{
    git_repo, lint_with_notes, matching, plan, publish_base_layer, stage, write_knowledge,
    DUPLICATE_HEADING,
};

const NO_MODULE: &str = "exists in the source graph for HEAD";
const KNOWLEDGE: &str = "structural issue in the knowledge tree";

#[test]
fn rust_filter_naming_a_graph_module_is_clean() {
    let repo = git_repo();
    publish_base_layer(repo.path(), &["src/lib.rs", "src/present_mod.rs"]);
    let criteria = [
        "cargo test --lib present_mod::",
        "cargo test --manifest-path Cargo.toml -- present_mod::case --skip absent::case",
        "cargo test --lib -- plain_name",
    ];
    let metadata = plan(vec![stage("feature", StageType::Standard, &criteria)]);
    let found = matching(&metadata, Some(repo.path()), NO_MODULE);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn rust_filter_for_a_module_the_stage_creates_is_clean() {
    let repo = git_repo();
    publish_base_layer(repo.path(), &["src/lib.rs"]);
    let criteria = [
        "cargo test --lib new_mod::",
        "cargo test --lib lints::fresh::",
        "cargo test --lib globbed::inner::",
        "cargo test --lib artifact_mod::",
    ];
    let mut creating = stage("feature", StageType::Standard, &criteria);
    creating.files = vec![
        "loom/src/new_mod.rs".to_string(),
        "loom/src/plan/lints/fresh/mod.rs".to_string(),
        "loom/src/globbed/**".to_string(),
    ];
    creating.artifacts = vec!["./src/artifact_mod.rs".to_string()];
    let found = matching(&plan(vec![creating]), Some(repo.path()), NO_MODULE);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn rust_filter_with_a_misspelt_outer_segment_warns() {
    let repo = git_repo();
    publish_base_layer(repo.path(), &["src/lib.rs", "src/plan/tests.rs"]);
    let criterion = "cargo test --lib totally::wrong::tests::";
    let metadata = plan(vec![stage("feature", StageType::Standard, &[criterion])]);
    let found = matching(&metadata, Some(repo.path()), NO_MODULE);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].message.contains("`totally::wrong::tests`"),
        "{found:?}"
    );
}

#[test]
fn rust_filter_naming_a_nested_module_path_is_clean() {
    let repo = git_repo();
    publish_base_layer(repo.path(), &["src/lib.rs", "src/plan/tests.rs"]);
    let criteria = [
        "cargo test --lib plan::tests::",
        "cargo test --lib -- plan::test",
    ];
    let metadata = plan(vec![stage("feature", StageType::Standard, &criteria)]);
    let found = matching(&metadata, Some(repo.path()), NO_MODULE);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn rust_filter_without_a_base_layer_leaves_one_note() {
    let repo = git_repo();
    let criteria = [
        "cargo test --lib first_mod::",
        "cargo test --lib second_mod::",
    ];
    let metadata = plan(vec![stage("feature", StageType::Standard, &criteria)]);
    let (findings, notes) = lint_with_notes(&metadata, Some(repo.path()));
    let filter_findings = findings.iter().filter(|f| f.message.contains(NO_MODULE));
    assert_eq!(filter_findings.count(), 0, "{findings:?}");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("no source-graph base layer"), "{notes:?}");
}

#[test]
fn knowledge_check_with_a_baseline_is_clean() {
    let repo = TempDir::new().expect("temp dir");
    write_knowledge(repo.path(), "mistakes.md", DUPLICATE_HEADING);
    let criteria = [
        "loom knowledge check --strict --baseline doc/knowledge-baseline.txt",
        "loom knowledge check --strict --baseline=doc/knowledge-baseline.txt",
        "loom knowledge check",
    ];
    let metadata = plan(vec![stage(
        "distill",
        StageType::KnowledgeDistill,
        &criteria,
    )]);
    assert!(matching(&metadata, Some(repo.path()), KNOWLEDGE).is_empty());
}

#[test]
fn strict_knowledge_check_on_a_clean_tree_is_clean() {
    let repo = TempDir::new().expect("temp dir");
    let clean = "# Patterns\n\n> Reusable patterns.\n\n## One\n\nSome prose.\n";
    write_knowledge(repo.path(), "patterns.md", clean);
    let check = ["loom knowledge check --strict"];
    let metadata = plan(vec![stage("knowledge", StageType::Knowledge, &check)]);
    assert!(matching(&metadata, Some(repo.path()), KNOWLEDGE).is_empty());
}

#[test]
fn strict_knowledge_check_outside_knowledge_stages_is_not_linted() {
    let repo = TempDir::new().expect("temp dir");
    write_knowledge(repo.path(), "mistakes.md", DUPLICATE_HEADING);
    let check = ["loom knowledge check --strict"];
    let metadata = plan(vec![stage("feature", StageType::Standard, &check)]);
    assert!(matching(&metadata, Some(repo.path()), KNOWLEDGE).is_empty());
}
