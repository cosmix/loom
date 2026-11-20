# Brief: fullscreen-rust (#35 detection, last-screen save, status kind)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/models/session/types.rs`, `loom/src/models/session/methods.rs`, `loom/src/fs/session_files/exact.rs`, `loom/src/orchestrator/terminal/tmux/capture.rs`, `loom/src/orchestrator/terminal/tmux/history.rs`, `loom/src/orchestrator/terminal/tmux/fullscreen.rs` (new), `loom/src/orchestrator/terminal/tmux/mod.rs`, `loom/src/orchestrator/terminal/tmux/tests_history.rs`, `loom/src/orchestrator/monitor/core.rs`, `loom/src/status/data/stopped.rs`, `loom/src/status/data/mod.rs`, `loom/src/status/attention_model.rs`, `loom/src/commands/status/web/model.rs`, `loom/src/commands/status/web/model_tests.rs`, `web/src/api/fixtures/snapshot.json`, `web/src/api/fixtures/statuses.json`.

Frozen, read-only: `loom/src/orchestrator/terminal/tmux/history_contract_tests.rs`, `loom/src/status/data/stopped_contract_tests.rs`.

## Facts (verified)

- Poll loop: `orchestrator/monitor/core.rs:145-192` `poll()`, every 5 s; `reconcile_codex_evidence` at :150 is the per-poll hook pattern. Liveness is PID-only (`native::pid_only_is_alive`).
- tmux helpers: `capture.rs:64` `exact_pane_target` (private, make `pub(super)`), `capture.rs:75-94` `capture_stdout` (bounded, socket check), `capture.rs:38-56` `capture_history_argv` (`capture-pane -p -e -J -S - -E -`).
- Teardown: `tmux/backend.rs:123-142` `kill_session` calls `history::save_history` first. The `Session` argument may be stale; read the flag from disk with `fs::session_files::exact::load_session_exact`.
- Session persistence: `models/session/types.rs:119-168`, constructor `methods.rs:8-30`, delta-writer pattern `fs/session_files/exact.rs:88-120` (`record_session_context_exact`).
- Status: `status/data/stopped.rs:43-63` `terminal_history_session` (newest ended tmux session with a history file), field `status/data/mod.rs:253-257`, attention `status/attention_model.rs:54,156-167,290-293`, web model `commands/status/web/model.rs:141-142,197`. Fixture pin `model_tests.rs:146-156` reads `web/src/api/fixtures/snapshot.json`.

## Change

1. `Session.fullscreen_seen: bool` with `#[serde(default, skip_serializing_if = "std::ops::Not::not")]`, false in `Session::new`. `record_session_fullscreen_exact(work_dir, id) -> Result<bool>` sets it once under the lock (true when it changed).
2. `tmux/capture.rs`: `alternate_on_argv(target)` = `display-message -p -t <exact target> '#{alternate_on}'`; `pane_on_alternate(socket, session) -> Option<bool>`; `capture_visible_argv` = `capture-pane -p -e -J -t <target>` (no `-S`/`-E`).
3. `tmux/fullscreen.rs` (new): `sample_fullscreen(work_dir, sessions)` called from `poll()` next to :150 for each Running, PID-alive tmux session: when `pane_on_alternate` is `Some(true)`, record the flag if unset and write the visible capture as the session's history file (same path and bounds as `write_history`, atomic). One `display-message` per session per poll; a capture only while on the alternate screen.
4. `tmux/history.rs`: `pub enum CaptureMode { Full, Visible }` and `pub fn capture_mode(fullscreen_seen: bool, alternate_now: Option<bool>) -> Option<CaptureMode>` exactly as the stage PUBLIC SURFACE states. `save_history` reads `fullscreen_seen` from disk and `pane_on_alternate`, then captures Full, Visible, or keeps the file already saved (None).
5. `status/data`: `pub enum HistoryKind { History, LastScreen }` (`#[serde(rename_all = "snake_case")]`), `StageSummary.terminal_history_kind: Option<HistoryKind>` set with the session (LastScreen when that session has `fullscreen_seen`). Attention model carries the kind; `text_note` says `last screen of its last session: less -R ...` for LastScreen. Web model attention entry gains `terminal_history_kind`.
6. Fixtures: add the field where `terminal_history_session` appears in `snapshot.json` (one stage gets `last_screen`) and in `statuses.json` if it carries attention entries; keep `fixture_matches_serde_output` green.

## Tests

- tmux test in `tests_history.rs` behind `skip_without_tmux`: a pane script that prints `\033[?1049h` and sleeps; assert `pane_on_alternate` is `Some(true)`, `sample_fullscreen` sets the flag and saves a visible capture, `kill_session` keeps it. This skips inside confined runs; it must pass in a plain shell with tmux.
- `stopped.rs` tests: kind History for an ordinary session, LastScreen for a fullscreen one.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib status::data::`, `--lib orchestrator::terminal::tmux::`, `--lib commands::status::web::` pass, including the two frozen contracts; clippy clean.

## Traps

- Native sessions keep no history (`stopped.rs:58` filters tmux); leave them alone.
- A hung tmux server blocks the poll thread: always go through `capture_stdout`'s bounded runner.
- `fullscreen-web` owns `web/src/api/schema.ts`; it expects `terminal_history_kind` as `"history" | "last_screen"` on the stage summary and the attention entry. Do not rename.
