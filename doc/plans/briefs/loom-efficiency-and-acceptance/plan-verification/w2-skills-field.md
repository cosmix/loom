# W2 — stage `skills:` field, skill line in worker briefs, worker-table checks

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first.

## Goal

A plan stage can declare the skills its agents need; `loom plan verify` validates the names; the
stage signal and every worker brief carry them. Evidence: report section 4.4 (6.5% of spawn
prompts mention a skill; suggestion-to-load conversion near zero).

## Files you own (write)

- `loom/src/plan/schema/types.rs` (`StageDefinition`, around 204-325)
- `loom/src/models/stage/types.rs` (`Stage`, around 578) and `models/stage/methods.rs`
  (`Stage::from_definition`, 28-64)
- `loom/src/fs/stage_loading.rs` (`definition_from_stage`, 46-88)
- every other file that builds a `StageDefinition` literal (`rg -l 'StageDefinition \{' loom/src`
  names 18 in all): `plan/amendment.rs`, `plan/graph/tests.rs`, `plan/schema/tests/mod.rs`,
  `plan/schema/structural_checks/worker_table_tests.rs`, `fs/tests_stage_loading_round_trip.rs`,
  `git/worktree/base.rs`, `commands/init/tests.rs`, `commands/run/tests/mod.rs`, and six files
  under `orchestrator/core/`
- `loom/src/plan/schema/structural_checks/declared_skills.rs` (new),
  `structural_checks/worker_table.rs`, their tests, and the `mod` line in `structural_checks.rs`
- `loom/src/orchestrator/signals/generate.rs`, `loom/src/orchestrator/signals/format/skills.rs`
- `loom/src/commands/hook/worker_brief.rs`

Read-only: `loom/src/skills/` (`index.rs`, `index_catalog.rs`, `recommend`), and
`loom/src/plan/schema/validation.rs` (W1 wires your functions in).

## Steps

1. Add `#[serde(default)] pub skills: Vec<String>` to `StageDefinition` and `skills: Vec<String>`
   to `Stage`. `Stage::from_definition` assigns field by field, so a missed field compiles
   silently: add `stage.skills = definition.skills.clone();`. `definition_from_stage` is an
   exhaustive literal with no spread on purpose
   (`doc/loom/knowledge/mistakes/schema-reuse-and-silent-skips.md`); add the field there. `Stage`
   is also deserialised from `.loom/work/stages/*.md` frontmatter, so the `Stage` field needs
   `#[serde(default)]`. Add a round-trip assertion to `fs/tests_stage_loading_round_trip.rs`.
2. `check_declared_skills` with the signature pinned in W1's brief. Load the index the way
   `signals/generate.rs:67` (`generate_signal_with_skills`) obtains its `SkillIndex`; resolve
   names with `SkillIndex::get_by_name` (`skills/index.rs:128`). Unknown name with a loaded
   index → error entry. Index cannot be loaded → one warning. Empty or duplicate names → error.
3. Signal: in `generate.rs`, resolve `stage.skills` to `SkillMatch` values and put them first in
   `embedded_context.skill_recommendations`. In `format/skills.rs`
   (`format_skill_recommendations`, 6-36) add a third group rendered before `detected`:
   declared skills, worded as required for this stage, with the existing combined loader line.
4. Worker brief: `commands/hook/worker_brief.rs` builds the brief the spawn guard appends to
   subagent prompts. Add one line naming the stage's declared skills and the loader call, only
   when the list is non-empty. Keep inside the existing payload ceiling (`worker_brief.rs:142`).
5. `check_worker_granularity` in `worker_table.rs`: warn when a stage's worker table has four or
   more rows that each own exactly one path, naming the stage and quoting
   `group small tasks into one subagent: every spawn pays the boot cost`. Fix the `(NEW)` trap in
   `normalize_path` (132-151): strip a trailing parenthesised annotation and surrounding
   backticks before treating a cell piece as a path; add the missing test.

## Traps

- Two skill systems exist. `commands/skill_index.rs` builds the hook's keyword JSON and is not a
  name lookup. Use `crate::skills`.
- `is_core_skill` (`skills/index_catalog.rs:40`) reads `skills/core-skills.txt` at compile time;
  both core and catalogued names are valid declarations.
- One malformed row makes `table_claims` return nothing for the whole table
  (`worker_table.rs:98-115`). The granularity check reuses `worker_claims`; do not re-parse.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib plan::schema::structural_checks`
— run once. Report every fixture file you touched.
