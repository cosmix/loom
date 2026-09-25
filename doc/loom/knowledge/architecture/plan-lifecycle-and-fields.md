---
verified: 5546d3c47ddc1f8890b40157134f057393b8b90e
---
# Plan Lifecycle And Fields

> Plan fields v1/v2, verify checks, lints

## Adding New Plan Fields Checklist

1. Add to StageDefinition (plan/schema/types.rs) with serde defaults
2. Add validation in validation.rs
3. Add to Stage model (models/stage/types.rs) with serde defaults
4. Copy in the canonical `Stage::from_definition()` builder (`models/stage/methods.rs`); init delegates to it
5. If goal-check: update has_any_goal_checks() in BOTH StageDefinition and Stage
6. If verification: add verify function in verify/goal_backward/ and call from run_goal_backward_verification()
7. Check ALL test files constructing Stage directly (src/ AND tests/ directories)
8. If the field reaches the agent: copy it onto `EmbeddedContext` (orchestrator/signals/types.rs) and
   emit it from BOTH format/sections.rs AND recovery_format.rs — the recovery signal embeds only the
   stable prefix, so a gated section missing there vanishes on any retry
9. Add the two backwards-compat tests (plan YAML without the key; legacy `.loom/work/stages/*.md` without
   the key) — see [Additive Schema Fields](../conventions.md); `#[serde(default)]` is the only migration
10. If it is user-facing, add a row to the README Stage Fields table

## Goal-Backward Verification (verify/goal_backward/)

Four verification layers for standard stages. **`truths` is NOT one of them** — it was removed from goal-backward and merged into acceptance. A duplicate section listing `truths` as a goal-backward layer was deleted from architecture.md on 2026-07-30; if you see that claim anywhere else, it is stale.

- **artifacts** -- Files must exist with real implementation (stub detection: TODO, FIXME, unimplemented\!, todo\!)
- **wiring** -- Regex patterns verifying code connections in source files
- **wiring_tests** -- Runtime command-based integration verification
- **dead_code_check** -- Command + pattern detection for unused code

Acceptance criteria (verify/criteria/runner.rs) now handle both:

- **Simple** -- Plain shell command, 5min timeout, exit 0 = pass
- **Extended** -- TruthCheck struct with stdout_contains, stderr_empty, exit_code, 30s timeout

Returns: GoalBackwardResult::Passed | GapsFound | HumanNeeded. Storage: `.loom/work/verifications/<stage-id>.json`.

Note: truths.rs module and verify_truth_checks() are retained for before_stage/after_stage verification (pre/post conditions), NOT for goal-backward.

## before_stage / after_stage / code_review Schema Fields — Execution Status

**Status as of 2026-09-24 (verified against `stage_executor.rs::before_stage_gate_passed`,
`models/stage/methods.rs::from_definition`, and `commands/stage/complete_verification.rs`).
Earlier text cited `plan_setup.rs:280-281` for field storage and `complete.rs:847-866` for
after-stage execution; both are stale -- `plan_setup.rs` no longer sets these fields, and
`complete.rs` after-stage logic moved to a dedicated file.**

| Field          | Schema Type                | Stored on Stage            | Executed   | Where                                             |
| -------------- | --------------------------- | --------------------------- | ---------- | -------------------------------------------------- |
| `before_stage` | `Vec<TruthCheck>`          | Yes (`models/stage/methods.rs::from_definition`) | Yes     | `orchestrator/core/stage_executor.rs::before_stage_gate_passed` (called pre-spawn) |
| `after_stage`  | `Vec<TruthCheck>`          | Yes (`models/stage/methods.rs::from_definition`) | Yes     | `commands/stage/complete_verification.rs` (calls `verify::before_after::run_after_stage_checks`) |
| `code_review`  | `Option<CodeReviewConfig>` | Yes (`models/stage/methods.rs::from_definition`) | Signal  | `orchestrator/signals/generate.rs` renders it for IV signals     |

**`before_stage` execution (`stage_executor.rs::before_stage_gate_passed`):**

- Runs after worktree creation, BEFORE session spawn
- **Gated on a pristine workspace.** `verify::before_after::find_prior_stage_work(stage_branch, base_branch, repo_root, worktree_path)` runs first; if it finds commits on `loom/<stage-id>` beyond the resolved base, or non-scaffold changes in the worktree, the checks are SKIPPED (logged at `info`) and the spawn proceeds. `before_stage` is a delta-proof ("the feature does not exist yet"), which is only meaningful on the first attempt -- re-running it on a re-spawn (orphan recovery, `loom stage retry`, crash retry) fails on the previous attempt's own work and blocks the stage before any session exists to finish it (unrecoverable loop; see mistakes.md 2026-07-27)
- Loom's own worktree scaffolding (`.loom/work`, `.claude/`, root `CLAUDE.md`) is discounted via `git::worktree::is_worktree_scaffold_path` -- it is present from the first spawn, and in repos that don't gitignore it, counting it would disable the gate entirely
- Calls `crate::verify::before_after::run_before_stage_checks(&stage.before_stage, &check_dir)`
- On failure gaps: stage becomes `Blocked` (FailureType::TestFailure), session NOT spawned. `TestFailure` is not auto-retryable (`should_auto_retry`), so the stage rests Blocked until an operator runs `loom stage retry`
- On errors (infrastructure): prints warning, continues anyway (advisory)
- TruthCheck timeout: 30 seconds (hardcoded in truths.rs:13)

**`after_stage` execution (`commands/stage/complete_verification.rs`, called from `loom stage complete`):**

- Runs during `loom stage complete`, AFTER acceptance criteria pass
- Skipped entirely when the stage definition's `after_stage` list is empty
- On failure: stage stays Executing, no merge, agent must fix and re-run

**`code_review` -- PERSISTED AND WIRED FOR SIGNAL GENERATION:**

- Parsed by serde at schema level and copied by `Stage::from_definition()` onto the runtime `Stage`
- `orchestrator/signals/generate.rs` reads the persisted runtime field; it does not reparse the plan
- `render_review_dimensions()` emits a `## Review Dimensions` checkbox section in IV signals, honoring `require_all`
- It remains agent guidance rather than an acceptance or goal-backward verification primitive

## load_stage_definition_from_plan — Centralized Plan Lookup

Centralized in `plan/parser/mod.rs` (re-exported via `plan/mod.rs`). Previously lived in `commands/verify.rs`.

**Signature:** `load_stage_definition_from_plan(stage_id, work_dir) -> Result<Option<StageDefinition>>`

Reads `.loom/work/config.toml` for plan path, calls `resolve_source_path()`, calls `parse_plan()`, finds stage by ID. Used by:

- `commands/stage/complete.rs` — after-stage execution

**Why plan/ layer:** completion needs the authoritative plan definition for checks that are intentionally evaluated from the active plan. Runtime policy copied onto `Stage` should not reparse the plan at its consumers.

## Plan Versioning / Runtime Amendment (Shipped — `plan/amendment.rs`)

Runtime plan amendment exists and is reachable from an `Accept` adjudication verdict. `.loom/work/plan_versions/` is its audit trail:

- `.loom/work/plan_versions/.lock` — `flock` (serializes amendments)
- `.loom/work/plan_versions/<n>.md` — snapshot of full plan content after amendment n
- `.loom/work/plan_versions/audit.md` — O_APPEND atomic rows (amendment log)

**The write set is TWO files, not one — this is the trap.** Under the lock, an amendment writes the snapshot, appends the audit row, replaces the live plan file via `safe_replace_outside_workdir`, **and rewrites the target stage's `.loom/work/stages/<n>-<id>.md`**. The last step is not optional: `plan/graph/loader.rs` prefers `.loom/work/stages/` over the plan file, so amending only the plan would leave `sync_graph_with_stage_files` serving the old criteria forever. (An earlier version of this section described a 6-step flow ending at "atomic rename plan file", omitting the stage-file write.)

The proposed value is deserialized into the **real** `AcceptanceCriterion` / `WiringCheck` types before anything is written, so a malformed patch fails fast rather than corrupting the plan. A per-stage cap (default 3, `loom.adjudication.max_amendments_per_stage`) bounds runaway adjudication.

**Recovery** — `verify_plan_versions_consistency()`, called from orchestrator startup, handles three divergences:

| On disk                                 | Action                                                             |
| ---------------------------------------- | -------------------------------------------------------------------- |
| Snapshot written, audit row missing     | Orphaned snapshot — removed so the next amendment can claim the id |
| Audit row appended, plan file still old | Re-apply the snapshot to plan + stage file (catch-up commit)       |
| Plan + audit in sync, stage file stale  | Re-apply just the stage-file update                                |

## Plan Immutability Invariant (Narrowed, Not Removed)

Plans are loaded ONCE at daemon startup via `build_execution_graph()` -> `ExecutionGraph::build()`; there is no general reload mechanism, and the in-memory `graph: ExecutionGraph` on `Orchestrator` holds all state. Amendment is the one sanctioned mutation path and it is deliberately narrow: **only the `acceptance` and `wiring` arrays on a single stage**. Stage IDs, dependencies, `working_dir`, DAG topology, and plan structure are never amendable — so the graph the daemon loaded at startup stays topologically valid for the life of the run.

## What `loom plan verify` Rejects Before a Stage Runs (2026-09-19)

Four checks turn plan-authoring mistakes into an error or warning at `loom plan verify` time, before a
stage session spends tokens on them. All run inside `validation.rs` (`validate` for errors,
`validate_structural_preflight` for warnings).

| Check | Severity | Source | Catches |
| --- | --- | --- | --- |
| Declared skills | error / warning | `structural_checks/declared_skills.rs` | a stage `skills:` list with an empty name, a duplicate, or a name absent from the skill index; a warning when no index can be loaded |
| Host paths | error | `host_paths.rs`, `host_paths/commands.rs` | an ephemeral (`/tmp`) `allow_write` grant, an absolute grant missing on the host, a `setup` `mkdir`/`touch`/redirect aimed outside the worktree, a `TMPDIR=` override |
| Criterion hazards | error / warning | `validation/criterion_hazards.rs` | see below |
| Base-tree evaluation | warning (`baseline` bucket) | `validation/base_tree.rs` | a criterion that already passes at HEAD |

**Declared skills.** `StageDefinition::skills` (`plan/schema/types.rs:321`) names the skills a stage's
agents need. The signal renders them before detected ones (`signals/format/skills.rs`) and the worker-brief
hook names them (`commands/hook/worker_brief.rs`), so a declared skill reaches a worker even with an empty
context pack. `check_declared_skills(stages)` takes ONLY the stages and resolves `~/.claude/skills` plus its
catalog, the way the orchestrator does at run time. An earlier draft also took a `repo_root` and preferred
`<repo>/.claude/skills` exclusively; on this repository, which has project skills there, that reported
catalogued names such as `loom-rust` as unknown.

**Criterion hazards.** Error-level (`Hazard::is_error`): `MaskedExit`, `HomeFromExpansion` (HOME assigned
from a variable or substitution, including `export HOME=$x`), `BareMktempDir`. Warning-level: `Network`,
`PlanPath` (reads `doc/plans/`, which the lifecycle renames), `VitestNameFilter`, `PipeStatus`,
`RgReplace` (`rg -r` is `--replace`), `TestRunnerInWiring`, `HardcodedTmp`. Commands are lexed
(`shell_lex.rs`), so text inside a quoted `rg` pattern is data and never read as a hazard.

**The masked-exit rule inspects the final top-level statement only** (`criterion_hazards/masked_exit.rs`):
split on `;`, `;;` and `&` at paren depth 0, then flag a final `|| true`, `|| :` or `|| exit 0`, or a final
bare `true`/`:`/`exit 0` when a real statement precedes it. So `rm -rf t || true; cargo build` is clean,
`cargo test; true` is flagged, and a lone `true` is NOT flagged: 39 test fixtures use it as a placeholder
criterion. Known limits, left in place because both shapes are rare: `strip_parens` strips a leading `(`
and a trailing `)` independently, so `cargo build && (cargo test || true)` is flagged although a failing
build still fails it, and redirects after the final `true` (`cmd || true 2>/dev/null`) are not flagged.

**Base-tree evaluation** reproduces, without running it, the exit status of a single `rg` or `grep` over
literal paths against HEAD and warns when it already meets the criterion's expectation, since such a
criterion cannot tell a stage that did its work from one that did nothing. A file is read from the checkout
when the changed-path probe does not list it, else through `git show HEAD:<path>`. The probe is the plumbing
command `git diff-index --name-only -z HEAD --`, NOT porcelain `git diff`: porcelain opportunistically
rewrites `.git/index`, and `plan verify` must have no side effects. Its tests build repositories through
`crate::git::run_git` rather than hand-rolled `Command::new("git")` chains.

## Plan Version 2: Fields, Validation, and `plan verify` Lints

`loom.version` accepts `1` and `2`; anything else fails with `Unsupported version: <n>. Supported versions: 1, 2.`
A v1 plan using a v2-only field gets one error per use, `` `<field>` requires `version: 2` `` (stage-scoped for a stage
field). Tests that need an "unsupported" version use `3`. The runtime `Stage` carries `plan_version: u32` (serde
default 1) and every v2 behaviour reads it; nothing reads the plan file at run time to learn the version.
`Stage::from_definition(definition, &PlanIdentity)` takes the plan's id, version and `ratchet_files`;
`PlanIdentity` is built through `From<&ParsedPlan>` (`models/stage/checks.rs`).

**v2 fields** (types in `plan/schema/types_v2.rs`, re-exported through `plan/schema/mod.rs`'s explicit list):

| Field | On | Meaning |
| --- | --- | --- |
| `contracts: [{id, file, test, runner?, scenario, rejects}]` | stage | tests written and frozen before implementation (`architecture/contract-phase.md`); `id` is `^[a-z0-9][a-z0-9-]*$` and unique in the stage |
| `harness: [glob]` | stage | extra files the contract writer may edit; frozen with the contracts |
| `reachable: [{symbol, from, min_confidence?, description}]` | stage | the new unit must be reached from an entry point (`architecture/verification-v2-gates.md`) |
| `wiring[].literal: bool` | wiring check | escape the pattern before matching; `source` may be a glob |
| `ratchet_files: [path]` | plan (`LoomConfig`), copied onto every `Stage` | exact checkout-relative baseline or ledger files; any change raises `TI-ratchet` |

v2 validation, each an error: every `standard` stage has at least one contract and `knowledge`, `knowledge-distill`
and `integration-verify` stages have none; contract `file`, `test`, `scenario`, `rejects` non-empty; `file`,
`harness` and `ratchet_files` entries relative with no `..`; `reachable` fields non-empty and `min_confidence`
within 0..=1. `validate()` skips the wiring regex-compile check for `literal: true` (a literal such as `run(`
is not a valid regex). Adding any field to `Stage` grows the ledgered `Stage::default` literal; pay it with
a flattened sub-struct or a moved unit, not a ledger raise (the dispute counters went to `stage.tally`).
`TruthCheck`, `WiringCheck` and `AcceptanceCriterion` moved to `models/stage/checks.rs`.

**Lints** (`plan/schema/validation/v2_lints/`, one entry `run(&LintContext, &mut notes)`, called from
`commands/plan/verify.rs` and mapped through `v2_fields::split_lint_findings`). A finding is an error when the plan
is v2 and the lint is `error_in_v2`, else a structural warning (so `--strict` fails on it). `plan verify` has no
side effects: no cache or index writes, no builds. Commands are lexed with `validation/shell_lex.rs`.

| Lint | Rule | Error in v2 |
| --- | --- | --- |
| unknown `loom` subcommand (`loom_subcommands.rs`) | argv[0] `loom` names a path absent from the compiled clap tree | yes, downgraded to a warning only when the calling stage or a transitive dependency touches `loom/src/cli` |
| regex (`regex_patterns.rs`) | wiring pattern the 1 MiB-limit builder rejects; `[[`; a pattern read as a flag; `rg` (minus `-F`/`-P`/`--pcre2`/`--engine`) and `grep -E` patterns that do not compile in Rust syntax. grep BRE and PCRE are skipped: Rust syntax rejects valid BRE (`foo(`) | mixed |
| sandbox capability (`sandbox_capability.rs`) | network binary without `allowed_domains`; a criterion needing `tmux`, `docker`, `loom map`, `loom knowledge context` | yes |
| knowledge check (`knowledge_check.rs`) | `loom knowledge check --strict` without `--baseline` while the tree has structural issues; uses `catalog::structural_issues(root)` (`build_curated(root, false)`, evidence collector off, read-only) | yes |
| rust filters (`rust_filters.rs`, G5) | a `cargo test` module filter whose every `::` segment is not present in the base graph (node name or path component) and that no `files:` entry could create; no base layer gives one note | no |
| contracts (`contracts.rs`) | unknown `runner`; undetectable runner (warning: completion falls back to exit code); v2 integration-verify without an acceptance command an adapter recognises as a full run | mixed |

**The unknown-subcommand lint and a plan that adds the subcommand.** The CLI tree is the verifying binary's, so
a stage that adds `loom stage contracts` and a later stage that runs it fail `plan verify` before the first
stage runs. The downgrade above resolves it; testing an independent stage's typo still errors. The installed
`loom` on `PATH` can predate a merge: `loom project detect` printed "unrecognized subcommand" from a checkout
that contains it.

**The D4 "Rustc wrapper" lint was dropped.** It warned on every cargo plan whenever sccache was installed and
`LOOM_SCCACHE` unset, but loom withholds `RUSTC_WRAPPER` from sandboxed sessions that cannot run it
(`build_cache::sccache_usable_in`), so the lint fired for a failure that cannot occur and made
`plan verify --strict loom/tests/fixtures/plans/v2-valid.md` fail on any host with `/usr/bin/sccache`.

**Authoring rules learned from the merged tree** (also in `skills/loom-plan-writer/references/v2-contracts.md`):
a contract with no adapter runs its `test` string as the command and only the exit code judges it; harness globs
cover test-only files (a glob over `mod.rs` freezes the file the implementer must edit); prefer contract
locations that need no new `mod` declaration; a stage whose only goal-backward check is `reachable` is now
verified (the field is counted in `has_any_goal_checks`), but pair it with `artifacts` anyway; the integration-verify
stage lists the full test command. The verification report document that two briefs cited as required reading was never tracked and is
absent from every stage worktree: a plan whose briefs cite a doc commits it with the plan.
