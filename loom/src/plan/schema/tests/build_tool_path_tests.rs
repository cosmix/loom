//! Build-tool config-file resolution tests, split out of `validation_tests.rs`
//! purely to keep that file under the line-count ceiling.

use super::make_stage;
use crate::plan::schema::types::AcceptanceCriterion;
use crate::plan::schema::validation::validate_structural_preflight;

#[test]
fn test_preflight_manifest_path_resolves_against_working_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sub_dir = dir.path().join("loom");
    std::fs::create_dir(&sub_dir).expect("create working_dir");
    std::fs::write(sub_dir.join("Cargo.toml"), "[package]\nname = \"loom\"\n")
        .expect("write manifest");

    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = "loom".to_string();
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        "cargo test --manifest-path Cargo.toml".to_string(),
    )];
    stage.artifacts = vec!["README.md".to_string()];

    let warnings = validate_structural_preflight(&[stage], Some(dir.path()));
    assert!(
        warnings.iter().all(|w| !w.contains("not found")),
        "{warnings:?}"
    );
}
