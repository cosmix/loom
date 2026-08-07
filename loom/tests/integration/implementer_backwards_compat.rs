//! Backwards-compatibility tests for the `implementer` field.
//!
//! `implementer` was added after `ultracode` with `#[serde(default)]` as the
//! sole compatibility mechanism (no version stamp, no migration pass — the
//! project forbids those at this stage). These tests prove that mechanism
//! actually holds for both entry points a pre-existing installation hits:
//! a plan file authored before the field existed, and a `.work/stages/*.md`
//! file written to disk by a previous loom build.

use std::fs;
use tempfile::TempDir;

use loom::models::stage::Implementer;
use loom::plan::parser::parse_plan;
use loom::verify::transitions::load_stage;

/// A multi-stage plan with no `implementer` key anywhere in the YAML,
/// as any plan written before this field existed would look.
fn plan_without_implementer_field() -> &'static str {
    r#"# Legacy Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-a
      name: "Stage A"
      working_dir: "."
      acceptance:
        - "true"
    - id: stage-b
      name: "Stage B"
      working_dir: "."
      acceptance:
        - "true"
      dependencies:
        - stage-a
```

<!-- END loom METADATA -->
"#
}

#[test]
fn implementer_backwards_compat_plan_yaml_without_field_parses() {
    let temp = TempDir::new().unwrap();
    let plan_path = temp.path().join("PLAN-legacy.md");
    fs::write(&plan_path, plan_without_implementer_field()).unwrap();

    let parsed = parse_plan(&plan_path).expect("plan without implementer key should parse");

    assert_eq!(parsed.stages.len(), 2);
    for stage in &parsed.stages {
        assert_eq!(
            stage.implementer,
            Implementer::Claude,
            "stage '{}' should default to Implementer::Claude when the key is absent",
            stage.id
        );
    }
}

/// A `.work/stages/01-legacy-stage.md` file whose YAML frontmatter has no
/// `implementer` key, as a stage file written by a previous loom build
/// (before this field existed) would look on disk.
fn stage_file_without_implementer_field() -> &'static str {
    r#"---
id: legacy-stage
name: Legacy Stage
description: null
status: queued
dependencies: []
parallel_group: null
acceptance: []
files: []
plan_id: null
worktree: null
session: null
parent_stage: null
child_stages: []
created_at: 2026-01-01T00:00:00Z
updated_at: 2026-01-01T00:00:00Z
completed_at: null
close_reason: null
working_dir: "."
retry_count: 0
max_retries: null
last_failure_at: null
resolved_base: null
base_branch: null
merged: false
merge_conflict: false
---

# Stage: Legacy Stage
"#
}

#[test]
fn implementer_backwards_compat_stage_file_without_field_loads() {
    let temp = TempDir::new().unwrap();
    let stages_dir = temp.path().join("stages");
    fs::create_dir_all(&stages_dir).unwrap();
    fs::write(
        stages_dir.join("01-legacy-stage.md"),
        stage_file_without_implementer_field(),
    )
    .unwrap();

    let stage = load_stage("legacy-stage", temp.path())
        .expect("a stage file predating the implementer field should still load");

    assert_eq!(
        stage.implementer,
        Implementer::Claude,
        "a stage file written before the implementer field existed must load as Claude"
    );
}
