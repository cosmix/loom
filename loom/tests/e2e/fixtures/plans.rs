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
    - id: stage-2
      name: "Second Stage"
      description: "Build on first stage"
      dependencies:
        - stage-1
      acceptance:
        - "Build succeeds"
      files:
        - "Cargo.toml"
```

<!-- END loom METADATA -->

## Stage 1: First Stage

Initial setup stage that has no dependencies.

## Stage 2: Second Stage

Build on first stage - depends on stage-1 completing.
"#
    .to_string()
}

/// Returns a plan with stages in parallel groups
///
/// Stage 1 has no dependencies, stages 2 and 3 both depend on stage 1
/// and can run in parallel (same parallel group).
pub fn parallel_plan() -> String {
    r#"# PLAN: Parallel Execution Test

This plan demonstrates parallel stage execution.

## Overview

After Stage 1 completes, Stages 2 and 3 can execute in parallel.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Foundation Stage"
      description: "Setup foundation"
      dependencies: []
      acceptance:
        - "Foundation ready"
      files:
        - "README.md"
    - id: stage-2
      name: "Parallel Stage A"
      description: "First parallel task"
      dependencies:
        - stage-1
      parallel_group: "parallel-group-1"
      acceptance:
        - "Task A complete"
      files:
        - "src/module_a.rs"
    - id: stage-3
      name: "Parallel Stage B"
      description: "Second parallel task"
      dependencies:
        - stage-1
      parallel_group: "parallel-group-1"
      acceptance:
        - "Task B complete"
      files:
        - "src/module_b.rs"
```

<!-- END loom METADATA -->

## Stage 1: Foundation Stage

This stage sets up the foundation for the parallel work.

## Stage 2: Parallel Stage A

This stage can run in parallel with Stage 3.

## Stage 3: Parallel Stage B

This stage can run in parallel with Stage 2.
"#
    .to_string()
}

/// Returns a complex plan with mixed dependencies (diamond pattern)
///
/// Creates a diamond dependency pattern:
/// - Stage 1: No dependencies
/// - Stage 2: Depends on Stage 1
/// - Stage 3: Depends on Stage 1
/// - Stage 4: Depends on both Stage 2 and Stage 3
pub fn complex_plan() -> String {
    r#"# PLAN: Complex Dependencies Test

This plan demonstrates complex dependency patterns including a diamond structure.

## Overview

Stage 1 is the foundation. Stages 2 and 3 both depend on Stage 1 and can run
in parallel. Stage 4 depends on both Stages 2 and 3 completing.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Foundation"
      description: "Base foundation stage"
      dependencies: []
      acceptance:
        - "Foundation established"
      files:
        - "src/core.rs"
    - id: stage-2
      name: "Left Branch"
      description: "Left side of diamond"
      dependencies:
        - stage-1
      parallel_group: "branches"
      acceptance:
        - "Left branch complete"
      files:
        - "src/left.rs"
    - id: stage-3
      name: "Right Branch"
      description: "Right side of diamond"
      dependencies:
        - stage-1
      parallel_group: "branches"
      acceptance:
        - "Right branch complete"
      files:
        - "src/right.rs"
    - id: stage-4
      name: "Convergence"
      description: "Merges both branches"
      dependencies:
        - stage-2
        - stage-3
      acceptance:
        - "Integration complete"
      files:
        - "src/integration.rs"
```

<!-- END loom METADATA -->

## Stage 1: Foundation

The base foundation that all other stages build upon.

## Stage 2: Left Branch

Processes the left side of the workflow.

## Stage 3: Right Branch

Processes the right side of the workflow.

## Stage 4: Convergence

Brings together the results from both branches.
"#
    .to_string()
}

/// Returns a stage with comprehensive acceptance criteria
///
/// A single-stage plan with multiple acceptance criteria for testing
/// the acceptance verification system.
pub fn stage_with_acceptance() -> String {
    r#"# PLAN: Acceptance Criteria Test

This plan has a single stage with multiple acceptance criteria.

## Overview

Tests the acceptance criteria verification system.

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Quality Gate"
      description: "Stage with comprehensive acceptance criteria"
      dependencies: []
      acceptance:
        - "cargo test --all"
        - "cargo clippy -- -D warnings"
        - "cargo fmt --check"
        - "cargo doc --no-deps"
      files:
        - "src/**/*.rs"
        - "tests/**/*.rs"
        - "Cargo.toml"
```

<!-- END loom METADATA -->

## Stage 1: Quality Gate

This stage must pass all quality checks:

1. All tests pass
2. No clippy warnings
3. Code is properly formatted
4. Documentation builds successfully
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
```

<!-- END loom METADATA -->

## Stage 1: Minimal Stage

A minimal stage with only required fields.
"#
    .to_string()
}

/// Returns a plan with a long sequential chain
///
/// Creates 5 stages in a strict sequential order where each stage
/// depends on the previous one.
pub fn long_sequential_chain() -> String {
    r#"# PLAN: Long Sequential Chain

A plan with a long chain of sequential dependencies.

## Overview

5 stages that must execute in strict order: 1 -> 2 -> 3 -> 4 -> 5

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Stage 1"
      description: "First in chain"
      dependencies: []
      acceptance:
        - "Stage 1 complete"
    - id: stage-2
      name: "Stage 2"
      description: "Second in chain"
      dependencies:
        - stage-1
      acceptance:
        - "Stage 2 complete"
    - id: stage-3
      name: "Stage 3"
      description: "Third in chain"
      dependencies:
        - stage-2
      acceptance:
        - "Stage 3 complete"
    - id: stage-4
      name: "Stage 4"
      description: "Fourth in chain"
      dependencies:
        - stage-3
      acceptance:
        - "Stage 4 complete"
    - id: stage-5
      name: "Stage 5"
      description: "Fifth in chain"
      dependencies:
        - stage-4
      acceptance:
        - "Stage 5 complete"
```

<!-- END loom METADATA -->

## Stages

Each stage depends on the previous one completing and being verified.
"#
    .to_string()
}
