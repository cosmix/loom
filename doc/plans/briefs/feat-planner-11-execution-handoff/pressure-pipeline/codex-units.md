# Codex units of stage `pressure-pipeline`

These are IMPLEMENTER units: `loom-codex-forwarder` runs that write code for this stage. They are
unrelated to the Codex `$pressure` step this plan's code drives (`codex exec`), which is the
product under construction, not a lane.

Each unit: the model flags in its block below (always `--effort high`), FOREGROUND, Bash timeout 600000 ms. A unit
writes only its files, runs no git and no tests (compile checks at most, `cargo check --lib`
from `loom/`), and touches no dot-loom or dot-work path. The orchestrator runs the proof command
right after the unit, then `git status --short`. Stay 10% under the limits (360 lines per file, 45
per function). Double literal braces inside `format!`; CLI placeholders in rustdoc go in
backticks. `loom map` answers from the BASE tree, so this file names what earlier work added.

## Wave 1 (foreground, while P1 runs in the background)

### R1 `src/pressure/record.rs`

- Model: `--model gpt-6.1-sol --effort high`.
- Owns: `loom/src/pressure/record.rs` (ending with `#[cfg(test)] mod tests;`).
- Spec: `doc/plans/briefs/feat-planner-11-execution-handoff/pressure-pipeline/p2-record-writer.md`, sections
  "API", "render", "append_to_text placement", "append_record". Implement exactly that API:
  `RecordLine`, `RecordLine::render`, `RecordWrite`, `append_record`, `append_to_text`.
- Context: P1 (Claude, same stage) creates `loom/src/pressure/mod.rs` with `pub mod record;`. Until
  P1 lands, the crate does not see your file; do not create `mod.rs`.
- Traps: `O_NOFOLLOW` via `std::os::unix::fs::OpenOptionsExt::custom_flags(libc::O_NOFOLLOW)`;
  keep the original mode (`PermissionsExt`); re-check the inode before `rename`; CRLF handling.
- Security-relevant (it writes the user's plan file). Frozen contracts in
  `loom/tests/pressure_pipeline_contracts.rs` pin it: `record_keeps_the_plan_file_mode`,
  `moved_or_symlinked_plan_is_not_written`, `record_appends_after_existing_section_items`,
  `record_creates_section_at_end_when_absent`, `record_ignores_a_heading_before_the_marker`,
  `record_line_renders_rounds_models_and_outcome`; integration-verify's security review reads this file.
- Proof (orchestrator, after P1 and R2): `cargo test --lib pressure::record::` and
  `cargo test --test pressure_pipeline_contracts`.

### R2 `src/pressure/record/tests.rs`

- Model: `--model gpt-6.1-sol --effort high`.
- Owns: `loom/src/pressure/record/tests.rs`.
- Spec: p2-record-writer.md "Tests". Use `super::*` for R1's API, which is exactly:
  `pub struct RecordLine { pub date: chrono::NaiveDate, pub claude_model: String, pub codex_models: Vec<String>, pub rounds: u32, pub missing_rounds: Vec<u32> }`,
  `impl RecordLine { pub fn render(&self) -> String }`, `pub enum RecordWrite { Written, PlanMoved }`,
  `pub fn append_record(plan_path: &Path, line: &RecordLine) -> anyhow::Result<RecordWrite>`,
  `pub fn append_to_text(text: &str, rendered: &str) -> String`; and `tempfile::TempDir`.
- Proof: `cargo test --lib pressure::record::`.

### T1 built-in backend default

- Model: `--model gpt-6.1-terra --effort high`.
- Owns: `loom/src/models/session/types.rs` and the one assertion at `loom/src/user_config/tests.rs:94`.
- Spec: p3-tmux-default.md edits 1 and 3: move `#[default]` on `SessionBackendKind` from `Native`
  to `Tmux`, update the two variant doc comments, change the test assertion to `Tmux`.
- Also report (do not edit): every `SessionBackendKind::default()` and `#[serde(default)]` reader
  and whether each spawn path assigns `backend` before the session file is written
  (`rg -n '\.backend = ' src/orchestrator`), for the orchestrator's audit.
- Proof: `cargo test --lib models::session::` and `cargo test --lib user_config::`.

### T2 init prompt label

- Model: `--model gpt-6.1-terra --effort high`.
- Owns: `loom/src/commands/init/backend.rs` and `loom/src/commands/init/backend_tests.rs`.
- Spec: p3-tmux-default.md edit 2: `backend_default_label(None)` returns `tmux, built-in default`;
  update `backend_tests.rs:63`; rename `init_backend_effective_backend_defaults_to_native_without_user_config`
  to `..._defaults_to_tmux_without_user_config` asserting `SessionBackendKind::Tmux`.
- Proof: `cargo test --lib init::`.

### T3 docs

- Model: `--model gpt-6.1-terra --effort high`.
- Owns: `README.md` (repo root) and `skills/loom-usage/SKILL.md`.
- Spec: p3-tmux-default.md edit 6 (README lines naming native as the default, plus the one new
  sentence); in the skill, only lines that call native the default.
- Proof: `rg -n -i 'native.*default|default.*native' README.md skills/loom-usage/SKILL.md` prints nothing that still calls native the built-in default.

### T4 dashboard fixtures

- Model: `--model gpt-6.1-terra --effort high`.
- Owns: `web/src/api/fixtures/config.json` and `web/src/test/settings-kit.tsx`.
- Spec: p3-tmux-default.md edit 5: in the `terminal.backend` entry the `default`, `user.value`,
  `project.value` and `effective.value` become `"tmux"`; the same four in `settings-kit.tsx`.
  Change no other byte. Do not run `oxfmt`; the orchestrator formats.
- Proof: `cargo test --lib config_api::` and, from `web/`, `node_modules/.bin/vitest run src/components/settings-model.test.ts`.
