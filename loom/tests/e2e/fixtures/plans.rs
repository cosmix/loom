//! Plan fixture generators for E2E tests
//!
//! Provides pre-built plan content strings with valid loom METADATA blocks

/// Returns a simple sequential plan with 2 stages
///
/// Stage 2 depends on stage 1, forming a simple sequential dependency chain.
pub fn simple_sequential_plan() -> String {
    r#"# PLAN: Simple Sequential Test

This is a simple test plan with two sequential stages.

## Overview

Stage 1 must complete before Stage 2 can begin.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "First Stage"
      description: "Initial setup stage"
      dependencies: []
      acceptance:
        - "Setup complete"
      files:
        - "src/main.rs"
      working_dir: "."
    - id: stage-2
      name: "Second Stage"
      description: "Build on first stage"
      dependencies:
        - stage-1
      acceptance:
        - "Build succeeds"
      files:
        - "Cargo.toml"
      working_dir: "."
```

<!-- END loom METADATA -->

## Stage 1: First Stage

Initial setup stage that has no dependencies.

## Stage 2: Second Stage

Build on first stage - depends on stage-1 completing.
"#
    .to_string()
}

/// Returns a minimal valid plan
///
/// Single stage with minimal required fields, useful for basic tests.
pub fn minimal_plan() -> String {
    r#"# PLAN: Minimal Test

Minimal valid plan for testing.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Minimal Stage"
      working_dir: "."
```

<!-- END loom METADATA -->

## Stage 1: Minimal Stage

A minimal stage with only required fields.
"#
    .to_string()
}

