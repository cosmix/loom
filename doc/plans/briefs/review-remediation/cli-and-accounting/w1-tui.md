# cli-and-accounting / w1-tui: status TUI reconnect, signal exit, subscribe token, config edit keys

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md`, sections D4, CR-19, CR-57 (sub-claim a only), CR-58 (sub-claim a only).

## Findings covered
- D4: when the daemon closes a subscriber socket while it is still alive (it shuts the write side of a slow subscriber), the TUI reads EOF and ends the session as "Daemon exited". It must reconnect instead.
- CR-19 (narrowed): the Ctrl+C handler cleans the terminal from the signal thread and calls `process::exit(0)`. It must only set a flag; the main loop leaves cleanly and the process exits 130. The config TUI gets the same shape. No crash marker for the config TUI.
- CR-57(a): `daemon_client::subscribe` reads the auth token from the current directory instead of the work directory it is connected to.
- CR-58(a): the config TUI edit key handler inserts control characters and treats `Ctrl+<letter>` as the plain letter.

## Files
Own (write):
- `loom/src/commands/status/ui/tui/app.rs`
- `loom/src/commands/status/ui/tui/app_view.rs`
- `loom/src/commands/status/ui/tui/app_tests.rs`
- `loom/src/commands/status/ui/tui/daemon_client.rs`
- `loom/src/commands/status/web/broadcast.rs`
- `loom/src/commands/config/tui/mod.rs`
- `loom/src/commands/config/tui/tests/cycling.rs`

Read only: `loom/src/commands/status/ui/tui/app_input.rs`, `loom/src/daemon/server/tokens.rs` (`read_auth_token`), `loom/src/daemon/socket.rs` (`socket_path`), `loom/src/utils.rs` (`cleanup_terminal_crossterm`).
Frozen (never edit): `loom/src/commands/status/ui/tui/subscriber_contract_tests.rs` (new, written before your work).

## Changes

### 1. D4: reconnect on EOF (`app.rs`, `TuiApp::receive_response`)
Today the `Err(error) if is_socket_disconnected(&error)` arm sets `self.socket_closed = true`, tries the saved report, returns `Ok(false)` when a report is open, `Ok(true)` when a completion summary exists, and otherwise sets `last_error = "Daemon exited"`, renders, sleeps 500 ms and returns `Ok(true)` (which ends the session through `advance`).
Rewrite the arm:
1. Do not set `socket_closed` at the top. Keep the order: if `plan_report` is none and not `completed_with_failures()`, `load_report(work_path)`.
2. If `plan_report` is some: set `socket_closed = true`, return `Ok(false)` (unchanged outcome).
3. If `completion_summary` is some: set `socket_closed = true`, return `Ok(true)` (unchanged outcome).
4. Otherwise: `let recovered = self.reconnect_after_read_error(&error, stream, work_path)?; *reader = ResponseFrameReader::new(Duration::from_secs(30)); return Ok(recovered);`, exactly as the generic `Err(error)` arm below it does. Delete the inline "Daemon exited" block and its 500 ms sleep; `reconnect_after_read_error` already sets "Daemon exited", renders and sleeps when all `RECONNECT_ATTEMPTS` fail, and returns `Ok(true)` then.
Add one guard in `reconnect_after_read_error`: at the top of each attempt, `if !self.running.load(Ordering::SeqCst) { return Ok(true); }`, so a Ctrl+C during the reconnect loop does not wait for three connect timeouts.
The existing tests `tui_receives_fragmented_completion_before_daemon_eof`, `daemon_eof_without_a_final_message_opens_the_saved_report` and `a_failed_plan_keeps_the_legacy_summary_despite_a_saved_report` take the report or completion branch before the reconnect and must stay green without edits.

### 2. CR-57(a): subscribe takes the work directory (`daemon_client.rs`, callers)
- Add `pub(crate) fn user_token(work_dir: &Path) -> String` in `daemon_client.rs`: `read_auth_token(crate::daemon::socket_path(work_dir).parent().unwrap_or(work_dir)).unwrap_or_default()`. This is the expression `connect` uses today on one line (`let token = read_auth_token(socket_path.parent()...`); make `connect` call `user_token(work_dir)` so there is one spelling.
- Change `subscribe` to `pub fn subscribe(stream: &mut UnixStream, work_dir: &Path) -> Result<()>` and build the request with `user_token(work_dir)`. Delete the `crate::commands::common::work_dir_path()` lookup.
- Callers (the only two, found with `rg 'subscribe\(&mut'`): `TuiApp::connect_and_subscribe` in `app.rs` (pass `work_path`) and `daemon_session` in `commands/status/web/broadcast.rs` (pass `work_path`).
- `TuiApp::unsubscribe` in `app.rs` reads the token with `read_auth_token(work_path)` on the unresolved path; switch it to `user_token(work_path)` and drop the `read_auth_token` import from `app.rs` if nothing else in the file uses it.

### 3. CR-19: cooperative exit (`app.rs`, `app_view.rs`, `config/tui/mod.rs`)
- `TuiApp::install_exit_handler` (`app.rs`): the closure only does `running.store(false, Ordering::SeqCst)`. Remove `cleanup_terminal_crossterm()` and `process::exit(0)` from it. Add a doc line: the handler also serves SIGTERM and SIGHUP (the `ctrlc` `termination` feature).
- Add `fn interrupted(&self) -> bool { !self.running.load(Ordering::SeqCst) }` on `TuiApp`. `running` is written only by that handler, so false means a signal arrived.
- `run` (`app.rs`): after `self.unsubscribe(..)` and `self.cleanup_terminal()` and the completion-summary print, `if self.interrupted() { std::process::exit(130); }` before `result` is returned.
- `run_report` (`app_view.rs`): after `self.cleanup_terminal()`, the same `if self.interrupted() { std::process::exit(130); }`.
- Config TUI (`config/tui/mod.rs`): add `running: Arc<AtomicBool>` to `ConfigTui` (initialised `true` in `new`). In `run`, clone it into the handler, which only stores `false`; replace `ctrlc::set_handler(|| { cleanup...; exit(0) })` accordingly and rewrite the long comment above it to say what is true now (the handler sets the flag, `event_loop` sees it within one 100 ms poll, `cleanup_terminal` runs, then the process exits 130). `event_loop` becomes `while self.running.load(Ordering::SeqCst) { ... }` returning `Ok(())` after the loop. In `run`, after `app.cleanup_terminal()`, `if !app.running.load(..) { std::process::exit(130); }` before returning `result`. Do not add a crash marker.
- Imports: `std::sync::{atomic::{AtomicBool, Ordering}, Arc}` in the config module.

### 4. CR-58(a): config edit keys (`config/tui/mod.rs`)
- `dispatch_edit_key(state, key: KeyEvent) -> bool`. Call site in `dispatch_key` passes `key` (the Ctrl+C check above it stays).
- `KeyCode::Char(character)` arm: insert only when `!character.is_control() && !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)`; otherwise do nothing. `SHIFT` stays allowed (capital letters). The other arms (`Backspace`, `Enter`, `Esc`) are unchanged.

## Interfaces shared with other workers
None. `daemon_client::subscribe(stream, work_dir)` has exactly the two callers above and both are yours.

## Tests
- `app_tests.rs`, new test `eof_with_a_live_daemon_reconnects` (top level of the file, near `tui_receives_fragmented_completion_before_daemon_eof`). Build a temp work dir (`tempfile::TempDir`), bind a `std::os::unix::net::UnixListener` at `crate::daemon::socket_path(dir)`, and run a server thread that accepts twice. Each connection: `read_message::<Request>` (expects `Ping`), write `Response::Pong`, `read_message::<Request>` (expects `SubscribeStatus`), write `Response::Ok`. Connection one then drops the stream. Connection two writes one `Response::StatusUpdate { data: Box::new(StatusData { plan_name: Some("probe".into()), ..StatusData::default() }) }` and then holds the socket open until the test ends (use a channel to release it). In the test: `let mut stream = TuiApp::connect_and_subscribe(dir)?`, `TuiApp::headless()`, `ResponseFrameReader::new(1 s)`, then `advance` twice. Assert both calls return `true`, `app.last_error.as_deref() != Some("Daemon exited")`, and after the second call `app.status.data.plan_name.as_deref() == Some("probe")` and `app.last_error.is_none()`. Fails today: the first `advance` returns `false`. Use `redirect_user_config` as the `report` module tests do if `read_current` needs it. `StatusData` is `crate::commands::status::data::StatusData`.
- `app_tests.rs`, new test `a_pending_interrupt_stops_reconnecting`: `app.running.store(false, ..)`, a listener-less temp dir and an EOF stream (`UnixStream::pair()` with the sender dropped, read timeout 5 ms); `advance` returns `false` (the session ends) in under 300 ms, with no reconnect attempt and no 80 ms backoff sleeps. Today the EOF arm sleeps 500 ms before ending, so the elapsed assertion fails; the new code skips every attempt and every sleep.
- `daemon_client.rs` test module, new test `subscribe_sends_the_token_of_the_given_work_dir`: temp dir with `user.token` containing `tok-abc\n`, `UnixStream::pair()`, a thread that reads a `Request` from one end and answers `Response::Ok`; call `subscribe(&mut other_end, dir)`; assert the request is `Request::SubscribeStatus { auth_token }` with `auth_token == "tok-abc"`. The current directory of the test process is not the temp dir, so it fails today (empty token).
- `config/tui/tests/cycling.rs`, new test `control_characters_do_not_reach_the_edit_buffer`: focus `context.ceiling_tokens`, press Enter (opens the editor), record `state.edit_buffer()`, dispatch `KeyEvent::new(KeyCode::Char('\u{85}'), KeyModifiers::NONE)` and `KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)` through `dispatch_key`, assert the buffer is unchanged; then dispatch `Char('Z')` with `KeyModifiers::SHIFT` and assert it was appended. `press` in that file takes only a `KeyCode`, so build the `KeyEvent`s directly. Fails today for the first two.
- Contract to turn green: `commands::status::ui::tui::subscriber_contract_tests::eof_with_a_live_daemon_reconnects`.
- The config TUI signal handling has no unit test: the handler is process-global (`ctrlc::set_handler` once per process) and `ConfigTui::new` needs a real terminal.

## Proof
From `loom/`: `cargo check --all-targets`; `cargo test --lib commands::status::ui::tui::`; `cargo test --lib commands::config::tui::`; `cargo test --lib commands::status::web::broadcast`.

## Traps
- `receive_response` returns `true` for "end the session" (inverse of `advance`). A successful reconnect returns `false` from `reconnect_after_read_error`.
- Do not set `socket_closed` before deciding to reconnect: `advance` returns early forever when it is set, so a resubscribed stream would never be read.
- The `ctrlc` handler is installed once per process; leave `run_report` calling `install_exit_handler` as it does.
- `cargo fmt` may push `app.rs` past 360 lines; it is 327 now and this change adds about 25. If it exceeds 380, move `interrupted` and the reconnect guard into `app_view.rs`.
- The `connect()` token path is unchanged in behaviour; only its spelling moves into `user_token`.
