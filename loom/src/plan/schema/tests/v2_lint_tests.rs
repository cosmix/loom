//! `plan verify` lints (DESIGN D4): the named cases. Each lint's clean twin
//! lives in the sibling files declared below.
//!
//! Assertions select one lint's findings by message, so a finding another lint
//! raises on the same command does not disturb them.

use std::path::Path;

use tempfile::TempDir;

use crate::context::extract::file_node;
use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
use crate::context::source_graph::{FileCoverage, NodeLanguage};
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::git::run_git_checked;
use crate::plan::schema::validation::v2_fields::split_lint_findings;
use crate::plan::schema::validation::v2_lints::{run, LintContext, LintFinding};
use crate::plan::schema::{
    AcceptanceCriterion, LoomConfig, LoomMetadata, StageDefinition, StageType,
};

#[path = "v2_lint_command_tests.rs"]
mod command_lints;
#[path = "v2_lint_repo_tests.rs"]
mod repo_lints;

fn stage(id: &str, stage_type: StageType, acceptance: &[&str]) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        working_dir: ".".to_string(),
        stage_type: Some(stage_type),
        acceptance: acceptance
            .iter()
            .map(|command| AcceptanceCriterion::Simple(command.to_string()))
            .collect(),
        ..Default::default()
    }
}

fn plan(stages: Vec<StageDefinition>) -> LoomMetadata {
    LoomMetadata {
        loom: LoomConfig {
            version: 2,
            stages,
            ..Default::default()
        },
    }
}

/// Every finding and note for `metadata` linted against `repo_root`.
fn lint_with_notes(
    metadata: &LoomMetadata,
    repo_root: Option<&Path>,
) -> (Vec<LintFinding>, Vec<String>) {
    let mut notes = Vec::new();
    let ctx = LintContext {
        metadata,
        repo_root,
    };
    let findings = run(&ctx, &mut notes);
    (findings, notes)
}

/// The findings whose message contains `needle`.
fn matching(metadata: &LoomMetadata, repo_root: Option<&Path>, needle: &str) -> Vec<LintFinding> {
    let (findings, _) = lint_with_notes(metadata, repo_root);
    findings
        .into_iter()
        .filter(|finding| finding.message.contains(needle))
        .collect()
}

/// A git repository with one empty commit.
fn git_repo() -> TempDir {
    let dir = TempDir::new().expect("temp dir");
    run_git_checked(&["init", "-q"], dir.path()).expect("git init");
    let identity = ["-c", "user.name=lint", "-c", "user.email=lint@example.com"];
    let commit = [
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "init",
    ];
    let args: Vec<&str> = identity.iter().chain(&commit).copied().collect();
    run_git_checked(&args, dir.path()).expect("git commit");
    dir
}

/// Publish a base layer for the repository's HEAD holding one file node per path.
fn publish_base_layer(repo: &Path, paths: &[&str]) {
    let head = run_git_checked(&["rev-parse", "HEAD"], repo).expect("HEAD");
    let work_dir = WorkDir::new(repo).expect("work dir");
    let store = ContextStore::open(&work_dir).expect("context store");
    let mut layer = GraphLayer {
        revision: head.clone(),
        ..Default::default()
    };
    for path in paths {
        let language = NodeLanguage::Rust;
        let node = file_node(
            Path::new(path),
            b"",
            language,
            "test".into(),
            &FileCoverage::Full,
        );
        let entry = FileEntry {
            nodes: vec![node],
            ..Default::default()
        };
        layer.files.insert(path.to_string(), entry);
    }
    let published = GraphStore::new(store.root(), work_dir.root()).publish_base(&head, &layer);
    assert!(
        published.expect("publish base layer"),
        "a fresh repository has no base layer"
    );
}

/// A tier-1 knowledge file with one `## ` heading twice: a structural issue.
const DUPLICATE_HEADING: &str =
    "# Mistakes\n\n> Lessons learned.\n\n## Foo\n\nOne.\n\n## Foo\n\nTwo.\n";

/// Write a knowledge file under the repository's knowledge root.
fn write_knowledge(repo: &Path, name: &str, body: &str) {
    let root = repo.join("doc/loom/knowledge");
    std::fs::create_dir_all(&root).expect("knowledge root");
    std::fs::write(root.join(name), body).expect("knowledge file");
}

#[test]
fn unknown_loom_subcommand_is_error_in_v2() {
    let distill = stage(
        "distill",
        StageType::KnowledgeDistill,
        &["loom knowledge verify"],
    );
    let metadata = plan(vec![distill]);
    let needle = "`verify` is not a subcommand of `loom knowledge`";
    let found = matching(&metadata, None, needle);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].stage_id.as_deref(), Some("distill"));
    assert!(found[0].error_in_v2);
    let (errors, warnings) = split_lint_findings(metadata.loom.version, found);
    assert_eq!(errors.len(), 1, "the v2 path maps the finding to an error");
    assert!(errors[0].to_string().starts_with("Stage 'distill': "));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn dash_leading_rg_pattern_is_flagged() {
    let criterion = r#"rg -qF "--out" src/x.rs"#;
    let metadata = plan(vec![stage("feature", StageType::Standard, &[criterion])]);
    let needle = "passes the pattern `--out` where rg reads it as a flag";
    let found = matching(&metadata, None, needle);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].error_in_v2);
}

#[test]
fn rust_filter_matching_no_module_warns() {
    let repo = git_repo();
    publish_base_layer(repo.path(), &["src/lib.rs", "src/present_mod.rs"]);
    let criterion = "cargo test --lib nonexistent_mod::";
    let metadata = plan(vec![stage("feature", StageType::Standard, &[criterion])]);
    let (findings, notes) = lint_with_notes(&metadata, Some(repo.path()));
    let found: Vec<_> = findings
        .iter()
        .filter(|finding| {
            finding
                .message
                .contains("no module `nonexistent_mod` exists")
        })
        .collect();
    assert_eq!(found.len(), 1, "{findings:?}");
    assert!(
        !found[0].error_in_v2,
        "the filter lint is a warning in every version"
    );
    assert!(notes.is_empty(), "the base layer was readable: {notes:?}");
}

#[test]
fn knowledge_strict_check_without_baseline_is_flagged() {
    let repo = TempDir::new().expect("temp dir");
    write_knowledge(repo.path(), "mistakes.md", DUPLICATE_HEADING);
    let distill = stage(
        "distill",
        StageType::KnowledgeDistill,
        &["loom knowledge check --strict"],
    );
    let metadata = plan(vec![distill]);
    let found = matching(
        &metadata,
        Some(repo.path()),
        "structural issue in the knowledge tree",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].error_in_v2);
}

#[test]
fn network_binary_without_domains_is_error_in_v2() {
    let metadata = plan(vec![stage(
        "feature",
        StageType::Standard,
        &["curl https://x"],
    )]);
    let needle = "runs `curl` while the stage's sandbox allows no network domain";
    let found = matching(&metadata, None, needle);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].error_in_v2);
}
