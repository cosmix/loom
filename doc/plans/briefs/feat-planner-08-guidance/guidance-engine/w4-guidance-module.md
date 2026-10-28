# W4: embed `plan-plugin/`, render it per session, generate briefs and messages

Stage `guidance-engine` of `doc/plans/PLAN-feat-planner-08-guidance.md`. Tier: sonnet (`loom-software-engineer`).
Never run git, never spawn subagents. At most one narrow check, once, at the end:
`cargo test --lib planning::guidance::` from `loom/`. The frozen contract file
`tests/planning_guidance_contracts.rs` is READ-ONLY: never edit it; make it pass.

## Files you own (relative to `loom/`)

- `build/assets.rs`
- `src/assets/mod.rs` (the `tests_table` module only: add `PLAN_PLUGIN` to `groups()` and a
  required-files test)
- `src/planning/mod.rs` (one line: `pub mod guidance;`)
- `src/planning/guidance/mod.rs` (declaring `pub mod start; pub mod opening; pub mod models;
  mod brief; mod plugin;` and `#[cfg(test)] mod tests;`), `brief.rs`, `plugin.rs`,
  `text/common_brief.md`, `text/summary_brief.md`

LANE SPLIT: codex units write `start.rs` (C5), `models.rs` (C8), `opening.rs` and
`opening/tests.rs` (C6) before you start, and `tests.rs` (C7, the tests of §7 and §7b) after you
return: `doc/plans/briefs/feat-planner-08-guidance/guidance-engine/codex-units.md`. §2, §3, §6 and §7 remain
the specification they implement; read their files, do not rewrite them. Report any defect in them.
- `src/plan_server/guidance/mod.rs`, `src/plan_server/guidance/provider.rs`,
  `src/plan_server/guidance/brief_route.rs`, `src/plan_server/guidance/tests.rs` (new)
- `src/plan_server/routes.rs` (one line in `all_routes()`, one in `install_all()` after plan 7's)
- `../web/src/plan/api/fixtures/session-brief.json` (new; plan 9 writes its zod schema)
- `src/plan_server/mod.rs` (one line: `pub mod guidance;`)
- `tests/planning_guidance_operator.rs` (new; the operator-run test, section 9 below)

Read-only: everything under `../plan-plugin/` (W1a, W1b write it in parallel; the orchestrator
wrote `.claude-plugin/plugin.json`), `src/user_config/` and `src/fs/work_dir/config_sections/`
(W3 adds the four authoring getters), `../commands/`.

## 1. Embedding (`build/assets.rs`)

`generate_embedded_assets` (`build/assets.rs:53-102`) emits one `pub const <NAME>: &[Asset]`
per root through `emit_group`; `ASSET_ROOTS` (`:21-28`) feeds `emit_asset_rerun_keys`, which
panics on a missing root. Add `const PLAN_PLUGIN_ROOT: &str = "plan-plugin";`, add it to
`ASSET_ROOTS` (the array length becomes 7, and update the doc comment's count), and emit
`PLAN_PLUGIN` with `walk_files(&plan_plugin)` and `KeyShape::Nested`, so keys read
`.claude-plugin/plugin.json`, `skills/plan/SKILL.md`, `agents/summary.md`. Check that
`walk_files`/`is_skipped` do not drop the `.claude-plugin` dot-directory; if they do, give the
walk an explicit allowance for that one directory name rather than changing `is_skipped` for
the other roots. `src/assets/install.rs::install_all` must not reference `PLAN_PLUGIN`
(section 09: never installed into `~/.claude`). In `src/assets/mod.rs` `tests_table`, add
`("PLAN_PLUGIN", PLAN_PLUGIN)` to `groups()` (array length 5) and a test that `PLAN_PLUGIN` has
`.claude-plugin/plugin.json`, all nine `skills/<name>/SKILL.md` and both `agents/*.md`.
`build/assets.rs` is 307 lines; stay under 400.

## 2. Module layout and public surface (`src/planning/guidance/`)

Plan 1 created `loom::planning` (`src/planning/mod.rs`) with `checklist`, `project`, `slug`,
`metaplan`, `vision`; plan 4 added `planning::summary` with `config::SummaryModels`. Re-read
their public items before writing (`loom map --outline src/planning/checklist.rs` and so on).
Every file under 400 lines, every function under 50.

```rust
// start.rs
pub enum AuthoringStart {
    Offer(crate::planning::checklist::Offer),
    Refine { target: PathBuf, passage: bool },
    Knowledge { open_file: Option<PathBuf> },
    /// A chat started by sending a message (section 08 "Starting a chat" 3): a refinement
    /// session whose opening message is the user's text; `paths` are the attachment paths
    /// parsed from it with `plan_session::attachments::parse_message`.
    Message { paths: Vec<PathBuf> },
    NoStep,
}
impl AuthoringStart {
    pub fn from_offer(offer: &Offer) -> Option<AuthoringStart>
    /// Looks `id` (`Offer::id` form, e.g. `write-section:05`) up among `checklist.primary`
    /// and `checklist.secondary`; None when absent or not an authoring offer. Two start ids
    /// that are not checklist offers (plan 9 sends them): `refine:<path relative to doc/>`
    /// (and `refine-passage:<path>`) for a refinement started on a document, and
    /// `knowledge` / `knowledge:<path relative to doc/>`. Their path must pass
    /// `planning::paths::check_relpath_syntax` and name a document in `project`; anything else
    /// is None. (Signature therefore takes the project.)
    pub fn from_offer_id(id: &str, project: &Project) -> Option<AuthoringStart>
    /// The label the chat shows on the opening message (section 08 "Opening messages"):
    /// e.g. `Write section 05 — Concept mode`, `Write PLAN-delivery-truth.md`, `Refine
    /// doc/plans/PLAN-x.md`, `New chat`.
    pub fn label(&self, project: &Project) -> String
}
pub enum StepClass { Spec, Plan }
pub fn step_class(start: &AuthoringStart) -> StepClass
// mod.rs
pub enum PressureCommand { Pressure, Address }
pub enum SessionGuidanceKind {
    Authoring(AuthoringStart),
    Summary { ids: Vec<String> },
    Pressure { command: PressureCommand, plan_rel: String, brief: String, model: String, effort: String },
}
pub struct SessionGuidance { pub brief_path: PathBuf, pub plugin_dir: PathBuf,
    pub opening_message: String, pub model: String, pub effort: String, pub display_name: String }
pub fn render_session_guidance(kind: &SessionGuidanceKind, project: &Project,
    runtime: &crate::planning::runtime::RuntimePaths, session_dir: &crate::plan_session::kind::SessionDir, models: &GuidanceModels)
    -> anyhow::Result<SessionGuidance>
// SessionDir is plan 6's (`plan_session::kind`): use its `brief()` (brief.md) and
// `plugin()` (plugin/) accessors, never hand-joined names.
pub const CLOSING_MESSAGE: &str = "...";
/// Launch block (plan 11 sets it when a Codex step changed session configuration that runs
/// code: `.claude/`, `.mcp.json`, `CLAUDE.md`). It is ON DISK so a restart keeps it:
/// `<runtime dir>/config-block.json` (plan 1's `RuntimePaths::dir`), written 0600 through
/// `crate::fs::safe_fs` (no-follow), holding `{ "reason": .., "changed": [paths], "set_at": RFC 3339 }`.
/// While the file exists, `render_session_guidance` refuses EVERY kind (Authoring, Summary and
/// Pressure: a second pressure run would load the same hooks) with an error naming the reason,
/// and writes nothing.
pub struct ConfigBlock { pub reason: String, pub changed: Vec<String>, pub set_at: String }
pub struct StartBlocks;
impl StartBlocks {
    pub fn block(runtime: &RuntimePaths, reason: &str, changed: &[String]) -> anyhow::Result<()>
    pub fn clear(runtime: &RuntimePaths) -> anyhow::Result<()>          // removes the file
    pub fn current(runtime: &RuntimePaths) -> anyhow::Result<Option<ConfigBlock>>
}   // render_session_guidance reads it through the `runtime` argument: the SERVER's RuntimePaths
    // (`ctx.runtime()`), never a fresh `runtime_paths(root)`, which honours LOOM_PLAN_STATE inside a
    // session and can name a different directory; an unreadable or malformed file counts as
    // blocked (fail closed)
// opening.rs
pub fn opening_message(start: &AuthoringStart, project: &Project) -> anyhow::Result<String>
pub fn summary_opening_message(ids: &[String]) -> String
pub fn pressure_opening_message(cmd: PressureCommand, plan_rel: &str) -> String
// models.rs
pub struct ModelEffort { pub model: String, pub effort: String }
pub struct GuidanceModels { pub spec: ModelEffort, pub plan: ModelEffort,
    pub summary_session: ModelEffort, pub document_agent: ModelEffort, pub overview_agent: ModelEffort }
impl GuidanceModels {
    pub fn resolve(project: &PlanConfig, user: &UserConfig) -> GuidanceModels
    pub fn load(root: &Path) -> anyhow::Result<GuidanceModels>
}
```

Re-export these from `mod.rs`. These names are what the frozen contracts call.

## 3. Offers, steps, targets, skills (`start.rs`, `opening.rs`)

`Offer` (plan 1) variants and their rows (section 09 "Opening messages" table):

| Start | Step label | Target line (second line of the opening message) | Skill | Class |
| --- | --- | --- | --- | --- |
| `Offer::StartVision` | `step 1, Vision` | `Write doc/product/index.md, the product vision.` | `vision` | Spec |
| `Offer::FixVisionTable` | `step 1, Vision` | `Fix the "Specification sections" table in doc/product/index.md so the app can read it.` | `vision` | Spec |
| `Offer::AddSectionToVision { number }` | `step 2, Product spec` | `Add doc/product/section_<nn>.md to the "Specification sections" table in doc/product/index.md.` | `vision` | Spec |
| `Offer::WriteSection { number }` | `step 2, Product spec` | `Write doc/product/section_<nn>.md, listed in the vision as "<the table row's text>".` | `section` | Spec |
| `Offer::StartMetaplan` | `step 3, Metaplan` | `Write doc/plans/index.md, the first metaplan.` | `metaplan` | Plan |
| `Offer::WritePlan { metaplan, slug }` | `step 4, Plans` | `Write doc/plans/PLAN-<slug>.md for the entry "<entry heading text>" in <metaplan path>.` | `plan` | Plan |
| `Offer::AddPlanToMetaplan { slug }` | `step 4, Plans` | `Add an entry for PLAN-<slug>.md to a metaplan in doc/plans/.` | `metaplan` | Plan |
| `Offer::RefineSpec` | `done, Review what was built` | `Review the done plans and their run reviews against the product spec.` | `retro` | Spec |
| `Offer::NewMetaplanRound` | `done, New metaplan round` | `Write doc/plans/metaplan-<nn>.md, the next metaplan round.` (`<nn>` = next free two-digit number among existing `doc/plans/metaplan-<nn>.md`, starting at 02 when only `index.md` exists) | `metaplan` | Plan |
| `Refine { target, .. }` | `any, Refine` | `Refine <target>.` plus ` The message that follows attaches the passage.` when `passage` | `refine` | Spec when `target` starts with `doc/product/`, else Plan |
| `Message { paths }` | `any, Refine` | (none: the opening message is the user's text, verbatim) | `refine` | Plan when the first path starts with `doc/plans/`, else Spec |
| `Knowledge { open_file }` | `any, Knowledge` | `Change the knowledge base under doc/loom/knowledge/` plus `, starting from <file>` when set, then `.` | `knowledge` | Spec |
| `NoStep` | none | (see below) | none | Spec |
| `Offer::PressureTest`, `Offer::StartRun` | `from_offer` returns `None` | | | |

`<nn>` is the zero-padded two-digit number. The table row's text for `WriteSection` is the
vision line at `VisionRow::line` (plan 1's `vision::parse_sections_table`), trimmed, with its
outer `|` removed and cells joined by ` | `, cut to 120 characters with `…` on a char boundary.
The entry heading text is plan 1's `MetaplanEntry::heading` with the `## ` prefix and the
trailing `` (`PLAN-…`) `` removed. A missing row or entry is an error naming the offer id
(`Offer::id`).

Opening message, three lines (section 09):

```text
loom plan: <step label>.
<target line>
Load the skill loom-plan:<skill>, then begin by telling me what you read and what you plan to ask.
```

`NoStep`: `loom plan: no step chosen.` / `Ask me what I want to work on.` (two lines).
Summary: `loom plan: rebuild summaries.` / `Rebuild: <ids joined by ", ">.` / `Load the skill
loom-plan:summaries.` Pressure: exactly `/loom-plan:pressure <plan_rel>` or
`/loom-plan:address <plan_rel>`. Display names (section 07 "Naming"): authoring `loom plan:
<skill name or "chat">` plus ` <nn>` for a section (`loom plan: section 07`), summary `loom
plan: summaries`, pressure `loom plan: pressure` / `loom plan: address`.

## 4. Briefs (`brief.rs`, `text/*.md`)

`text/common_brief.md` holds the common body in section 09's order ("The common brief", items 1
to 6): what the session is (browser chat, plain-text questions one at a time, no question
tool, typed slash commands are refused so never suggest one, do not invoke
`superpowers:brainstorming` or `superpowers:writing-plans`; discussions run through
`loom-plan:brainstorming`); documents are the truth and the next session starts cold (record
each decision in the document it binds when it settles); the conventions of section 03 "What
the writing sessions keep to" QUOTED as its seven bullets (copy `doc/product/section_03.md`
lines 243-251 verbatim); the write limits of section 09 "Write limits" (all six bullets); the
turn-end duty (item 5, with the two-wave rebuild of "Summaries after a turn"); the closing
message: expect it, follow it, ask nothing after it, and reply `Closing write done.`; and where
a `CLAUDE.md` names other places for planning documents, follow this brief and say so once.
Include the text with `include_str!`. Under 120 lines.

`brief.rs`: `common_brief(start: &AuthoringStart, project: &Project) -> anyhow::Result<String>`
returns the body plus the starting-point block:

```markdown
## Starting point

Step: <n>, <Step name>.            (or `Step: none chosen.`, `Step: any, Refine.`, `Step: done, Review what was built.`)
Target: <target line without its leading verb sentence form; reuse the opening message's target line>
Skill: loom-plan:<skill>.          (omitted for NoStep)

Project state at launch (it may change; re-read the files before relying on it):
- Vision: present, <L> sections listed, <W> written (<nn>, <nn>, ...).   | `- Vision: absent.`
- Metaplans: none.                   | `- Metaplans: <path>, <path>.`
- Plans: <w> written (<u> unlisted), <r> running, <d> done, <t> retired.
- Locked plan files: none.           | `- Locked plan files: <path>, <path>.`
```

All counts come from `project.checklist.state` (plan 1's `ProjectState`: `vision_present`,
`sections_listed`, `sections_written`, `metaplans`, `plans_unlisted`, `plans_by_state`,
`locked_files`); the locked line lists only locked PLAN files (`locked_files` filtered to
paths whose name carries `IN_PROGRESS-`, `DONE-` or `RETIRED-`), in path order. (Plan line:
the spec example's wording was ambiguous; this form is the plan's decision.)

`text/summary_brief.md`: section 09 "Summary sessions": rebuild summaries and nothing else; never
ask the user anything; follow `loom-plan:summaries`; the write limits bullets on knowledge,
summaries, and no git or execution. Under 40 lines. The summary kind's brief is this text alone.
The pressure kind's brief is the `brief` string the caller passes, alone.

`CLOSING_MESSAGE` is section 09's text, lines 320-325 of `doc/product/section_09.md`, verbatim,
with `\n` line breaks and the indentation of its continuation line kept.

## 5. Rendering (`plugin.rs`, `render_session_guidance`)

- `session_dir.path()` must exist, be a directory and not a symlink (`std::fs::symlink_metadata`);
  `session_dir.plugin()` and `session_dir.brief()` must not exist yet (any kind of entry,
  symlink included, is an error naming the path). Fail closed; write nothing on error.
- Create directories with mode 0700 and files with 0600 using `OpenOptions::new()
  .write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW)`; never follow a
  symlink; never `create_dir_all` through an existing path you did not create.
- Kind parts: Authoring writes `.claude-plugin/plugin.json`, every `skills/**` entry and both
  agents from `crate::assets::PLAN_PLUGIN`; Summary writes `.claude-plugin/plugin.json`,
  `skills/summaries/**` and both agents; Pressure writes `.claude-plugin/plugin.json` and
  `commands/pressure.md`, `commands/address.md` taken from `crate::assets::CLAUDE_COMMANDS`
  (keys `pressure.md`, `address.md`) byte for byte. No other bytes are written into `plugin/`.
- Agent rendering: in `agents/summary.md` replace the frontmatter's single `model:` line with
  `model: <document_agent.model>` and the `effort:` line with `effort:
  <document_agent.effort>`; `agents/overview.md` the same from `overview_agent`. Only lines
  inside the first `---` ... `---` block are touched. A source agent file without exactly one
  `model:` and one `effort:` line there is an error (it means the embedded source drifted).
- `brief.md`: Authoring → `common_brief`; Summary → summary brief; Pressure → the given `brief`.
- `SessionGuidance.model/effort`: Authoring → `models.spec` or `models.plan` by `step_class`;
  Summary → `models.summary_session`; Pressure → the kind's `model`/`effort`.

## 6. Models (`models.rs`)

`GuidanceModels::resolve(project, user)`: `spec` from `project.spec_authoring_model()` else
`user.plan_spec_authoring_model()` (and the three other authoring slots likewise, as
`commands/pressure/models.rs:34-37` `slot` does); `summary_session` = overview model and effort,
`document_agent` = document-summary pair, `overview_agent` = overview pair, all from plan 4's
`SummaryModels::resolve(project, user)` (`loom::planning::summary::config`). `load(root)` reads
`.loom/work/config.toml`'s `[plan]` through `read_plan_config` when the workspace exists (as
`commands/pressure/mod.rs:305-310` `project_pressure_config` does) and `UserConfig::load()`.

## 7. Unit tests (`tests.rs`, not frozen; yours)

- every opening-message row of the table in §3 on a fixture `Project` (build one with plan 1's
  `Project::load` over a `tempfile::TempDir` holding a vision with a sections table, two
  sections, `doc/plans/index.md` with one entry, an `IN_PROGRESS-PLAN-foo.md`);
- `from_offer` returns `None` for `PressureTest` and `StartRun`;
- `step_class` for every row, including `Refine` under `doc/product/` and under `doc/plans/`;
- the brainstorming source contains none of `docs/superpowers`, `writing-plans`, `git commit`;
- every `skills/*/SKILL.md` in `PLAN_PLUGIN` starts with `---\nname: <dir>\n` and has a
  `description:` line;
- `skills/knowledge/SKILL.md` contains each of `commands/distill.md` lines 14-19 byte for byte
  (read through `CLAUDE_COMMANDS["distill.md"]`);
- the common brief contains `doc/product/section_03.md` lines 243-251 byte for byte (read the
  spec file at test time via `env!("CARGO_MANIFEST_DIR")/../doc/product/section_03.md`);
- `CLOSING_MESSAGE` equals the fenced block at `doc/product/section_09.md` lines 320-325 (read
  at test time the same way).
- `PLAN_PLUGIN`'s `.claude-plugin/plugin.json` parses as JSON with `"name": "loom-plan"`;
- `src/assets/install.rs` does not reference `PLAN_PLUGIN` (`include_str!("../../assets/install.rs")`
  from the test module's location, or read through `env!("CARGO_MANIFEST_DIR")`);
- the common brief carries section 09's six parts in order (assert the byte offsets of six
  marker phrases you put at the start of each part increase), each write-limit bullet lead
  (`Documents only`, `Locked plans`, `Run reviews`, `Knowledge`, `Summaries`, `No git and no
  execution`), and the text `superpowers:writing-plans`;
- the starting-point block on a project with no vision, no metaplan and no locked plan reads
  `- Vision: absent.`, `- Metaplans: none.`, `- Locked plan files: none.`; `NoStep` gives
  `Step: none chosen.` and no `Skill:` line;
- `summary_opening_message(&["product/section_03", "product"])` is the three-line form of
  section 09 "Summary sessions"; the summary brief contains `loom-plan:summaries` and says it
  asks the user nothing;
- `GuidanceModels::resolve` with a project `PlanConfig` setting `plan_authoring_model` and a user
  config setting it differently returns the project value (project tier wins);
- display names: `loom plan: section 07` for `WriteSection { number: 7 }`, `loom plan: plan`
  for `WritePlan`, `loom plan: summaries`, `loom plan: pressure`, `loom plan: address`, `loom
  plan: chat` for `NoStep`;
- per-skill target strings: `vision` contains `doc/product/index.md` and `Specification
  sections`; `section` contains `doc/product/section_` and `## Open questions`; `metaplan`
  contains `doc/plans/index.md` and `metaplan-`; `plan` contains `Implements:` and
  `loom-plan-writer`; `refine` contains `## Open questions`; `retro` contains `REVIEW-`;
  `knowledge` contains the line ``Do not run `loom memory pending` or `loom memory resolve`.``;
  every step skill (vision, section, metaplan, plan, refine) contains `loom plan check` and
  `loom plan summary status --json`; both agents contain `loom plan summary seal`.

## 7b. StartBlocks tests

In `tests.rs`, with `LOOM_HOME` at a `TempDir` (`#[serial_test::serial]`): after `StartBlocks::block`,
rendering an Authoring, a Summary and a Pressure kind each returns an error containing the
reason and writes neither `brief.md` nor `plugin/`; the file is mode 0600; a block written, then
a fresh `PlanGuidance` built (a simulated restart: nothing in memory), still refuses an
Authoring render; a malformed `config-block.json` refuses; after `clear` every kind renders; and
with `LOOM_PLAN_STATE` set in the environment to another directory, a block written to the server's
`RuntimePaths` (built with `planning::runtime::runtime_paths_under(<tmp loom dir>, root)`) still
refuses a render given those same `RuntimePaths` (no code path re-resolves the runtime).

## 8. Wiring into plan 7's bridge (`src/plan_server/guidance/`)

Plan 7 (`doc/plans/PLAN-feat-planner-07-session-bridge.md`, stage `bridge-runtime`; full types in
`doc/plans/briefs/feat-planner-07-session-bridge/common.md`) owns the launch: `Bridge::start_session(StartRequest
{ kind, guidance: Box<dyn FnOnce(&SessionDir) -> Result<LaunchGuidance> + Send>, opening_origin,
extra_env })` creates the session directory, then calls the closure. Authoring chats go through
`plan_server::chat::guidance::GuidanceSlot` (an extension in `ctx.extensions()`) holding
`Arc<dyn GuidanceProvider>`:
`fn authoring(&self, ctx: &PlanContext, dir: &SessionDir, offer: Option<&str>, first_message: Option<&str>) -> Result<LaunchGuidance>`
and `fn closing_message(&self) -> String`. Plan 7 installs `ChecklistGuidance`
(`plan_server/chat/guidance.rs`), which refuses an offer id the checklist does not offer now; you
replace it, and a replacing provider owns that "not offered now" refusal for its own ids.

- In `src/planning/guidance/mod.rs`: `impl From<SessionGuidance> for crate::plan_session::bridge::LaunchGuidance`
  (field for field: `brief_path, plugin_dir, opening_message, model, effort, display_name`,
  plus `opening_on_argv: false`: plan 7 pastes every opening; plan 11 sets it true for pressure).
- `src/plan_server/guidance/provider.rs`: `pub struct PlanGuidance { models: GuidanceModels, rendered: Mutex<BTreeMap<String /* session id = dir file name */, SessionBriefView>> }`
  implementing `GuidanceProvider`. `authoring`: `offer` Some(id) → `AuthoringStart::from_offer_id(id, &ctx.project())`,
  an unknown id is an error naming it (plan 7 reports it to the browser); `offer` None and
  `first_message` Some(text) → `AuthoringStart::Message { paths }` with paths from
  `plan_session::attachments::parse_message(text).1`, and the opening message is `text`
  verbatim (override `SessionGuidance.opening_message`); both None → `NoStep`. Render with
  `render_session_guidance(&SessionGuidanceKind::Authoring(start), &ctx.project(), ctx.runtime(), dir,
  &self.models)`, remember the brief text, model, effort, display name and label under the
  session id, and return `.into()`. `closing_message` returns `CLOSING_MESSAGE`.
  `GuidanceModels::load(ctx.root())` is read once at `install` (config changes apply to the
  next server start, as every `[plan]` key does).
- `src/plan_server/guidance/brief_route.rs`: `GET /api/session/brief?session=<id>` (Role
  Viewer; the Brief link of section 08 "Starting a chat" opens it read-only, read-only browsers
  included) → `SessionBriefView { session_id, label, brief, model, effort, display_name }`
  (serde snake_case; a wire type in this file, registered with plan 2's parity pattern: fixture
  `../web/src/plan/api/fixtures/session-brief.json` and a `session_brief_fixture_matches_serde_output`
  test). An id not in the map is 404 `unknown_session`; ids are matched exactly against the
  map's keys (never joined into a path).
- `src/plan_server/guidance/mod.rs`: `pub fn routes() -> Vec<RouteSpec>` (the brief route) and
  `pub fn install(ctx: &Arc<PlanContext>)` calling `GuidanceSlot::set(Arc::new(PlanGuidance::new(..)))`.
  Add `guidance::routes()` to `src/plan_server/routes.rs::all_routes()` and `guidance::install(ctx)`
  to `install_all()` AFTER plan 7's chat install (so the slot exists).
- `first_message` is plan 7's FORMATTED message (the user's text plus the `<loom-attachments>` block,
  as plan 7 delivers it by paste after `SessionStart`; plan 7 never puts it on the `claude` argv).
  Parse its attachment paths with `parse_message`; your provider test passes a formatted block.
- Summary and pressure sessions do not go through the slot: plans 10 and 11 call
  `render_session_guidance` inside the `guidance` closure they pass to `Bridge::start_session`
  (`Box::new(move |dir| render_session_guidance(&kind, &project, &runtime, dir, &models).map(Into::into))`,
  `runtime` a clone of the server's `ctx.runtime()`).
- Plan 7's tests that asserted `FixedGuidance` output keep passing (they install it
  themselves); add `tests.rs` cases: a WritePlan offer id renders the plan step model; an
  unknown offer id errors; a first message with an attachment under `doc/plans/` uses the plan
  step model and keeps the text as the opening message; the brief route returns 404 for an
  unknown id and the stored brief for a rendered one.

## 9. Operator test (`tests/planning_guidance_operator.rs`)

One `#[test] #[ignore = "needs a logged-in Claude Code, tmux and network"]` function per skill
family: `operator_vision_session_loads_its_skill`, `operator_section_session_loads_its_skill`,
`operator_metaplan_session_loads_its_skill`, `operator_plan_session_loads_its_skill`. Each first
returns early when `loom::process::sandbox_probe::skip_unless(which::which("tmux").is_ok() &&
which::which("claude").is_ok(), <test name>, "tmux or claude missing")` is true (so
`LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns a missing prerequisite into a failure, never a pass),
builds a scratch git repository in a `TempDir` (a vision listing two sections with one written,
`doc/plans/index.md` with one entry), starts the planning server in-process with `loom::plan_server::start(ServerOptions { root, host: "127.0.0.1".parse()?, port: Some(0), read_only: loom::plan_server::prereq::ReadOnly::from_problems(vec![]) })` (plan 2's public API; `LOOM_HOME` at a `TempDir` so the runtime directory is the test's own), which returns `RunningServer { local, token, ctx }`; HTTP requests first `GET /?token=<token>` with `Host: 127.0.0.1:<port>` (`port = local.port()`) to collect the two cookies from the 302 (`loom_plan_<port>`, `loom_plan_browser_<port>`), then send them with `Origin: http://<local>` on every request (the shape of plan 2's `bare_loom_plan_serves_the_planning_app` contract); `RunningServer::stop` at the end. Plan 2's `#[cfg(test)]` helpers are not reachable from an integration test target, starts a chat through plan 7's `Bridge::start_session` (reached as `running.ctx.extensions().get::<Bridge>()`) with the
family's offer (`StartVision`, `WriteSection { number: 2 }`, `StartMetaplan` on a repo without a
metaplan, `WritePlan`), waits up to 5 minutes for the first main `Stop`, and asserts the transcript
(`Bridge::transcripts(id)`) holds a `Skill` tool use naming `loom-plan:<skill>`. It then sends
"Write the document now with what you know; ask nothing further." through `Bridge::send`, waits
for the next `Stop` (up to 10 minutes), asserts the target path (`doc/product/index.md`,
`doc/product/section_02.md`, `doc/plans/index.md`, `doc/plans/PLAN-<slug>.md`) exists with a
level-1 heading, ends the session with `EndMode::Teardown`, and prints the transcript path. It
compiles in the stage's build; only the operator runs it.
