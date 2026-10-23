# Strengthen Verification: Three-Layer Defense Against Unwired, Duplicated, and Blind Code

## Context

Loom's integration-verify stages consistently catch the same three classes of issues:

1. **Unwired/dead code** — Stages write well-tested code that compiles and passes acceptance, but is never registered, mounted, or imported. The feature is unreachable.
2. **Duplicated code** — Stages implement functionality that already exists elsewhere in the codebase because they didn't look for existing implementations first.
3. **Insufficient understanding** — Stages modify areas they don't fully understand, creating inconsistencies with existing patterns.

These are **prevention problems** that the current system only catches at the **detection layer** (integration-verify). By then, fixes are expensive and context is exhausted. This plan adds enforcement at three layers: plan-time, stage-completion, and integration-verify.

## Proposed Changes

### Layer 1: Plan-Time Validation (`loom init`)

**Goal:** Catch missing verification declarations before execution starts.

#### 1a. Artifact → Wiring Coherence Rule

**File:** `loom/src/plan/schema/validation.rs` — new function `check_artifact_wiring_coherence()`

If a standard stage declares `artifacts` (new files), validate that it also has at least one `wiring` or `wiring_test` proving those files are connected. Emit a **warning** (not error) via `validate_structural_preflight()` because:

- The wiring might happen in a downstream stage
- But the plan author should explicitly acknowledge this

If the stage has **no downstream dependents** (it's a leaf in the DAG), escalate to a validation **error** in `validate()` — leaf stages that create files MUST prove they're wired.

```text
Example warning:
  Stage 'add-cache-layer': has 3 artifacts but no wiring checks.
  If wiring happens in a downstream stage, add a wiring check there.
  If this stage wires its own code, add wiring checks proving integration.
```

#### 1b. Cross-Stage Wiring Coverage

**File:** `loom/src/plan/schema/validation.rs` — new function `check_cross_stage_wiring_coverage()`

For each stage with artifacts, walk the dependency chain forward. If neither the stage itself nor any of its direct dependents (or the integration-verify stage) have wiring checks referencing those artifacts, warn:

```text
  Stage 'add-cache-layer' creates artifacts but no stage in the dependency
  chain verifies wiring. Add wiring checks to this stage or a dependent.
```

This requires building a simple forward-dependency map (reverse of the existing dependency graph).

#### 1c. before/after Recommendation for Feature Stages

**File:** `loom/src/plan/schema/validation.rs` — extend `validate_structural_preflight()`

If a standard stage has artifacts AND acceptance criteria (it's a feature stage), but no `before_stage`/`after_stage` checks, warn:

```text
  Stage 'add-cache-layer': Consider adding before_stage (test should fail
  pre-feature) and after_stage (test should pass post-feature) for delta
  verification.
```

This is purely advisory — before/after is the strongest verification but we can't require it for all stages.

---

### Layer 2: Stage Completion (`loom stage complete`)

**Goal:** Detect unwired code and enforce communication about downstream wiring needs.

#### 2a. New File Wiring Detection

**New module:** `loom/src/verify/wiring_detection.rs`

At stage completion (after acceptance passes, before merge), detect new files and check if they're referenced:

1. Run `git diff --name-status <base-branch>..HEAD` to find newly added files (status `A`)
2. Filter to source files (`.rs`, `.ts`, `.py`, `.go`, `.js`, `.tsx`, `.jsx`)
3. For each new source file, extract its "importable name" (module name, file stem)
4. Search the worktree for references to that name in other files (simple grep)
5. Files that exist but are never referenced = **potentially unwired**

**Integration point:** `loom/src/commands/stage/complete.rs` — `run_verification_phase()`, after goal-backward verification passes.

**Behavior:**

- **Unwired files + no downstream dependents** → ERROR: "New file `src/cache.rs` is not imported or referenced anywhere. Wire it in or remove it."
- **Unwired files + has downstream dependents** → WARNING + memory check (see 2b)
- **All files referenced** → PASS silently

**Language-specific reference detection:**

- Rust: `mod <name>`, `use ...<name>`, `use crate::<path>::<name>`
- TypeScript/JS: `import.*from.*<name>`, `require.*<name>`
- Python: `import <name>`, `from <name>`
- Go: `import.*<name>`

This doesn't need to be perfect — false negatives (missing a reference) are fine. We're catching the obvious case where a file is completely orphaned.

#### 2b. Memory Enforcement for Downstream Wiring

**File:** `loom/src/commands/stage/complete.rs` — extend `run_verification_phase()`

If 2a detects unwired files AND the stage has downstream dependents:

1. Read the stage's memory entries from `.work/memory/<stage-id>.md`
2. Check if any memory entry mentions the unwired files (simple substring match on file names)
3. If no memory entries reference the unwired files → ERROR:

```text
  New files not yet wired: src/cache.rs, src/cache/store.rs
  Since downstream stages will wire these, you MUST record memory notes
  explaining what needs to happen:
    loom memory note "src/cache.rs needs to be registered in src/main.rs
    via mod cache; and added to the command dispatch in src/commands/mod.rs"
```

This forces the agent to leave a paper trail for downstream stages, preventing the "I wrote it but nobody knows it exists" problem.

#### 2c. Duplicate Symbol Detection

**New module:** `loom/src/verify/duplicate_detection.rs`

At stage completion, check if newly defined symbols duplicate existing ones:

1. For each new/modified source file, extract top-level symbol names:
   - Rust: `pub fn <name>`, `pub struct <name>`, `pub enum <name>`, `pub trait <name>`
   - TS/JS: `export function <name>`, `export class <name>`, `export const <name>`
   - Python: `def <name>`, `class <name>`
2. Search the rest of the codebase (excluding the new files) for same-named symbols
3. Report matches as **warnings** (not errors — name collisions can be intentional):

```text
  Warning: New function `parse_config` in src/cache/config.rs may duplicate
  existing `parse_config` in src/config/parser.rs:42. Verify this is intentional.
```

**Integration point:** `loom/src/commands/stage/complete.rs` — `run_verification_phase()`, advisory only.

This is a best-effort heuristic. It won't catch semantic duplication (different names, same behavior), but it catches the common case where an agent reimplements something without knowing it exists.

---

### Layer 3: Integration-Verify Strengthening

**Goal:** Give integration-verify agents better context and enforce structured code review.

#### 3a. Cross-Stage Change Summary in Signal

**File:** `loom/src/orchestrator/signals/generate.rs` and `format/` — extend signal content for integration-verify stages

When generating the signal for an integration-verify stage, include a **change summary** section:

```markdown
## Cross-Stage Changes

### Stage: add-cache-layer (completed)

Files added: src/cache.rs, src/cache/store.rs, src/cache/config.rs
Files modified: src/main.rs, src/commands/mod.rs
Total: +342 -12

### Stage: add-logging (completed)

Files added: src/logging/structured.rs
Files modified: src/main.rs, src/lib.rs
Total: +156 -8

### Potential Concerns

- src/main.rs modified by 2 stages — verify no conflicts
- 4 new files added — verify all are wired
```

**Implementation:** For each completed dependency stage, read its worktree branch's diff against the merge base. Aggregate file lists and line counts. This data is already partially available from the merge system.

#### 3b. Memory-Based Wiring Checklist

**File:** `loom/src/orchestrator/signals/generate.rs` — extend `build_signal_context()`

When generating the signal for an integration-verify stage, aggregate memory entries from ALL completed stages and extract "needs wiring" notes into an explicit checklist:

```markdown
## Downstream Wiring Checklist

From stage 'add-cache-layer':

- [ ] src/cache.rs needs mod declaration in src/main.rs
- [ ] CacheCommand needs registration in src/commands/mod.rs dispatch

From stage 'add-logging':

- [ ] StructuredLogger needs initialization in src/main.rs startup
```

**Implementation:** Read `.work/memory/<stage-id>.md` for each dependency. Filter entries containing file path patterns or keywords like "needs", "wire", "register", "mount", "import", "add to". Format as checklist items.

#### 3c. Structured Code Review Requirement

**New plan schema field:** `code_review` on integration-verify stages (optional but recommended)

```yaml
code_review:
  dimensions:
    - security # OWASP, auth, secrets, injection
    - architecture # patterns, coupling, duplication
    - testing # coverage gaps, edge cases
    - wiring # all features reachable and callable
  require_all: true # All dimensions must be covered
```

**File:** `loom/src/plan/schema/types.rs` — new `CodeReviewConfig` struct
**File:** `loom/src/plan/schema/validation.rs` — warn if integration-verify lacks code_review

The signal prefix for integration-verify already includes multi-dimensional review instructions. This makes it declarative and verifiable — integration-verify agents must explicitly address each dimension.

**Verification at completion:** For each declared dimension, check memory entries for review findings. If a dimension has no memory entries, warn:

```text
  Code review dimension 'security' has no findings recorded.
  Run security review and record results with: loom memory note "Security review: ..."
```

#### 3d. Aggregated Wiring Re-Verification

**File:** `loom/src/commands/stage/complete.rs` — extend integration-verify completion

When completing an integration-verify stage, re-run ALL wiring checks from ALL prior stages on the merged codebase (not individual worktrees). This catches cases where:

- A wiring check passed in the worktree but the merge broke it
- Cross-stage changes invalidated a wiring pattern

**Implementation:** Load the plan, collect all wiring checks from all stages, run `verify_wiring()` on each using the integration-verify worktree as working_dir. Adjust paths relative to each stage's working_dir.

---

## Design Decisions

1. **Leaf-stage wiring enforcement is BLOCKING** — When a stage has no downstream dependents and creates unwired files, `loom stage complete` fails with an error. The agent must wire the code or remove it. No dead code merges.

2. **All three layers implemented together** — Plan-time validation, stage-completion checks, and integration-verify strengthening ship as one cohesive change. The layers reinforce each other.

3. **Duplicate detection is name-only and advisory** — Simple same-name symbol matching across the codebase. Warnings only, never blocks completion. Catches the obvious "reimplemented without looking" case.

## Implementation Order

Implement in dependency order within a single effort:

1. **Schema/types first** — New `CodeReviewConfig` struct, any new plan schema fields (3c)
2. **New verify modules** — `wiring_detection.rs` (2a), `duplicate_detection.rs` (2c)
3. **Plan-time validation** — All validation.rs changes (1a, 1b, 1c)
4. **Stage completion** — Wire new modules into complete.rs (2a, 2b, 2c, 3d)
5. **Signal enrichment** — Cross-stage summary and wiring checklist (3a, 3b)
6. **Tests** — Unit tests for each module, integration test for full flow

## Critical Files

| File                                        | Changes                                            |
| ------------------------------------------- | -------------------------------------------------- |
| `loom/src/plan/schema/validation.rs`        | 1a, 1b, 1c — new validation rules                  |
| `loom/src/commands/stage/complete.rs`       | 2a, 2b, 2c, 3d — new completion checks             |
| `loom/src/verify/wiring_detection.rs`       | 2a — **new module** for file reference detection   |
| `loom/src/verify/duplicate_detection.rs`    | 2c — **new module** for symbol duplicate detection |
| `loom/src/verify/mod.rs`                    | Register new modules                               |
| `loom/src/orchestrator/signals/generate.rs` | 3a, 3b — richer integration-verify signals         |
| `loom/src/orchestrator/signals/format/`     | 3a, 3b — signal content formatting                 |
| `loom/src/plan/schema/types.rs`             | 3c — CodeReviewConfig struct                       |
| `loom/src/plan/graph/`                      | 1b — forward dependency traversal                  |

## Verification

1. **Unit tests** for each new validation rule (validation.rs) — test with/without artifacts+wiring
2. **Unit tests** for wiring_detection.rs — test reference detection per language
3. **Unit tests** for duplicate_detection.rs — test symbol extraction and matching
4. **Integration test** — create a plan with unwired artifacts, verify `loom init` warns
5. **E2E test** — run a stage that creates new files without wiring, verify completion fails/warns
6. **Manual test** — run a full loom plan and verify integration-verify signal includes change summary and wiring checklist
