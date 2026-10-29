# C1: the version-2 bookend lints (codex unit)

Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first. Paths below are relative to `loom/`.

## Files

Write only these two:

- `src/plan/schema/validation/v2_lints/bookends.rs` (new)
- `src/plan/schema/validation/v2_lints/mod.rs` (three one-line edits)

Read for the pattern, do not edit:

- `src/plan/schema/validation/v2_lints/loom_subcommands.rs`, function `check`: builds `index_by_id` and walks ancestry with `transitive_dependencies`.
- `src/plan/schema/validation/v2_lints/mod.rs`: `LintContext`, `LintFinding`, `LintFinding::in_stage`, `run`.
- `src/plan/schema/detect.rs`: `detect_stage_type`.
- `src/plan/schema/structural_checks.rs`: `transitive_dependencies` (returns the indices a stage depends on, transitively).

Never run git. Never touch a `.loom/` path. Use no network.

## Step 1: write `bookends.rs`

A module doc of two or three lines: a version-2 plan needs exactly one knowledge-distill stage after every other stage, and every standard stage needs an integration-verify stage after it; both findings are errors in version 2 and warnings in version 1.

Entry point, exactly:

```rust
pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>)
```

Imports: `std::collections::HashMap`, `crate::plan::schema::detect_stage_type`, `crate::plan::schema::structural_checks::transitive_dependencies`, `crate::plan::schema::StageType`, and `super::{LintContext, LintFinding}`. `LintFinding::in_stage(stage, message, error_in_v2)` is private to the parent module and callable from this child module.

Behaviour:

1. `let stages = &ctx.metadata.loom.stages;` Return at once when it is empty.
2. Resolve each stage type once with `detect_stage_type(stage)`: a stage may omit `stage_type`, and the plan runs it as the type its id and name imply.
3. Build `index_by_id: HashMap<&str, usize>` from stage ids, as `loom_subcommands::check` does.
4. Distill lint. Collect the indices of `StageType::KnowledgeDistill` stages.
   - Count not equal to 1: push one plan-wide finding and skip the rest of this lint:
     `LintFinding { stage_id: None, message: format!("the plan has {count} knowledge-distill stages; a plan needs exactly one, after every other stage, so the memory of every stage is curated once"), error_in_v2: true }`.
   - Exactly one, index `d`: the stages whose `dependencies` list contains `stages[d].id`, in plan order. When there is at least one, push `LintFinding::in_stage(&stages[d], format!("knowledge-distill stage has dependents ({ids}); it must be the last stage of the plan"), true)` with `ids` joined by `", "`.
   - Then `let ancestors = transitive_dependencies(d, stages, &index_by_id);` and for every index `i != d` in plan order with `!ancestors.contains(&i)`, push `LintFinding::in_stage(&stages[i], format!("is not an ancestor of knowledge-distill stage`{}`, so its memory is never distilled", stages[d].id), true)`.
5. Verify lint. Compute `transitive_dependencies` once for each `StageType::IntegrationVerify` index. For every `StageType::Standard` stage `s` in plan order that no integration-verify ancestor set contains (this includes a plan with no integration-verify stage), push `LintFinding::in_stage(&stages[s], "is not an ancestor of any integration-verify stage, so its merged code is never verified".to_string(), true)`.

Split the body into `check` plus two private helpers (one per lint) so every function stays under 50 lines; keep the file under 150 lines. The four message texts above are byte-exact: unit tests and the frozen contracts select findings by them. `ValidationError` prefixes a stage finding with `Stage '<id>':`, so each stage message starts in lower case.

## Step 2: register the module in `mod.rs`

- Add `mod bookends;` as the first line of the child-module list (alphabetical: directly before `mod contracts;`).
- In `run`, add `bookends::check(ctx, &mut out);` on the line directly after `contracts::check(ctx, &mut out);`.
- Change `mod loom_subcommands;` to `pub(crate) mod loom_subcommands;` (a later worker exposes a function from it).

`run` is called only by `loom plan verify` (`src/commands/plan/verify.rs`, `execute`). Register the lint nowhere else.

## Step 3: check your own text

Run no build, test or formatter: six Claude workers build the same crate in this worktree at the same time, so a cargo command can wait on their build lock past the 540-second forward deadline. Re-read both files against the signatures and the four message texts above instead. The orchestrator runs `cargo build` right after this unit returns, and runs the unit tests a sibling worker writes for these lints in `src/plan/schema/tests/v2_lint_bookend_tests.rs`; a compile error goes to a fresh Claude worker.

Report the final line count of `bookends.rs` and the three edited lines of `mod.rs`.
