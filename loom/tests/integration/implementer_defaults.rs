//! Default-lane tests for the `implementers` field.
//!
//! `implementers` carries `#[serde(default)]` on both structs that hold it, so
//! omitting the key yields `["claude"]` rather than an empty set. These tests
//! drive that default through the two real entry points rather than through the
//! struct: a plan file whose YAML never mentions the key, and a
//! `.work/stages/*.md` file loaded back off disk. The stage-file path is the one
//! that has no other coverage — every other test constructs `Stage` in memory,
//! where a wrong default would be masked by the constructor.

use std::fs;
use tempfile::TempDir;

use loom::models::stage::Implementer;
use loom::plan::parser::parse_plan;
use loom::verify::transitions::load_stage;

/// A multi-stage plan with no `implementers` key anywhere in the YAML.
fn plan_without_implementers_field() -> &'static str {
    r#"# Plan

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
fn plan_yaml_without_implementers_defaults_to_claude_lane() {
    let temp = TempDir::new().unwrap();
    let plan_path = temp.path().join("PLAN-default.md");
    fs::write(&plan_path, plan_without_implementers_field()).unwrap();

    let parsed = parse_plan(&plan_path).expect("plan without implementers key should parse");

    assert_eq!(parsed.stages.len(), 2);
    for stage in &parsed.stages {
        assert_eq!(
            stage.implementers.preferred(),
            Implementer::Claude,
            "stage '{}' should prefer the Claude lane when the key is absent",
            stage.id
        );
        assert!(
            !stage.implementers.includes_codex(),
            "stage '{}' must not license codex without an explicit opt-in — \
             the codex safety doctrine is gated on exactly this",
            stage.id
        );
    }
}

/// A `.work/stages/01-default-stage.md` file whose YAML frontmatter has no
/// `implementers` key, as a stage file written for a plan that never set one
/// would look on disk.
fn stage_file_without_implementers_field() -> &'static str {
    r#"---
id: default-stage
name: Default Stage
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

# Stage: Default Stage
"#
}

#[test]
fn stage_file_without_implementers_loads_with_claude_lane() {
    let temp = TempDir::new().unwrap();
    let stages_dir = temp.path().join("stages");
    fs::create_dir_all(&stages_dir).unwrap();
    fs::write(
        stages_dir.join("01-default-stage.md"),
        stage_file_without_implementers_field(),
    )
    .unwrap();

    let stage = load_stage("default-stage", temp.path())
        .expect("a stage file with no implementers key should load");

    assert_eq!(stage.implementers.preferred(), Implementer::Claude);
    assert!(!stage.implementers.includes_codex());
    assert!(!stage.implementers.is_mixed());
}

/// The same stage file, but with a mixed lane list written into frontmatter —
/// the round-trip a stage gets when its plan declares both lanes.
#[test]
fn stage_file_with_mixed_implementers_round_trips() {
    let temp = TempDir::new().unwrap();
    let stages_dir = temp.path().join("stages");
    fs::create_dir_all(&stages_dir).unwrap();
    let with_lanes = stage_file_without_implementers_field().replace(
        "merge_conflict: false\n",
        "merge_conflict: false\nimplementers:\n  - codex\n  - claude\n",
    );
    fs::write(stages_dir.join("01-default-stage.md"), with_lanes).unwrap();

    let stage = load_stage("default-stage", temp.path())
        .expect("a stage file with a mixed lane list should load");

    assert!(stage.implementers.is_mixed());
    assert!(stage.implementers.includes_codex());
    assert!(stage.implementers.includes_claude());
    assert_eq!(
        stage.implementers.preferred(),
        Implementer::Codex,
        "preference order must survive the disk round-trip"
    );
}
