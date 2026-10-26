# Codex units of stage `guidance-engine`

Each unit is one `loom-codex-forwarder` run, `--model gpt-6.1-terra --effort high`, spawned in
the FOREGROUND with a Bash timeout of 600000 ms. A unit writes only its files, runs no git, runs
no tests (compile checks at most: `cargo check --lib` from `loom/`), and touches no path under a
dot-loom or dot-work directory. The orchestrator runs each unit's proof command right after it,
then `git status --short`. Files stay 10% under the size limits (360 lines per file, 45 lines per
function) because `cargo fmt` grows them. Double literal braces inside `format!`. Put CLI
placeholders inside backticks in rustdoc.

Codex answers `loom map` from the BASE tree: units below name what earlier units of this stage
added.

## Wave A (6 units at once, before the Claude workers W3 and W4)

### C1 `src/user_config/keys.rs`: four KeySpec rows
- Owns: `loom/src/user_config/keys.rs`. Reads: the same file, `src/user_config/pressure.rs`.
- Entry point: `KEYS` in `keys.rs`. Plan 4 already added `plan.document_summary_model`,
  `plan.document_summary_effort`, `plan.overview_model`, `plan.overview_effort`.
- Steps: 1. After `plan.overview_effort`, add four `KeySpec` entries with section `plan`:
  `plan.spec_authoring_model` (field `spec_authoring_model`, `ValueKind::Enum(crate::claude::CLAUDE_MODELS)`,
  help `Claude model a loom plan authoring session runs on for spec steps`),
  `plan.spec_authoring_effort` (`Enum(crate::models::stage::ALLOWED_REASONING_EFFORTS)`,
  help `Reasoning effort a loom plan authoring session runs at for spec steps`),
  `plan.plan_authoring_model` (`Enum(CLAUDE_MODELS)`, help `Claude model a loom plan authoring session runs on for plan steps`),
  `plan.plan_authoring_effort` (`Enum(ALLOWED_REASONING_EFFORTS)`, help `Reasoning effort a loom plan authoring session runs at for plan steps`).
- Steps (cont.): 2. In `loom/src/user_config/tests.rs` (also yours), add one test: `spec("plan.spec_authoring_model")` and the three other names resolve, and each `parse("opus")`/`parse("high")` succeeds while `parse("bogus")` errors.
- Owns also: `loom/src/user_config/tests.rs` (that one test only).
- Done: `cargo check --lib` compiles once C2 and C4 also landed. Proof (orchestrator, after wave A): `cargo test --lib user_config::`.

### C2 `src/user_config/plan.rs`: section fields, parse, value_of, toml
- Owns: `loom/src/user_config/plan.rs`. Reads: `src/user_config/pressure.rs` (the pattern, lines 17-169).
- Entry points: `PlanSection`, `PlanSection::parse`, `UserConfig::plan_value_of`, `UserConfig::plan_toml` (plan 4).
- Steps: 1. Add `Option<String>` fields `spec_authoring_model`, `spec_authoring_effort`,
  `plan_authoring_model`, `plan_authoring_effort` to `PlanSection`, parsed with
  `parse::get_enum(doc, "plan", "<field>", <vocabulary>)` like the existing fields. 2. Add the four
  `plan.<field>` arms to `plan_value_of`, each `(ConfigValue::Text(self.plan_<field>().to_string()), self.origin_of(self.plan.<field>.as_ref()))`;
  the getters `plan_spec_authoring_model()` etc. are written by C4 in `plan_authoring.rs`. 3. Add
  the four `field = value` lines to `plan_toml`.
- Trap: defaults live only on the C4 getters, never here.
- Proof: `cargo test --lib user_config::` (after wave A).

### C3 `src/fs/work_dir/config_sections/plan_config.rs`: project-tier accessors
- Owns: `loom/src/fs/work_dir/config_sections/plan_config.rs`. Reads: `pressure_config.rs` beside it.
- Entry point: `PlanConfig` (plan 4; it deserialises the `[planning]` table of the workspace config).
- Steps: 1. Add `Option<String>` fields `spec_authoring_model`, `spec_authoring_effort`,
  `plan_authoring_model`, `plan_authoring_effort`. 2. Add `pub fn spec_authoring_model(&self) -> Option<&str>`
  and the three others, dropping an out-of-vocabulary value as the existing accessors do. 3. Add
  unit tests in the file's `mod tests`: each accessor returns a set value; an out-of-set value is
  dropped; a document holding the run's `[plan]` table (`source_path`, `plan_id`, `plan_name`,
  `base_branch`) plus `[planning] plan_authoring_model = "sonnet"` yields `Some("sonnet")`.
- Proof: `cargo test --lib fs::work_dir::`.

### C4 `src/user_config/plan_authoring.rs` (new): getters and tests
- Owns: `loom/src/user_config/plan_authoring.rs`, and the one line `mod plan_authoring;` in
  `loom/src/user_config/mod.rs` (beside `mod plan;`).
- Steps: 1. `impl UserConfig` with `pub fn plan_spec_authoring_model(&self) -> &str` (default
  `opus`), `plan_spec_authoring_effort` (`medium`), `plan_plan_authoring_model` (`opus`),
  `plan_plan_authoring_effort` (`high`), each `self.plan.<field>.as_deref().unwrap_or(<default>)`
  (field names from C2). 2. `#[cfg(test)] mod tests` in the same file: the four defaults on
  `UserConfig::default()`; `crate::user_config::parse_document("[plan]\nspec_authoring_model = \"sonnet\"\nplan_authoring_effort = \"low\"\n")`
  resolves those values and `value_of(spec("plan.spec_authoring_model"))` reports `Origin::User`;
  `plan_authoring_model = "gpt-5"` is rejected by `parse_document`.
- Proof: `cargo test --lib user_config::plan_authoring`.

### C5 `src/planning/guidance/start.rs` (new)
- Owns: `loom/src/planning/guidance/start.rs` (ending with `#[cfg(test)] mod tests;`) and
  `loom/src/planning/guidance/start/tests.rs` (`step_class` for every row of W4's §3 table,
  `from_offer` None for `PressureTest` and `StartRun`). W4 writes `src/planning/guidance/mod.rs` with
  `pub mod start;` (until then, `cargo check` cannot see the file; compile after W4).
- Pinned interface (verbatim from W4's brief §2): `pub enum AuthoringStart { Offer(crate::planning::checklist::Offer), Refine { target: PathBuf, passage: bool }, Knowledge { open_file: Option<PathBuf> }, Message { paths: Vec<PathBuf> }, NoStep }`,
  `impl AuthoringStart { pub fn from_offer(offer: &Offer) -> Option<AuthoringStart>; pub fn from_offer_id(id: &str, project: &Project) -> Option<AuthoringStart>; pub fn label(&self, project: &Project) -> String }`,
  `pub enum StepClass { Spec, Plan }`, `pub fn step_class(start: &AuthoringStart) -> StepClass`.
- Steps: 1. The enum, `from_offer` (None for `PressureTest`, `StartRun`). 2. `from_offer_id`:
  checklist `primary` + `secondary` by `Offer::id`; start ids `refine:<path>`,
  `refine-passage:<path>`, `knowledge`, `knowledge:<path>`, the path checked with
  `crate::planning::paths::check_relpath_syntax` and required to exist in `project` (else None).
  3. `step_class` and `label` per the table in W4's brief §3 (`doc/plans/briefs/feat-planner-08-guidance/guidance-engine/w4-guidance-module.md`).
- Proof: `cargo test --lib planning::guidance::start` and `cargo test --test planning_guidance_contracts offer_id_lookup_rejects_unknown_ids` (after W4).

### C8 `src/planning/guidance/models.rs` (new)
- Owns: `loom/src/planning/guidance/models.rs` (declared by W4's `mod.rs`; ends with
  `#[cfg(test)] mod tests;`) and `loom/src/planning/guidance/models/tests.rs`.
- Pinned: `pub struct ModelEffort { pub model: String, pub effort: String }`, `pub struct GuidanceModels { pub spec: ModelEffort, pub plan: ModelEffort, pub summary_session: ModelEffort, pub document_agent: ModelEffort, pub overview_agent: ModelEffort }`,
  `impl GuidanceModels { pub fn resolve(project: &PlanConfig, user: &UserConfig) -> GuidanceModels; pub fn load(root: &Path) -> anyhow::Result<GuidanceModels> }`.
- Steps: 1. `resolve`: each authoring slot is the project accessor (C3) or else the user getter
  (C4), as `slot` in `src/commands/pressure/models.rs:34-37` does; the summary slots come from
  plan 4's `crate::planning::summary::config::SummaryModels::resolve(project, user)`
  (`summary_session` and `overview_agent` = overview pair, `document_agent` = document pair).
  2. `load`: the workspace's `PlanConfig` through `read_plan_config` when `WorkDir::new(root)` succeeds (copy `project_pressure_config` in `src/commands/pressure/mod.rs:305-310`), else default; `UserConfig::load()`.
  3. In `models/tests.rs`: a project value beats a different user value; with nothing set the
  defaults are opus/medium, opus/high, sonnet/high, haiku/medium, sonnet/high.
- Proof: `cargo test --lib planning::guidance::models`.

## Wave A2 (after wave A)

### C6 `src/planning/guidance/opening.rs` (new) plus `src/planning/guidance/opening/tests.rs`
- Owns both files. Reads: C5's `start.rs` (added in wave A; pinned above), plan 1's `vision`, `metaplan`, `project` modules.
- Pinned: `pub fn opening_message(start: &AuthoringStart, project: &Project) -> anyhow::Result<String>`, `pub fn summary_opening_message(ids: &[String]) -> String`, `pub fn pressure_opening_message(cmd: PressureCommand, plan_rel: &str) -> String`, `pub fn display_name(kind_label: &str, start: Option<&AuthoringStart>) -> String`; `PressureCommand` is defined by W4 in `mod.rs` as `pub enum PressureCommand { Pressure, Address }`.
- Steps: 1. The three-line form and every row of W4's brief §3 table. 2. Summary, pressure and
  display-name forms (§3). 3. `opening/tests.rs`: one test per table row on a fixture project in a
  `TempDir` (built with `Project::load`), the summary and pressure forms, display names.
- Proof: `cargo test --lib planning::guidance::opening`.

## Wave C (after W4 returns)

### C7 `src/planning/guidance/tests.rs`: the module's unit tests
- Owns: `loom/src/planning/guidance/tests.rs` (W4's `mod.rs` declares `#[cfg(test)] mod tests;`).
- Steps: 1-3. Write every test listed in W4's brief §7 and §7b, against W4's `render_session_guidance`,
  `brief.rs` and `plugin.rs` (written by W4 in this stage; read them from the worktree, not `loom map`).
- Proof: `cargo test --lib planning::guidance::`.
