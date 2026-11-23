# planner-durability / w2-pressure: Codex configuration baseline on disk; private, no-follow pressure log

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/planner-backend.md` section P4; `validation/security.md` section CR-18; `validation/cli-tui-webserver.md` section CR-77 (the pressure `--dry-run` bullet only).

## Findings covered
- P4: the pre-Codex configuration snapshot lives only in `CodexHalf::before`; a server that dies while Codex runs never compares it, so a configuration edit raises no confirmation.
- CR-18: `loom pressure` writes the Codex log at a predictable temp path with `File::create` (follows a symlink, truncates its target), and two readers load the whole log.
- CR-77 (dry-run bullet): `render_dry_run` prints argv with `join(" ")`, so arguments holding spaces or `$` read ambiguously.

## Files
Own (write), paths under `loom/`:
- `src/plan_server/pressure/config_guard.rs`, `src/plan_server/pressure/baseline.rs` (new), `src/plan_server/pressure/mod.rs`
- `src/plan_server/pressure/codex.rs`, `src/plan_server/pressure/install.rs`, `src/plan_server/pressure/recover.rs`
- `src/plan_server/pressure/tests.rs` (one `mod` line), `src/plan_server/pressure/tests/recovery_baseline.rs` (new)
- `src/pressure/codex_run.rs`, `src/pressure/codex_run/log_file.rs` (new), `src/pressure/codex_run/tests.rs`
- `src/commands/pressure/paths.rs`, `src/commands/pressure/mod.rs`, `src/commands/pressure/spawn.rs`, `src/commands/pressure/tests.rs`

Read only: `src/planning/guidance/blocks.rs` (`StartBlocks`), `src/fs/safe_replace.rs`, `src/fs/safe_read.rs` and `src/fs/safe_read/tail.rs` (`read_tail`), `src/testrun/command.rs` (`shell_words`), `src/plan_session/bridge/recovery.rs`, `src/plan_server/pressure/tests/duties.rs`, `src/plan_server/pressure/tests/fake.rs`, `src/plan_server/pressure/fixtures/codex.sh`, `src/plan_server/summaries/codex_spawner.rs`.

Frozen (contract tests, never edit): `src/plan_server/pressure/tests/baseline_contract_tests.rs`, `src/pressure/codex_log_contract_tests.rs`.

## Changes

### 1. P4: persist the baseline, check it at recovery
- `plan_server/pressure/config_guard.rs` `ConfigSnapshot` (line 47-50): add `Serialize, Deserialize` to the derive (`use serde::{Deserialize, Serialize};`). Nothing else changes there.
- New `plan_server/pressure/baseline.rs`, declared `mod baseline;` in `plan_server/pressure/mod.rs`. File `config-baseline.json` in `runtime.dir` (pin this name: a contract checks it).
  - `pub(super) fn save(runtime: &RuntimePaths, before: &ConfigSnapshot) -> anyhow::Result<()>`: `crate::fs::safe_replace::replace_private(&runtime.dir, Path::new(BASELINE_FILE), &serde_json::to_vec(before)?)` (atomic; a crash mid-write leaves the old file or none).
  - `pub(super) fn load(runtime: &RuntimePaths) -> anyhow::Result<Option<ConfigSnapshot>>`: `read_bounded(&runtime.dir, Path::new(BASELINE_FILE), MAX_BASELINE_BYTES)` with `MAX_BASELINE_BYTES = 32 * 1024 * 1024`; not found (`safe_read::is_not_found`) is `Ok(None)`; read or parse failure is `Err`.
  - `pub(super) fn clear(runtime: &RuntimePaths) -> anyhow::Result<()>`: `safe_remove_in_workdir` through `safe_open_dirfd(&runtime.dir)`; a missing file is `Ok`.
- `plan_server/pressure/codex.rs` `CodexHalf::start` (line 51-92): after `snapshot`, `baseline::save(env.ctx.runtime(), &before)`, mapping an error to `Err(format!("cannot record the session configuration before Codex: {error:#}"))` (no Codex starts without a saved baseline). When `spawn_codex_with_fallback` fails, call `baseline::clear` before returning its error.
- `CodexHalf::configuration_changed` (line 252-273): keep the current decision, then clear the baseline on every path before returning (it is the only caller of the comparison; `finish` and `abandon` both reach it). A clear that fails prints `pressure run: the configuration baseline was not removed: <error>` through `line(&..)` with `safe(..)`; the run goes on.
- `plan_server/pressure/recover.rs`: add `pub(super) fn check_baseline(root: &Path, runtime: &RuntimePaths, report: &RecoveryReport, grace: Duration) -> Option<String>` returning the terminal line, or `None` when no baseline exists or nothing changed:
  1. `baseline::load`; `Ok(None)` returns `None`.
  2. Codex children of every pressure session in `report.torn_down` and `report.closed`: poll `verify_process_identity(child.identity())` every 100 ms until none is `VerifiedAlive` or `Unverifiable`, at most `grace`. Still running: reason `NOT_EXITED`, no paths.
  3. Otherwise `snapshot(root)` and `changed(&before, &after)`: empty means `baseline::clear` and `None`; non-empty gives `format!("{CONFIG_CHANGED}: {}", changed.join(", "))` with those paths. A failed load or snapshot gives `format!("{CONFIG_CHANGED}: cannot be checked after the last server stopped ({error:#})")`.
  4. `StartBlocks::block(runtime, &reason, &changed)`; only when that succeeded, `baseline::clear` (a failed block keeps the baseline for the next start). Return `Some(format!("pressure run: {}", safe(&reason)))`, or the not-saved line when the block failed.
  `CONFIG_CHANGED` and `NOT_EXITED` are `pub(super)` in `codex.rs`; import them.
- `plan_server/pressure/install.rs` `install_with` (line 188-193): capture `ctx.runtime().clone()` and `tools.codex_exit_grace` before `tools` moves; inside the `on_recovered` closure, after the `interrupted_runs` lines, print `check_baseline(&root, &runtime, report, grace)` when it is `Some`. Recovery has sent SIGTERM to verified children by then (`bridge/recovery.rs` `clean`), which is why the grace wait exists.

### 2. CR-18: the pressure log
- `commands/pressure/paths.rs` `codex_log_path` (line 17-19): becomes `pub(super) fn codex_log_path(repo_root: &Path) -> PathBuf` returning `repo_root/.loom/work/pressure/codex-<pid>.log`, beside `claude_marker_path`. Doc: inside the repository's own pressure directory, never the shared temp directory.
- `commands/pressure/mod.rs` `execute` (line ~208): `codex_log_path(&repo_root)`. After the `args.dry_run` branch returns, `std::fs::create_dir_all(codex_log.parent())` with context `cannot create <dir>` (the directory may not exist yet; `.loom/work` may be a symlink in a worktree, which `create_dir_all` follows by design).
- New `pressure/codex_run/log_file.rs`, declared `mod log_file;` in `codex_run.rs`. Move `UNAVAILABLE_PHRASES` and `fn unavailable_line` (`codex_run.rs` line 22-42) into it unchanged and `use log_file::unavailable_line;` in `codex_run.rs` (the tests reach it through `use super::*`). `codex_run.rs` is at 400 lines: this move is what makes room.
  - `pub(super) fn create_log(path: &Path) -> anyhow::Result<std::fs::File>`: if `symlink_metadata(path)` says regular file, remove it (a previous attempt's log: each attempt re-creates the log); any other existing entry (a symlink, a directory) fails with `refusing the codex log {}: it is not a regular file`; then `OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC).open(path)`. `create_new` is `O_EXCL`: an entry planted between the removal and the open fails the open instead of being followed. Comment that constraint.
  - `pub(super) const LOG_SCAN_BYTES: u64 = 256 * 1024;` and `pub(super) fn tail(path: &Path, max_bytes: u64) -> Option<Vec<u8>>`: `crate::fs::safe_read::read_tail(parent, Path::new(file_name), max_bytes).ok()` (no follow, regular files only).
- `codex_run.rs` `spawn_codex_logged` (line 375-376): `log_file::create_log(log_path)` with the existing context. `CodexJob::unavailable_line` (line 191-195): scan `log_file::tail(&self.log_path, LOG_SCAN_BYTES)` instead of `std::fs::read`.
- `commands/pressure/spawn.rs` `print_log_tail` (line 37-42): print the lossy text of `read_tail(parent, file_name, max_bytes as u64)`; unreadable prints nothing, as today. Make `log_file::tail` `pub(crate)` and call it, so the parent/name split exists once.
- The plan server's `summaries/codex_spawner.rs` and `plan_server/pressure/codex.rs` pass their own session-directory logs through the same `spawn_codex_logged`; they get the no-follow open without an edit.

### 3. CR-77: quoted dry-run argv
`commands/pressure/mod.rs` `render_dry_run_step` (line 90-122): the three `claude_args(..).join(" ")` / `codex_args(..).join(" ")` become `crate::testrun::command::shell_words(args.iter().map(String::as_str))`. The output is still display text, never run.

## Interfaces shared with other workers
w1 moves `RecoveredSession` and `RecoveryReport` from `plan_session/bridge/mod.rs` into `bridge/recovery.rs` and re-exports them at the same path, `crate::plan_session::bridge::{RecoveredSession, RecoveryReport}`, with unchanged fields (`id`, `kind`, `children: Vec<ChildRecord>`). Import them by that path only.

## Tests
- P4, new `plan_server/pressure/tests/recovery_baseline.rs` (add `mod recovery_baseline;` to `plan_server/pressure/tests.rs`; `duties.rs` is 365 lines, leave it), helpers `Fixture::new`, `Fixture::restart`, `call_on`, `f.codex(..)`, `f.fake.script(vec![Script::Hold])`, all `#[serial]`:
  - `a_baseline_with_no_edit_leaves_no_block`: save a baseline of the unchanged project through `baseline::save(f.ctx.runtime(), &snapshot(&f.root).unwrap())`, restart with a report holding one torn-down pressure session, assert no `config-block.json` and no `config-baseline.json`.
  - `an_unreadable_baseline_blocks`: write `config-baseline.json` as `not json`, restart, assert the block exists and its reason contains `cannot be checked`.
  - `a_finished_run_clears_its_baseline`: `f.start(1)` with the default fake Codex (exits at once), `f.ended()`, then no `config-baseline.json`; and a `wait_until` during a held run (`hold_first_step` pattern of `tests/authoring.rs:29`) sees the file exist while Codex runs.
  The first two fail today (no baseline is read at recovery); the third fails today on its "exists while Codex runs" half.
- CR-18, `commands/pressure/paths.rs` `tests`: `codex_log_path(repo)` starts with `repo/.loom/work/pressure` and is not under `std::env::temp_dir()`. `pressure/codex_run/tests.rs`: `a_previous_regular_log_is_replaced` (spawn twice on one path; the second log holds only its own output) and `a_directory_at_the_log_path_is_refused`.
- CR-77, `commands/pressure/tests.rs`: `test_render_dry_run_quotes_arguments_with_spaces` asserts the codex line ends with `-C /repo '$pressure doc/plans/PLAN-foo.md'`.
- Contracts to turn green: `abrupt-death-loses-baseline`, `codex-log-follows-symlink`.

## Proof
From `loom/`: `cargo check --all-targets`; `cargo test --lib pressure::codex_run::`; `cargo test --lib commands::pressure::`; `cargo test --lib plan_server::pressure::`. `wc -l src/pressure/codex_run.rs` must be under 390 after `cargo fmt`.

## Existing test that changes
`commands/pressure/tests.rs` `test_render_dry_run_shows_real_argv` line 134-137: the expected codex line becomes `codex exec --sandbox workspace-write -m gpt-6.2-sol -c model_reasoning_effort=xhigh -C /repo '$pressure doc/plans/PLAN-foo.md'` (the prompt argument is now quoted). Change only that literal and report it.

## Traps
- Writing a `StartBlocks` block before launch instead of a baseline: it refuses the run's own Claude sessions (`planning/guidance/mod.rs:117`).
- Keeping the baseline in the session directory: recovery removes that directory before `on_recovered` runs.
- `O_NOFOLLOW` alone after removing whatever is at the path passes the symlink contract's target check but not its refusal check; refuse a non-regular entry.
- Comparing right after recovery without waiting for the SIGTERMed Codex to exit lets a late write escape the check.
- `codex_run.rs` past 400 lines fails `cargo test --test maintainability`.
