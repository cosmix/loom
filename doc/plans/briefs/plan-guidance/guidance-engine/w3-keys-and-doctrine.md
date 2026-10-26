# W3: the dashboard fixture, and the doctrine edits outside the plugin

LANE SPLIT: the four keys' Rust code (Part 1 below) is written by codex units C1 (`keys.rs`), C2
(`plan.rs`), C3 (`plan_config.rs`) and C4 (`plan_authoring.rs`, getters and their tests), see
`doc/plans/briefs/plan-guidance/guidance-engine/codex-units.md`, in wave A before you start.
Part 1 is kept here as the specification they implement; you own only Parts 2 and 3. If a
Part 1 file is wrong, report it; do not edit it.

Stage `guidance-engine` of `doc/plans/PLAN-plan-guidance.md`. Tier: sonnet (`loom-software-engineer`).
Never run git, never spawn subagents. At most one narrow check, once:
`cargo test --lib user_config::` from `loom/`.

## Files you own (paths relative to `loom/`, the stage's working_dir)

- `src/commands/status/web/config_api/tests.rs` (only if a key-count assertion there names the
  `[plan]` keys)
- `../web/src/api/fixtures/config.json`
- `../web/src/components/settings-model.test.ts`
- `../skills/loom-plan-writer/SKILL.md`
- `../skills/loom-plan-writer/references/plan-template.md`
- `../commands/pressure.md`
- `../commands/address.md`
- `../README.md`

Your one check: `cargo test --lib config_api::` (the fixture parity test).

## Part 1: four keys appended to plan 4's `[plan]` section

Plan 4 (`doc/plans/PLAN-plan-summary-store.md`, stage `summary-cli`, merged before this plan)
created the `[plan]` section with `plan.document_summary_model/_effort` and
`plan.overview_model/_effort`: `src/user_config/plan.rs` (`PlanSection`, its `parse`, getters
`UserConfig::plan_document_summary_model()` etc., `plan_value_of`, `plan_toml`) and the project
tier `src/fs/work_dir/config_sections/plan_config.rs` (`PlanConfig` with `Option<&str>`
accessors, `read_plan_config(work_dir)`). Re-read both files first; they mirror
`src/user_config/pressure.rs` and `src/fs/work_dir/config_sections/pressure_config.rs`.

Append, in this order, after the overview pair:

| Key | Field | Kind | Default | Help text |
| --- | --- | --- | --- | --- |
| `plan.spec_authoring_model` | `spec_authoring_model` | `ValueKind::Enum(crate::claude::CLAUDE_MODELS)` | `opus` | `Claude model a loom plan authoring session runs on for spec steps` |
| `plan.spec_authoring_effort` | `spec_authoring_effort` | `ValueKind::Enum(crate::models::stage::ALLOWED_REASONING_EFFORTS)` | `medium` | `Reasoning effort a loom plan authoring session runs at for spec steps` |
| `plan.plan_authoring_model` | `plan_authoring_model` | `Enum(CLAUDE_MODELS)` | `opus` | `Claude model a loom plan authoring session runs on for plan steps` |
| `plan.plan_authoring_effort` | `plan_authoring_effort` | `Enum(ALLOWED_REASONING_EFFORTS)` | `high` | `Reasoning effort a loom plan authoring session runs at for plan steps` |

Add, matching plan 4's style exactly:

- `KeySpec` rows in `src/user_config/keys.rs` `KEYS` after `plan.overview_effort`.
- `PlanSection` fields parsed with `parse::get_enum` as `pressure.rs:29-59` does; getters
  `pub fn plan_spec_authoring_model(&self) -> &str`, `plan_spec_authoring_effort`,
  `plan_plan_authoring_model`, `plan_plan_authoring_effort`, each returning the default above
  when unset; arms in `plan_value_of`; lines in `plan_toml`. Defaults live on the getters only
  (`keys.rs` module doc).
- `PlanConfig` accessors `spec_authoring_model()`, `spec_authoring_effort()`,
  `plan_authoring_model()`, `plan_authoring_effort()` (`Option<&str>`, out-of-set value dropped
  as `pressure_config.rs` does) and their unit tests beside plan 4's.
- Tests in `src/user_config/tests.rs`: the four defaults; a `[plan]` document setting
  `spec_authoring_model = "sonnet"`, `plan_authoring_effort = "low"` resolves those values and
  `value_of` reports `Origin::User` for them; an out-of-vocabulary value is rejected the way
  plan 4's summary keys are.
- `src/user_config/plan.rs` must stay under 400 lines and every function under 50 lines. If the
  four getters push it past 400, move the getters into `src/user_config/plan_authoring.rs`
  (new file, yours) with an `impl UserConfig` block, the split `user_config/render.rs` documents.

Project tier (px-seams-02, plan 4's decision): the user tier is `[plan]` in `~/.loom/config.toml`,
but the PROJECT tier of every one of these keys lives in the `[planning]` table of
`.loom/work/config.toml` (that file's `[plan]` table is the run identity: `source_path`,
`plan_id`, `plan_name`, `base_branch`). Plan 4's `PlanConfig` deserialises `[planning]` and
`user_config/workspace.rs` maps section `plan` to table `planning` through
`workspace_table(section)`; add your four fields to `PlanConfig` only, and add a test with a
`.loom/work/config.toml` holding the run's `[plan]` table plus `[planning] plan_authoring_model =
"sonnet"`: the project value wins and the run identity still parses.

W4 resolves these through `PlanConfig` and `UserConfig` in its `models.rs`; the signatures
above are its contract with you. Do not write anything under `src/planning/`.

## Part 2: the dashboard fixture

`src/commands/status/web/config_api/tests.rs::the_config_fixture_matches_a_real_payload` pins
`web/src/api/fixtures/config.json` to the real payload; adding keys breaks it until the fixture
is regenerated (that test's doc comment, lines 181-192, gives the method: print the payload the
test builds and write it with `serde_json::to_string_pretty` plus a trailing newline). Do that
with a throwaway `eprintln!` you remove afterwards, and keep every other byte. Then update
`web/src/components/settings-model.test.ts`'s row-count assertion for the `plan` section (four
pair rows after your keys: document_summary, overview, spec_authoring, plan_authoring). Do not
change `settings-model.ts`.

## Part 3: doctrine edits

1. `skills/loom-plan-writer/references/plan-template.md`: directly under line 6 `# Plan:
   [Title]`, add a blank line and `Implements: [section_<nn>, section_<nn>, ... or none]`
   (section 03 "Product-to-plan links": comma-separated `section_<nn>` tokens, or `none`). In
   the YAML block nothing changes. The contract
   `template_skeleton_carries_implements_and_verifies` requires it to be the second non-empty
   line of the template's markdown block and `loom plan verify` to accept the block.
2. `skills/loom-plan-writer/SKILL.md` Section 3, line 121 (the sentence beginning "Every plan
   is a markdown document"): after "(title," insert "the `Implements:` line directly under the
   title naming the product sections the plan implements or `none`,". Keep the rest of the
   sentence. This file is pinned text in several surfaces: run `rg -n 'Every plan is a markdown
   document' ../skills ../loom/src` and update any test that pins that sentence's exact bytes.
3. `commands/pressure.md` and `commands/address.md`: append one paragraph each, as the last
   line: `Never edit the plan's \`## Pressure tests\` section: the driver that ran this command
   writes that record.` (section 11, "The commands").
4. `README.md`: in the configuration section that lists `[pressure]` keys (find it with
   `rg -n 'pressure.claude_model' ../README.md`), add the four keys with their defaults under
   plan 4's `[plan]` summary keys.

## Done when

`cargo test --lib user_config::` passes (your one check). Report the final line count of
`src/user_config/plan.rs` and whether you split it.
