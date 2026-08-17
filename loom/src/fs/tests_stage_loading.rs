//! Tests for `fs/stage_loading.rs`.
//!
//! `definition_from_stage` is private to this module, so its two non-trivial
//! field mappings (the `working_dir` `Option<String>` -> `String` fallback,
//! and the `stage_type` non-optional -> `Option` wrap) are unit-tested
//! directly here via `super::*`. This file also owns [`stage_file_content`],
//! the shared fixture builder for a realistic `.work/stages/*.md` file
//! (`pub(super)` so the sibling `tests_stage_loading_fields.rs` and
//! `tests_stage_loading_round_trip.rs` modules can reuse it), plus the direct
//! regression guard for the reported bug (a realistic runtime stage file must
//! parse instead of erroring "Failed to parse StageDefinition from
//! frontmatter"). `extract_stage_definition`'s field-mapping/error-path tests
//! live in `tests_stage_loading_fields.rs` and the full round-trip fidelity
//! test lives in `tests_stage_loading_round_trip.rs` - both split out to stay
//! under the project's file/function line-count ceilings.

use super::*;
use crate::models::stage::{StageStatus, StageType};
use crate::verify::serialize_stage_to_markdown;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_definition_from_stage_defaults_missing_working_dir_to_root() {
    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.working_dir = None;

    let def = definition_from_stage(&stage);

    assert_eq!(def.working_dir, ".");
}

#[test]
fn test_definition_from_stage_preserves_explicit_working_dir() {
    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.working_dir = Some("loom".to_string());

    let def = definition_from_stage(&stage);

    assert_eq!(def.working_dir, "loom");
}

#[test]
fn test_definition_from_stage_wraps_stage_type_in_some() {
    let mut stage = Stage::new("Test Stage".to_string(), None);
    stage.stage_type = StageType::IntegrationVerify;

    let def = definition_from_stage(&stage);

    assert_eq!(def.stage_type, Some(StageType::IntegrationVerify));
}

/// Build markdown+frontmatter for a stage file the way `serialize_stage_to_markdown`
/// writes a real `.work/stages/*.md` file: a fully-populated [`Stage`] (every
/// runtime field present, e.g. `status`, `created_at`), with `configure`
/// applied on top to set the fields under test.
///
/// `extract_stage_definition` parses this shape, not a bare `StageDefinition`,
/// so tests must build fixtures this way rather than hand-writing plan-style
/// partial YAML.
pub(super) fn stage_file_content(
    id: &str,
    name: &str,
    configure: impl FnOnce(&mut Stage),
) -> String {
    let mut stage = Stage::new(name.to_string(), None);
    stage.id = id.to_string();
    configure(&mut stage);
    serialize_stage_to_markdown(&stage).unwrap()
}

/// Direct regression guard for the reported bug: a realistic runtime stage
/// file - frontmatter carrying runtime-only keys like `status`, `merged`,
/// `fix_attempts`, `verification_status`, `resolved_base`, `session`, and
/// `worktree` - must parse instead of erroring with "Failed to parse
/// StageDefinition from frontmatter".
#[test]
fn test_extract_stage_definition_parses_realistic_runtime_stage_file() {
    // `verification_status` needs no explicit override: `Stage::new` already
    // populates it (and every other runtime-only field this test targets,
    // like `dispute_count`/`evidence_rounds`), so it always appears in the
    // serialized frontmatter alongside the fields set below.
    let content = stage_file_content("real-stage", "Real Runtime Stage", |stage| {
        stage.status = StageStatus::Completed;
        stage.merged = true;
        stage.fix_attempts = 2;
        stage.resolved_base = Some("loom/_base/real-stage".to_string());
        stage.session = Some("session-123".to_string());
        stage.worktree = Some(".worktrees/real-stage".to_string());
        stage.plan_id = Some("plan-1".to_string());
        stage.dispute_count = 1;
        stage.evidence_rounds = 1;
        stage.amendments_applied = 1;
    });

    let result = extract_stage_definition(&content);

    assert!(
        result.is_ok(),
        "realistic runtime stage file must parse: {:?}",
        result.err()
    );
    let def = result.unwrap();
    assert_eq!(def.id, "real-stage");
    assert_eq!(def.name, "Real Runtime Stage");
}

#[test]
fn test_load_stages_from_work_dir_parses_realistic_stage_files() {
    let temp_dir = TempDir::new().unwrap();
    let stages_dir = temp_dir.path().join("stages");
    fs::create_dir(&stages_dir).unwrap();

    let stage_content = stage_file_content("stage-1", "Test Stage", |_stage| {});
    fs::write(stages_dir.join("0-stage-1.md"), stage_content).unwrap();

    let result = load_stages_from_work_dir(&stages_dir);

    assert!(result.is_ok());
    let stages = result.unwrap();
    assert_eq!(stages.len(), 1);
    assert_eq!(stages[0].id, "stage-1");
}
