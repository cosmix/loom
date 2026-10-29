# W4: the `[plan]` summary keys

Tier: sonnet (`loom-software-engineer`). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md`
("Config") first. Wave 1, parallel with W1; your files are disjoint from every other worker's.

## Files you own

`loom/src/user_config/keys.rs`, `loom/src/user_config/mod.rs`, `loom/src/user_config/render.rs`, `loom/src/user_config/tests.rs`,
`loom/src/user_config/workspace.rs`, `loom/src/fs/work_dir/config_sections.rs`,
`loom/src/commands/status/web/config_api/tests.rs`, `web/src/api/fixtures/config.json`.

Codex units write three modules from steps 2, 4 and 6 below in your wave (C8 `user_config/plan.rs`,
C9 `fs/work_dir/config_sections/plan_config.rs`) and the next (C10 `planning/summary/config.rs`); their
briefs (`c8-user-plan.md`, `c9-plan-config.md`, `c10-models.md`) pin the names you wire: you add
`mod plan;`, the `plan: plan::PlanSection` field and its parse call to `user_config/mod.rs`,
the `plan_value_of`/`plan_toml` calls to `render.rs`, the KeySpecs, the workspace mapping, the
`mod plan_config;` and re-export in `config_sections.rs`, the config_api test changes and the fixture.

## Keys (section 04 "Models"; section 09 "Models and effort")

| Key | Kind | Default | Help |
| --- | --- | --- | --- |
| `plan.document_summary_model` | `Enum(crate::claude::CLAUDE_MODELS)` | `haiku` | Model the loom-plan summary agent builds document summaries on |
| `plan.document_summary_effort` | `Enum(crate::models::stage::ALLOWED_REASONING_EFFORTS)` | `medium` | Reasoning effort for document summaries |
| `plan.overview_model` | models | `sonnet` | Model overviews and background summary sessions run on |
| `plan.overview_effort` | efforts | `high` | Reasoning effort for overviews and summary sessions |

Plan 8 later appends `plan.spec_authoring_*` and `plan.plan_authoring_*` after these four in the
same modules: keep each module's list easy to extend (one field, one getter, one arm per key).

## Mirror `[pressure]` exactly

1. `user_config/keys.rs`: four `KeySpec`s after the `models.*` entries (`:117-172`), section `plan`.
2. `user_config/plan.rs` (new): `PlanSection` with `Option<String>` fields parsed with
   `parse::get_enum` (pattern: `user_config/pressure.rs:16-59`); `UserConfig::plan_document_summary_model()`
   etc. with the defaults above as `pub const DEFAULT_PLAN_*` in `plan.rs`; `plan_value_of`
   (pattern `pressure.rs:125-155`) and `plan_toml` (pattern `pressure.rs:158-168`).
3. `user_config/mod.rs`: `mod plan;`, a `plan: plan::PlanSection` field on `UserConfig` (`:133-139`),
   parse in the constructor (`:285`), the doc comments that list workspace-backed sections.
   `render.rs` (`:17-60`): delegate to `plan_value_of` and add the `[plan]` block to `to_toml_string`
   in alphabetical section order (context, models, plan, pressure, terminal, update: the test at
   `user_config/tests.rs:281-300` pins the order; extend it).
4. `fs/work_dir/config_sections/plan_config.rs` (new): `PlanConfig` with `from_section_toml` and
   validated accessors and `pub fn read_plan_config(work_dir: &Path) -> PlanConfig`, the pattern of
   `pressure_config.rs` (`:1-100`), reading the table `[planning]` (const `PLANNING_SECTION`), NOT
   `[plan]`: `.loom/work/config.toml`'s `[plan]` table belongs to `loom init` and the daemon
   (`source_path`, `plan_id`, `plan_name`, `base_branch`; `fs/work_dir/config_sections.rs:11-12`), and
   both `deny_unknown_fields` here and `section_readable` there would make the project tier dead
   whenever a run exists. Re-export from `config_sections.rs` (`:31-38`) beside `read_pressure_config`.
5. `user_config/workspace.rs`: add `fn workspace_table(section: &str) -> &str` (`"plan" -> "planning"`,
   identity otherwise) and use it wherever the workspace file is read or written for a KeySpec
   (`resolve` `:170`, `backs` `:190`, `section_readable` `:219`, `write` `:134`); the
   `"pressure" | "models"` arm gains `"plan"`. A workspace write of `plan.overview_model` lands in
   `[planning]`; the run's `[plan]` table is never touched (test: write then re-read a file holding
   both tables, `[plan]` byte-identical). Plan 8 appends its four keys to the same `[planning]`
   project table.
6. `planning/summary/config.rs`: `SummaryModels::resolve(project, user)` (project value, else user
   value, which already falls back to the default; pattern `commands/pressure/models.rs:30-70`) and
   `SummaryModels::load(root)`: project tier from `crate::fs::work_dir::WorkDir::new(root)` when it
   resolves (pattern `commands/pressure/mod.rs:304-310`, `project_pressure_config`), user tier
   `UserConfig::load()`.
7. Dashboard config payload: `commands/status/web/config_api/tests.rs` pins the project scope to
   `"pressure" | "models"` sections (`:140`, `:291`) and counts the workspace-backed keys: add `"plan"`
   and fix the count comment. Regenerate `web/src/api/fixtures/config.json` the way the test at
   `:182-210` says (`the_config_fixture_matches_a_real_payload`: pretty JSON plus a trailing newline).
8. The settings dialog caption (`web/src/components/settings-model.ts`) and the web gates belong to
   stage `summary-settings-web`; do not edit web/src beyond the regenerated fixture.

## Tests (yours)

`user_config/tests.rs`: defaults, a set value, an unknown variant rejected naming `plan.overview_model`,
`value_of` arms (the test at `:318` iterates every key), render order. `plan_config.rs` unit tests as
`pressure_config.rs` has (`:100-160`). `planning/summary/config.rs` tests for precedence with a
non-default value at each tier.

## Proof

`cargo test --lib user_config::`, `cargo test --lib fs::work_dir::`,
`cargo test --lib commands::status::web::config_api::`, `cargo test --lib planning::summary::config`,
`cargo clippy --all-targets -- -D warnings`.
