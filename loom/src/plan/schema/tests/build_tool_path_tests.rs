//! Build-tool config-file resolution tests, split out of `validation_tests.rs`
//! purely to keep that file under the line-count ceiling.

use super::make_stage;
use crate::plan::schema::types::{AcceptanceCriterion, RegressionTest, WiringCheck};
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

#[test]
fn build_tool_checks_use_each_lexed_invocation_and_quoted_selector() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("crate dir")).unwrap();
    std::fs::write(dir.path().join("crate dir/Cargo.toml"), "").unwrap();
    let mut stage = make_stage("stage-1", "Stage One");
    stage.artifacts = vec!["README.md".to_string()];
    for command in [
        "cargo test --manifest-path 'crate dir/Cargo.toml'",
        "cargo test --manifest-path='crate dir/Cargo.toml'",
        "printf '%s' 'cargo test'",
        "echo --manifest-path missing && cargo test --manifest-path 'crate dir/Cargo.toml'",
    ] {
        stage.acceptance = vec![AcceptanceCriterion::Simple(command.to_string())];
        let warnings = validate_structural_preflight(&[stage.clone()], Some(dir.path()));
        assert!(
            warnings.iter().all(|w| !w.contains("not found")),
            "{command}: {warnings:?}"
        );
    }
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        "echo --manifest-path 'crate dir/Cargo.toml' && cargo test".to_string(),
    )];
    let warnings = validate_structural_preflight(&[stage], Some(dir.path()));
    assert!(
        warnings.iter().any(|w| w.contains("Cargo.toml not found")),
        "{warnings:?}"
    );
}

#[test]
fn redundant_working_directory_requires_a_component_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = "src".to_string();
    stage.artifacts = vec!["src_extra/file".to_string(), "src/file".to_string()];
    let warnings = validate_structural_preflight(&[stage], Some(dir.path()));
    let found: Vec<_> = warnings
        .iter()
        .filter(|w| w.contains("redundant working_dir prefix"))
        .collect();
    assert_eq!(found.len(), 1, "{warnings:?}");
    assert!(found[0].contains("src/file"));
}

#[test]
fn wiring_and_regression_paths_require_a_component_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = "src".to_string();
    stage.wiring = ["src_extra/file", "src/file"]
        .into_iter()
        .map(|source| WiringCheck {
            source: source.to_string(),
            pattern: "feature_function".to_string(),
            description: "Feature is wired".to_string(),
            literal: false,
        })
        .collect();
    for (path, expected) in [("src_extra/test.rs", false), ("src/test.rs", true)] {
        stage.regression_test = Some(RegressionTest {
            file: path.to_string(),
            must_contain: vec!["test_bug_fixed".to_string()],
        });
        let warnings = validate_structural_preflight(&[stage.clone()], Some(dir.path()));
        let prefix_warnings: Vec<_> = warnings
            .iter()
            .filter(|warning| warning.contains("redundant working_dir prefix"))
            .collect();
        assert_eq!(
            prefix_warnings.len(),
            1 + usize::from(expected),
            "{warnings:?}"
        );
        assert!(prefix_warnings
            .iter()
            .any(|warning| warning.contains("src/file")));
        assert_eq!(
            prefix_warnings.iter().any(|warning| warning.contains(path)),
            expected
        );
    }
}
