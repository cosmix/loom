//! Tests for `extract_stage_definition`'s field-mapping behavior and error
//! paths, moved here from `commands/run/tests.rs` - `extract_stage_definition`
//! lives in `fs::stage_loading`, so its tests belong beside it.
//!
//! The three field-mapping tests originally hand-wrote minimal, plan-shaped
//! frontmatter (missing `status` and other runtime-only keys) - exactly the
//! assumption the reported bug was made of, since `extract_stage_definition`
//! now parses a full `Stage`, not a bare `StageDefinition`. They now build
//! fixtures via [`stage_file_content`](super::tests::stage_file_content), the
//! same fixture builder `tests_stage_loading.rs` uses, which serializes a
//! real `Stage` the way `serialize_stage_to_markdown` writes an actual
//! `.loom/work/stages/*.md` file.

use super::tests::stage_file_content;
use super::*;
use crate::models::stage::{AcceptanceCriterion, StageType, TruthCheck};

#[test]
fn test_extract_stage_definition_valid() {
    let content = stage_file_content("stage-1", "Test Stage", |_stage| {});

    let result = extract_stage_definition(&content);

    assert!(result.is_ok());
    let def = result.unwrap();
    assert_eq!(def.id, "stage-1");
    assert_eq!(def.name, "Test Stage");
    assert_eq!(def.dependencies.len(), 0);
}

#[test]
fn test_extract_stage_definition_with_fields() {
    let content = stage_file_content("stage-2", "Complex Stage", |stage| {
        stage.description = Some("A complex stage".to_string());
        stage.working_dir = Some("loom".to_string());
        stage.dependencies = vec!["stage-1".to_string()];
        stage.parallel_group = Some("core".to_string());
        stage.acceptance = vec![AcceptanceCriterion::Simple("cargo test".to_string())];
        stage.setup = vec!["cargo build".to_string()];
        stage.files = vec!["src/*.rs".to_string()];
    });

    let result = extract_stage_definition(&content);

    assert!(result.is_ok());
    let def = result.unwrap();
    assert_eq!(def.id, "stage-2");
    assert_eq!(def.description, Some("A complex stage".to_string()));
    assert_eq!(def.working_dir, "loom");
    assert_eq!(def.dependencies, vec!["stage-1".to_string()]);
    assert_eq!(def.parallel_group, Some("core".to_string()));
    assert_eq!(def.acceptance.len(), 1);
    assert_eq!(def.setup.len(), 1);
    assert_eq!(def.files.len(), 1);
}

/// Minimal `TruthCheck` fixture for the `before_stage`/`after_stage`
/// overrides below.
fn truth_check(command: &str) -> TruthCheck {
    TruthCheck {
        command: command.to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: None,
        exit_code: None,
        description: None,
    }
}

/// Regression test for A-10: the old `StageFrontmatter` intermediate struct
/// hardcoded `stage_type`, `auto_merge`, `sandbox`, `context_ceiling_tokens`, and
/// `before_stage`/`after_stage` to defaults, silently dropping them on every
/// daemon restart (the loader prefers stage files over the plan).
/// `definition_from_stage` must preserve all of them.
#[test]
fn test_extract_stage_definition_preserves_previously_dropped_fields() {
    let content = stage_file_content("kn-stage", "Knowledge Stage", |stage| {
        stage.stage_type = StageType::Knowledge;
        stage.auto_merge = Some(false);
        stage.context_ceiling_tokens = Some(50);
        stage.plan_overview = Some(false);
        stage.before_stage = vec![truth_check("echo pre")];
        stage.after_stage = vec![truth_check("echo post")];
        stage.sandbox.enabled = Some(false);
    });

    let def = extract_stage_definition(&content).expect("should deserialize");

    assert_eq!(
        def.stage_type,
        Some(StageType::Knowledge),
        "stage_type must survive"
    );
    assert_eq!(def.auto_merge, Some(false), "auto_merge must survive");
    assert_eq!(
        def.context_ceiling_tokens,
        Some(50),
        "context_ceiling_tokens must survive"
    );
    assert_eq!(def.plan_overview, Some(false), "plan_overview must survive");
    assert_eq!(def.before_stage.len(), 1, "before_stage must survive");
    assert_eq!(def.after_stage.len(), 1, "after_stage must survive");
    assert_eq!(
        def.sandbox.enabled,
        Some(false),
        "sandbox override must survive"
    );
}

#[test]
fn test_extract_stage_definition_no_delimiter() {
    let content = "No frontmatter here";

    let result = extract_stage_definition(content);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("frontmatter"));
}

#[test]
fn test_extract_stage_definition_not_closed() {
    let content = "---\nid: test\nname: Test\n\nNo closing delimiter";

    let result = extract_stage_definition(content);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("not properly closed"));
}

#[test]
fn test_extract_stage_definition_invalid_yaml() {
    let content = "---\ninvalid: yaml: content:\n---\n";

    let result = extract_stage_definition(content);

    assert!(result.is_err());
}
