# Validation: web server and terminal bridge, TUI, CLI commands, completions, token accounting, quota

Revision checked: `62be2a3d2`. Paths are repo-relative. `loom/src/` is abbreviated `src/`.
Live check: the installed `loom complete` binary was used read-only for the completion findings.

---

## D1 — Terminal output stall never times out

**Verdict:** CONFIRMED
**Evidence:**
- `src/commands/status/web/terminal/bridge.rs:194-229` (`pump_pty`): `socket.flush()` errors go through `outbound_stop` (`:289`), which maps `WouldBlock`/`TimedOut`/`Interrupted` to `None`. The pass then ends before the PTY read, so the loop spins with one `WRITE_TIMEOUT` (250 ms) park per iteration and no clock.
- `bridge.rs:94` only `gate_stalled(...)`, which watches `pending.len()` (input queue to the PTY). Nothing times the output direction.
- `MAX_WRITE_BUFFER` (2 MiB, `:46`) is never reached because a stalled flush stops the pass before more PTY bytes are read. The buffer holds at most about one 16 KiB frame.
- No guard found: no TCP keepalive anywhere (`rg SO_KEEPALIVE` is empty), no ping. `running` and a reset/EPIPE error are the only other exits.
- Residual: a peer that is silent but still ACKing nothing eventually errors on the kernel retransmit timeout (about 15 min on Linux), but only while unacked data is queued.
**Fix seam:**
- Change `run_with` (`bridge.rs:75`) and `pump_pty` (`:194`). Add an `outbound_stalled_since: Option<Instant>` beside `gated_since`.
- Set it with `get_or_insert_with(Instant::now)` when `pump_pty`'s flush or send returned the `stalled` branch. Clear it when `flush()` returns `Ok`.
- Break with `Stop::Stalled` when `elapsed() >= stall_timeout`. Reuse the existing `stall_timeout` parameter and `GATE_STALL_TIMEOUT` (one deadline, so `start_bridge_with` needs no new argument).
- Generalise `gate_stalled(since, pending, timeout)` (`:132`) into a small `deadline_elapsed(since: &mut Option<Instant>, active: bool, timeout) -> bool` that both clocks call.
- `pump_pty` must report "stalled this pass" to the caller. Change its return to a small enum or `(Option<Stop>, bool)`.
- Progress semantics: "reset on progress" equals "a flush returned Ok". While the flush is stalled no new PTY bytes are read, so the pending buffer is at most one frame and any reader taking about 16 KiB per 30 s survives. Temporary slow readers keep today's behaviour.
- `finish()` (`:331`) already maps `Stop::Stalled` to close code 4008 "input stalled". Add a distinct reason string "output stalled" only if the page needs it. Otherwise leave it and rename nothing.
- Consumers: only `bridge::run` (`src/commands/status/web/terminal/upgrade.rs`, `start_bridge`) and the tests.
**Regression test:** extend `src/commands/status/web/terminal/tests_bridge/backpressure.rs`. It has `start_bridge_with(script, mode, socket_bytes, stall_timeout)` (`tests_bridge.rs:48`), `shrink_buffers`, `join_within`, `wait_for_close`, and the recovering-reader test `bridge_survives_a_client_that_stops_reading`.
- New test A: script `yes`, `socket_bytes=Some(8 * 1024)`, `stall_timeout=1 s`, client never reads. Assert `join_within(server, 5 s)`. Fails today: the thread never ends until the test harness times out.
- New test B: client sleeps 1.5x the timeout, then reads, and still gets `END`. Needs no change to the existing recovering test.
**Files a fix touches:** `src/commands/status/web/terminal/bridge.rs`, `src/commands/status/web/terminal/tests_bridge/backpressure.rs`.
**Size / tier:** M (about 50-70 lines with tests), judgment (concurrency, resource holding).
**Overlap:** CR-60 (same file and functions; do both in one change).

### D1 versus CR-60 reconciliation
They are two failure modes of one symptom, a slot held by a peer that never goes away.
- D1: the peer is alive at TCP level but not draining, with output flowing. The output buffer stalls.
- CR-60: the peer vanishes without FIN or RST (NAT drop, laptop lid) while the PTY is idle. Nothing is ever written, so nothing stalls.
- The outbound-stall clock does not cover the idle case.
- A ping with an inbound-silence deadline covers both, provided the deadline is paused while the input gate is shut (pongs are not read then). It is less exact than D1's clock, and in the stalled case the ping sits behind the pending frame.
- Decision: implement both in `bridge.rs`. D1's clock is mandatory. CR-60's ping/pong (below) is the small add-on. A brief that wants only one should pick ping/pong plus the gate pause, and note that D1's test A then needs a pong deadline instead of a flush clock.

---

## D4 — Subscriber EOF reported as "Daemon exited"

**Verdict:** CONFIRMED
**Evidence:**
- `src/daemon/server/client_write.rs:17-30`: `write_bounded_frame_with_timeout` calls `stream.shutdown(Write)` on a failed or slow write. A resumed TUI reads its backlog, then EOF.
- `src/commands/status/ui/tui/app.rs:240-282` (`receive_response`): `is_socket_disconnected` sets `socket_closed`, tries the saved report, then returns `Ok(true)` if a completion summary exists. Otherwise it sets `last_error = "Daemon exited"`, sleeps 500 ms and returns `Ok(true)`, which ends the session (`advance` at `:201`).
- Only non-EOF read errors go to `reconnect_after_read_error` (`:284`, 3 attempts, 80 ms backoff).
- No guard found. The existing test `daemon_eof_without_a_final_message_opens_the_saved_report` (`app_tests.rs:195`) pins the report path and does not cover a live daemon.
**Fix seam:**
- Change `receive_response` (`app.rs:240`). In the `is_socket_disconnected` arm, keep the report and completion branches. Replace the final "Daemon exited" tail with a call to `reconnect_after_read_error(&error, stream, work_path)` and reset the reader as the generic arm does (`*reader = ResponseFrameReader::new(...)`).
- Do not set `socket_closed` before the reconnect decision. Set it only when the reconnect gives up (it returns `Ok(true)` after setting "Daemon exited"). Otherwise `advance` (`:211`) returns early forever after a successful resubscribe.
- Reuse `connect_and_subscribe` (`:134`) and `RECONNECT_ATTEMPTS`/`RECONNECT_BACKOFF` (`:52`, `:55`). The 500 ms sleep after a final failure stays in `reconnect_after_read_error`.
- Consumers: `TuiApp::advance`, `run_event_loop`, `app_tests.rs`. The web broadcaster has its own loop (`src/commands/status/web/broadcast.rs`) and is not affected.
**Regression test:** `src/commands/status/ui/tui/app_tests.rs`. It has `socket_with(&[...])` (a `UnixStream` pair seeded with frames), `TuiApp::headless()`, `finished_run`, `ResponseFrameReader`.
- New test: a real `UnixListener` in a temp work dir accepting twice. The first connection closes immediately. The second answers the `Ping` and `SubscribeStatus` handshake and then sends a status frame. Assert that after `advance`, `app.last_error != Some("Daemon exited")` and the status was applied. Fails today: the session ends with "Daemon exited".
- `connect()` needs a daemon-shaped listener (Ping then Pong, then `Ok`). Write a small helper in the test.
**Files a fix touches:** `src/commands/status/ui/tui/app.rs`, `src/commands/status/ui/tui/app_tests.rs`.
**Size / tier:** S-M (about 30 lines), needs-exploration (the listener test helper).
**Overlap:** CR-19 (same file `app.rs`, different functions), CR-57 (`daemon_client.rs`, `connect_and_subscribe`).

---

## D6 — In-place rewrite between scans is not detected

**Verdict:** CONFIRMED (logic). Whether any real writer rewrites in place is not statically verifiable.
**Evidence:**
- `src/tokens/tracker.rs:239`: restart only on `path` change, `inode` change, or `stat.len < file.offset`. `:242` returns `Ok(false)` when `stat.len == offset`.
- A same-inode rewrite of equal length reads nothing. A longer rewrite reads from the old offset, in the middle of the new content or of a JSON line. Totals keep the old prefix.
- `src/tokens/tracker_tests.rs:33` `truncated_or_replaced_transcript_counts_its_new_content` scans after the shrink, so the `len < offset` branch is hit. The new-inode branch is tested via rename. Neither covers regrowth between polls.
- No guard found (`FileCursor`, `src/tokens/accumulate.rs:56-66`, holds no content fingerprint).
**Fix seam:**
- Add `FileCursor.anchor: u64` (hash of up to the last 256 bytes ending at `offset`, 0 when `offset == 0`). Add it to the `Serialize`/`Deserialize` derive, with `#[serde(default)]` or a `CHECKPOINT_VERSION` bump (`src/tokens/checkpoint.rs`; no migration needed per project policy, so a bump is enough).
- In `read_source` (`tracker.rs:221`), after the existing checks and before the `len == offset` early return, read the same window with a new `tail::read_fragment`-style helper (`src/tokens/tail.rs`, which already has `read_fragment(path, offset, len)`; add `read_window(path, end, n)`). On mismatch call `agent.restart(...)`.
- Update `anchor` after each successful read (`agent.file.offset = offset` at `:251`). Use the same hasher as `pass_memo::new_hasher`, or `crate::utils::sha256_hex` truncated.
- A restart rebuilds that file's contribution (`AgentState::restart`, `accumulate.rs:91`), so totals are recomputed rather than adjusted.
- Cost is one pread of at most 256 bytes per advanced or unchanged file per pass. The memo (`pass_memo.rs:40` `FileStamp`) already skips unchanged files before this point, so hot-path cost is unchanged.
**Regression test:** `src/tokens/tracker_tests.rs`, with `Fixture::add_session`, `fx.scan`, `body`, `line`, `append`, `alpha_total`.
- Test 1: write two lines, scan. Overwrite via `fs::write` (same inode) with two different lines of the same byte length and different usage. Scan. Assert the total equals the new lines only. Fails today (old total stays).
- Test 2: overwrite with a longer different body between scans. Assert the total equals a full fresh scan.
**Files a fix touches:** `src/tokens/tracker.rs`, `src/tokens/accumulate.rs`, `src/tokens/tail.rs`, `src/tokens/checkpoint.rs` (version), `src/tokens/tracker_tests.rs`.
**Size / tier:** M, needs-exploration.
**Overlap:** D7, D8 (all in the `src/tokens/` checkpoint and accumulate path).

---

## D7 — Checkpoint shedding drops dedup tables, so a revised message is double counted

**Verdict:** CONFIRMED
**Evidence:**
- `src/tokens/checkpoint.rs:114` `clear_tables_except` resets `agent.file.messages` for agents not in `advanced`. Its own doc says "A later line of a cleared file that repeats an id counts it again."
- `src/tokens/pass_memo.rs:231` `save_shedding` applies this to the in-memory checkpoint, not only the written copy.
- `src/tokens/accumulate.rs:157-166` `revise_claude`: `messages.replace(id, ..)` returns the previous observation only when the table still holds the id. With the table cleared the old usage is not retracted and the new vector is added in full.
- The existing cap test `tracker_tests.rs:289` asserts only `saved_table_len == 0` and that the totals are right at that moment.
- The Codex path is unaffected (`codex_total` is kept). The table is already bounded by `MESSAGE_TABLE_CAP` (`src/tokens/messages.rs`), so very old ids are lost anyway; the shed case widens this to recent ids.
**Fix seam:**
- Option A (recommended): add `FileCursor.table_shed: bool` (`accumulate.rs:56`). `clear_tables_except` sets it true instead of silently clearing, still clearing the table to keep the checkpoint under the cap. In `read_source` (`tracker.rs:221`), when `agent.file.table_shed` and the file has grown, call `agent.restart(&source.path, stat.inode)` so the agent is rebuilt from offset 0. Exact totals, one extra scan of that file only when it advances.
- Option B: keep a compact id to usage-vector map per idle agent. This works against the checkpoint cap, so it is rejected.
- Option A is settled. Totals are retained until the file advances, and a deleted file keeps its totals (the missing-file path returns early).
- Consumers: `checkpoint.rs`, `pass_memo.rs` (the call site is unchanged), `tracker.rs`, `accumulate.rs`.
**Regression test:** `src/tokens/tracker_tests.rs`, extending the cap test at `:289`. It has `bulky_body(tag, now)` (`tests_fixture.rs:95`, ids `"{tag}{n:0>1200}"`), `pass_at`, `saved_table_len`, `claude_line`, `append`.
- After the existing shedding assertions, append to the main transcript one line with an already-counted id (`format!("main{0:0>1200}", 0)`) but a newer timestamp and a different cache_write. Run `pass_at`. Compare `stage(&flow, "alpha").total` to a fresh `Fixture` scan of the same final bytes. Fails today with a double count.
**Files a fix touches:** `src/tokens/accumulate.rs`, `src/tokens/checkpoint.rs`, `src/tokens/tracker.rs`, `src/tokens/tracker_tests.rs`.
**Size / tier:** M, judgment (accounting correctness).
**Overlap:** D6 (both add state to `FileCursor` and branch in `read_source`; do them in one worker).

---

## D8 — Incomplete final token capture becomes the permanent plan record

**Verdict:** CONFIRMED
**Evidence:**
- `src/daemon/server/orchestrator.rs:206-215` `publish_plan_stats`: a `final_scan` error is only logged, then `finish_plan` runs regardless.
- `src/tokens/tracker.rs:81-86` `final_scan` returns `Err` only before registration (for example `ensure_tokens_dir`, `discover`, `:157-165`). A `read_sources` failure (`:168`) becomes a warning in `pass.warning`, which is logged, and the partial totals are still registered (`:171`).
- `src/tokens/plan_totals.rs:110-117` `plan_tokens`: with no registration it falls to `load_offline`, which returns the older checkpoint or an empty total for a missing tokens directory (`:129-138`).
- `src/plan_stats/finish.rs:23-45`: the first archive wins and a retry reuses it verbatim.
- The only caller of `finish_plan` is `orchestrator.rs:211`, so there is no later repair trigger.
- Second defect found: the registration map (`plan_totals.rs:58`, `register` at `:90`) is process-global and keyed by work root. A final scan that fails before `register` in a later run of the same long-lived daemon would read the previous run's registration. This is not verified: it depends on whether one daemon process can finish two plans.
- Refined claims hold: a checkpoint write failure after registration still gives accurate totals (`register` runs before the writes), and an unusable checkpoint errors in `build`, so nothing is published.
**Fix seam:**
- Change `final_scan` (`tracker.rs:81`) to return `Result<FinalScan>` where `FinalScan { complete: bool }` is false when `pass.warning` carried a read failure (use the `read_failure` value in `run_pass`, `:168-169`, rather than parsing the warning string). Thread it through `Pass`.
- Change `publish_plan_stats` (`orchestrator.rs:206`) to call `finish_plan` only when the final scan returned `Ok` and complete. On `Err` log an error naming that the archive was skipped.
- On incomplete reads, decision: do publish. A file that cannot be read now will not be readable later either, so withholding blocks the archive forever. Log the skipped file as an error. The pre-registration `Err` is the case that must not publish.
- Clear the stale registration at the start of `run_pass` for `LastLine::Count` (`plan_totals::clear(work_root)` before reading), so a failure falls back to the checkpoint and not to a previous run's totals.
- A real "idempotent repair" command is out of scope. Deferral means the archive is absent and the log says why.
- Consumers: `orchestrator.rs`, `tokens/mod.rs` (re-export), `plan_totals.rs`.
**Regression test:** `src/daemon/server/orchestrator_tests.rs`. Model: `published_plan_statistics_count_a_line_written_after_the_last_checkpoint` (`:256`), which uses `redirect_home`, `merged_run`, `transcript_line`, `finish_orchestration`.
- New test: a stale `tokens/checkpoint.json` from an earlier scan, then make the final scan fail before registration (replace `tokens` with a symlink so `ensure_dir` bails, `tracker.rs:297`). Assert `finish_orchestration` still returns `Success`, and that `plan-stats.json` and the archive are absent. Fails today: the stale totals are archived.
- Second test for the registration boundary: a partial read failure still publishes.
**Files a fix touches:** `src/daemon/server/orchestrator.rs`, `src/tokens/tracker.rs`, `src/tokens/plan_totals.rs`, `src/tokens/mod.rs`, `src/daemon/server/orchestrator_tests.rs`.
**Size / tier:** M, judgment (data loss: permanent record).
**Overlap:** D6, D7 (`tracker.rs`).

---

## E5 — Account rotation leaves the previous account's quota in the project cache

**Verdict:** CONFIRMED
**Evidence:**
- `src/quota/poller.rs:144-147`: a missing home or `credentials::account` error returns silently. The project file `quota/claude.json` is untouched.
- `poller.rs:187-211` `project_account`: when `account.quota` is `None` and `display_error` is `None`, nothing is written. `src/quota/shared.rs:110-116` gives `None` for an unconfirmed 429 streak (`consecutive_rate_limits` below `RATE_LIMIT_CONFIRMATIONS`). With a quota-less error, `cache::record_failure` (`src/quota/cache.rs:70-76`) re-writes account A's windows with an error set.
- `src/orchestrator/signals/format/budget.rs:15-22` `fresh()` drops errored or stale readings (`STALE_AFTER_SECS = 600`, `src/quota/model.rs:59`), so exposure is advisory routing for at most 600 s after the last A reading. Account A's reading can also keep appearing in the TUI quota panel.
- No guard keys the file by account. Cache is `claude.json` per provider.
**Fix seam:**
- Add `cache::clear_provider(work_root, provider) -> Result<()>` in `src/quota/cache.rs` (reject a symlink with `reject_symlink` from `crate::fs::cache_io`, then `remove_file`, ignoring `NotFound`).
- In `project_account` (`poller.rs:188`), for `account.quota == None` call `clear_provider(work_root, "claude")` instead of `record_failure`. An account with no quota has never read successfully, so the project file cannot hold anything of its own.
- In `poll_claude` (`:140-154`), on the two silent early returns (no home, no credentials) call `clear_provider`, then set `next_due`. Clearing an absent file is a no-op, so the codex-only case stays silent.
- Leave `poll_claude`'s `Err(e)` branch (`:176`, lock or IO failure with no account identity) as it is.
- Codex has the same shape (`poll_codex` returns silently without a binary) but no account rotation; leave it.
- Consumers: `src/orchestrator/signals/format/budget.rs`, the status quota view (`src/commands/status/ui/tui/ledger/quota.rs`), `src/quota/history.rs` (untouched).
**Regression test:** `src/quota/poller_tests.rs`. Model: `an_unconfirmed_429_keeps_the_last_reading_without_an_error` (`:89`), which uses `account_after(limits)`, `project_account`, `ProviderState::new`, `tempdir`.
- New test: `project_account(dir, &mut state, &account_after(0), 1_000)` (account A with a reading), then `project_account` with `AccountState { consecutive_rate_limits: 1, ..Default::default() }` (account B, no quota). Assert `cache::read_provider(dir, "claude").is_none()`. Fails today (A's reading remains).
**Files a fix touches:** `src/quota/cache.rs`, `src/quota/poller.rs`, `src/quota/poller_tests.rs`, `src/quota/cache_tests.rs`.
**Size / tier:** S-M (about 40 lines), mechanical.
**Overlap:** CR-55 (same function `project_account` and file).

---

## CR-19 — TUI `Ctrl+C` handler

**Verdict:** NARROWED (three sub-claims confirmed, one narrowed)
**Evidence:**
- `src/commands/status/ui/tui/app.rs:143-151`: the handler sets `running=false`, runs `cleanup_terminal_crossterm()` from the signal thread, then `process::exit(0)`.
- (a) Races terminal output: CONFIRMED. The main thread may be mid-draw (`render`). The handler's own comment in `config/tui/mod.rs:41-47` admits a mid-render landing.
- (b) Skips `unsubscribe`/`Drop`: CONFIRMED. `run()` (`:118-132`) never reaches `unsubscribe` or `cleanup_terminal`. Limited harm: the daemon drops the socket on exit, and `cleanup_terminal_crossterm` removes the marker.
- (c) Exits 0 on a signal: CONFIRMED. `ctrlc` has the `termination` feature (`loom/Cargo.toml:26`), so the handler also serves SIGTERM and SIGHUP.
- (d) "`ConfigTui` lacks a crash marker": confirmed that `write_tui_marker` is called only from `app.rs:120` and `app_view.rs:89`. It needs a work path (`tui.pid` is in the state root) and `loom config` has none, so the claim is a design gap, not a simple omission.
- Note: the event loop already checks `running` (`app.rs:180`, `app_view.rs:93`), so a cooperative exit exists.
**Fix seam:**
- `install_exit_handler` (`app.rs:143`): the handler stores `running=false` and `interrupted=true`, then waits up to 2 s for a `cleaned` `AtomicBool` that `run()` sets after `cleanup_terminal`. If the main thread does not finish, run `cleanup_terminal_crossterm()` and `process::exit(130)`.
- `run()` (`:118`): after `cleanup_terminal`, if `interrupted`, `process::exit(130)`. Exit code decision: 130 (SIGINT convention; `ctrlc` does not say which signal).
- `ConfigTui` (`src/commands/config/tui/mod.rs:48-52`): same cooperative pattern (loop `event_loop` at `:105` checks a `running` flag between 100 ms polls). Skip the crash marker: `loom config` has no state root, and the panic hook plus the signal handler already cover it.
- Reuse `crate::utils::cleanup_terminal_crossterm` (`src/utils.rs:200`).
- Consumers: `run_tui`, `run_finished_report` (`app_view.rs` has no handler; `run_report` is left alone), the config TUI.
**Regression test:** the handler is process-global (`ctrlc::set_handler` once per process), so a unit test is poor. Test the pure part: factor the wait-or-force logic into `fn wait_for_cleanup(done: &AtomicBool, limit: Duration) -> bool` and test it in `app_tests.rs` (returns true when `done` is set in time, false after the limit). Also assert via `run_event_loop` with `running=false` pre-set that the loop returns promptly. Cooperative exit itself is covered by `advance` tests already.
**Files a fix touches:** `src/commands/status/ui/tui/app.rs`, `src/commands/status/ui/tui/app_tests.rs`, `src/commands/config/tui/mod.rs`.
**Size / tier:** M, judgment (signal and terminal handling).
**Overlap:** D4 (`app.rs`), CR-58 (`config/tui/mod.rs`).

---

## CR-55 — Quota poller backoff clobber and unchecked time arithmetic

**Verdict:** CONFIRMED (split into two verdict lines)
- **(a) Backoff clobber: CONFIRMED.** `src/quota/poller.rs:195-210`: when `!written`, `record_error(..., next_interval(state.interval, true))` sets interval and `next_due`, then `:209-211` overwrites both from `account.interval_secs` and `account.next_poll_at`. This contradicts the doc comment on `finish_poll` (`:~230`, "a bad disk state still backs off rather than spinning"). Impact is bounded: `shared::poll_shared` skips the fetch before `next_poll_at`, so the loop re-projects cheaply, but it retries the failing write at the account cadence and never backs off.
- **(b) Unchecked `i64` arithmetic: NARROWED.** `src/quota/model.rs:94-96` `age_secs` is `(now - observed_at).max(0)`. With a deserialized `observed_at = i64::MIN`, the subtraction overflows (debug: panic, release: wraps negative, then `.max(0)` gives age 0, so the reading looks fresh). `reset_text` (`:~83-90`) cannot overflow for realistic `now` because `resets_at > now` is checked first. The input is a user-writable local cache file, so impact is low.
- **(c) Future `observed_at`: CONFIRMED.** `age_secs` clamps a future `observed_at` to 0, so a skewed or planted reading counts as fresh in `budget.rs:fresh()` until time catches up.
**Fix seam:**
- (a) `project_account` (`poller.rs:187-211`): wrap the final schedule lines in the `written` branch, applying the account schedule only when `written`.
- (b)/(c) `age_secs` (`model.rs:94`): `now.saturating_sub(observed_at)`. For a future `observed_at`, return `i64::MAX` when `observed_at > now + SKEW_SECS` (new const, 300 s). Otherwise clamp to 0 for small skew. Callers: `src/orchestrator/signals/format/budget.rs:19`, `src/commands/status/ui/tui/ledger/quota.rs:227`, `src/status/data/sanitize.rs` if it uses it (check with `rg age_secs`).
- Changing the future-skew behaviour alters renderers ("old" suffix shows for a future reading); accept that.
**Regression test:** `src/quota/poller_tests.rs`: symlink the provider file as in `finish_poll_does_not_append_history_when_the_snapshot_write_fails` (`:66`), call `project_account` with an account with a reading, then assert `state.interval == next_interval(initial, true)` (doubled), not `account.interval_secs`. Fails today. `src/quota/model_tests.rs`: `age_secs(i64::MIN, now)` must not panic and must be at least `STALE_AFTER_SECS`; `age_secs(now + 10_000, now)` must be stale.
**Files a fix touches:** `src/quota/poller.rs`, `src/quota/model.rs`, `src/quota/poller_tests.rs`, `src/quota/model_tests.rs`.
**Size / tier:** S, mechanical (a), judgment-lite (b/c skew threshold).
**Overlap:** E5 (same file `poller.rs`, same function `project_account`); do E5 and CR-55(a) in one worker.

---

## CR-56 — `loom completions`

- **(a) `"complete"`/`"completions"` missing from `MACHINE_PROTOCOL_COMMANDS`: REFUTED.** `src/main.rs:20` and `:62-67`: terminal recovery runs for them, but `recover_terminal_if_needed` (`src/utils.rs:112-131`) calls `restore_stale_terminal`, which refuses unless stdout is a terminal (`src/utils/terminal_recovery.rs:113-116`), and `cleanup_terminal_crossterm` writes sequences only `if stdout.is_terminal()` (`utils.rs:212`). Under `$(loom complete ...)` or `eval "$(loom completions bash)"` stdout is a pipe, so nothing reaches the payload. Residual hygiene only: `complete` runs the marker stat on every Tab. Adding both names to the list is harmless and optional.
- **(b) `route_completion` ignores depth-3 commands: CONFIRMED.** `src/completions/dynamic/mod.rs:186-192` (`route_completion`) special-cases only `[_cmd, "output", sub]`. Live check: `loom complete bash . "loom stage contracts " "" "contracts"`, `... "loom plan summary " ...` and `... "loom stage review " ...` all print nothing.
- **(c) Seven subcommands unrouted: CONFIRMED.** `complete_after_subcommand` (`mod.rs:260-315`) has no arm for `stage` + `adjudicate`, `admin-proof`, `amend`, `dispute-contract`, `dispute-findings`, `dispute-integrity` (six stage subcommands with a stage argument) plus the nested groups `contracts` and `review`. The exact number "7" was not recomputed; the list is what a fix must cover.
- **(d) `-S`/`--owner-stage` get no stage-id completion: CONFIRMED.** `complete_flag_value` (`mod.rs:197-241`) matches only `--stage`. `-S` is the short of `--stage` on nine `memory` subcommands (`src/cli/types_memory.rs:222-341`), and `--owner-stage` is at `src/cli/types_stage_disputes.rs:64`.
- **(e) `find_command_named` ambiguous DFS: CONFIRMED.** `commands.rs:20-25` returns the first match anywhere in the tree. Live repro: `loom complete bash . "loom review " "" "review"` prints `integrity status`, the subcommands of `stage review`, although top-level `review` is a leaf (`loom review --help`).
**Fix seam:**
- Replace `find_command_named` with `command_at_path` (`commands.rs:10`, already walks an exact path). Change `complete_subcommands(parent: &str, ..)` and `has_subcommands(command: &str)` to take `&[&str]` paths; callers are `complete_after_command` (`mod.rs:244`), the `("stage","output")` arm, and `tests/tests_commands.rs`. A top-level call passes `&[cmd]`.
- In `route_completion`'s match (`mod.rs:186-192`), replace the `[_cmd, "output", sub]` arm and the `_ =>` fallthrough with: for any `cmd_path` of length 2 or more, look up `command_at_path(&root, cmd_path)`. If it has subcommands, return `complete_subcommands(cmd_path, prefix)`. Else keep the existing explicit `(cmd, sub)` arms (status filters) for length 2, and for length 3 and above add `("stage","contracts"|"review", ..)` stage-id arms driven by the leaf command's first positional arg id (`stage_id`).
- Add `"--owner-stage"` and short `-S` to `complete_flag_value`'s stage arm (`mod.rs:~207`). `-S` conflicts with nothing in that function; check `prev_word` equals `"-S"`.
- Also add `"complete" | "completions"` to `MACHINE_PROTOCOL_COMMANDS` only if the owner wants it; not needed for correctness.
- Consumers: `src/completions/dynamic/{mod,commands}.rs`, tests in `src/completions/dynamic/tests/`.
**Regression test:** `src/completions/dynamic/tests/tests_routing.rs` (table at `:52`, rows `(label, prefix, prev_word, cmd_path, expected)`).
- Rows: `("stage contracts", "", "", &["stage","contracts"], <subcommand names>)`; `("--owner-stage", "core", "--owner-stage", &["stage","dispute-criteria"], ...)`; `("-S", "core", "-S", &["memory","note"], ...)`. Plus in `tests_commands.rs`: `complete_subcommands(&["review"], "")` must be empty. All fail today.
**Files a fix touches:** `src/completions/dynamic/mod.rs`, `src/completions/dynamic/commands.rs`, `src/completions/dynamic/tests/tests_routing.rs`, `src/completions/dynamic/tests/tests_commands.rs`.
**Size / tier:** M, needs-exploration.
**Overlap:** none found.

---

## CR-57 — `daemon_client::subscribe` token, `repair` exit code and special inodes

- **(a) `subscribe` reads the token from the CWD: CONFIRMED for the web path, latent for the TUI.** `src/commands/status/ui/tui/daemon_client.rs:53-57` calls `crate::commands::common::work_dir_path()` (CWD-resolved), while `connect()` (`:16-28`) uses the passed `work_dir`. The TUI callers pass `WorkDir::new(".")`'s root (`src/commands/status.rs:~45`), so they agree. The web broadcaster (`src/commands/status/web/broadcast.rs:170-218`) passes an explicit `base`, so with `base != CWD` the subscribe sends the wrong or empty token. `unsubscribe` (`app.rs:~140`) reads from `work_path`.
- **(b) `repair` exits 0 on failed fixes: CONFIRMED.** `src/commands/repair.rs:126-150` prints `issues_failed` and returns `Ok(())`; `dispatch.rs:339` returns that straight. The non-fix path also exits 0 with critical issues (a dry run, so leave it).
- **(c) `fix_invalid_work` fails on special inodes: CONFIRMED but marginal.** `src/commands/repair/workspace.rs:105-111`: `is_file()` follows symlinks and is false for sockets/FIFOs/devices, so `remove_dir_all` is called and fails with ENOTDIR. The detector only reports this for an entry that is neither dir nor symlink, which is rare.
**Fix seam:**
- (a) Change `subscribe(stream: &mut UnixStream)` to `subscribe(stream, work_dir: &Path)` and read the token with `read_auth_token(work_dir)` (the same call `unsubscribe` uses). Callers: `app.rs:136` (`connect_and_subscribe`, already has `work_path`) and `web/broadcast.rs:218` (`daemon_session`, has `work_path`). Use the same token location as `connect()` (`socket_path.parent()`, canonicalised) if the two differ; simplest is to have `connect` return the token it read.
- (b) After the summary block in `repair::execute`, `if result.issues_failed > 0 { bail!("{} repair(s) failed", result.issues_failed) }`. `main` already turns an `Err` into exit 1 (same as the other commands). Existing tests do not call `execute`.
- (c) In `fix_invalid_work`: `match std::fs::symlink_metadata(&work_path)`, `remove_dir_all` only when it is a real directory, else `remove_file`.
**Regression test:** `src/commands/status/ui/tui/daemon_client.rs` has an in-file test module (`:110`+): add a test that a token file written under a temp `work_dir` that is not the CWD is the one sent. `src/commands/repair/tests_actions.rs`: add a test that `apply_fixes` with a failing fix returns `issues_failed == 1`, then call a new thin `fn outcome(&RepairResult) -> Result<()>` for the bail (extract so it is testable). For (c), create a unix socket at the work path in `workspace.rs` tests and assert `fix_invalid_work` succeeds.
**Files a fix touches:** `src/commands/status/ui/tui/daemon_client.rs`, `src/commands/status/ui/tui/app.rs`, `src/commands/status/web/broadcast.rs`, `src/commands/repair.rs`, `src/commands/repair/workspace.rs`, tests as above.
**Size / tier:** S-M, mechanical.
**Overlap:** D4 (`daemon_client.rs`/`app.rs`), CR-77 (`RepairResult`).

---

## CR-58 — Config TUI edit input, table rendering, `loom usage` attribution, `loom ask`

- **(a) `dispatch_edit_key` accepts raw control characters: NARROWED.** `src/commands/config/tui/mod.rs:196-204` takes only the `KeyCode`. `Char(c)` goes to `append_char` (`src/commands/config/tui/state/edit.rs:100`) with no `is_control()` test. It also drops modifiers, so `Ctrl+<letter>` types the plain letter (a separate small bug). Terminals deliver most C0 bytes as `Ctrl+letter`, so the practical route is a pasted C1/Unicode control char.
- **(b) Table cells omit control-char stripping: REFUTED as an escape-injection risk.** `render/table.rs:208-240` passes the text to ratatui, whose `Buffer::set_stringn` drops any grapheme containing a control character (`ratatui-core-0.1.2/src/buffer/buffer.rs:351`, `.filter(|symbol| !symbol.contains(char::is_control))`). Values display minus those characters; nothing reaches the terminal. Using `inline_safe` would be cosmetic.
- **(c) `loom usage` suffix-based stage filter: CONFIRMED.** `src/commands/usage/discovery.rs:285-295` `filter_stages` keeps a file when `project_slug.ends_with("-worktrees-{stage}")`, with no anchor on this repository's slug, so another repo's worktree with a matching stage id is included in `--plan` totals. `stage_from_project_slug` (`:263-283`) and `parent_spawns.rs:~52` already anchor on `repo_slug`; `filter_stages` is the one that does not. `collect_projects` (`:56`) walks every project under the Claude projects root.
- **(d) `loom ask` deny-list env scrubbing: NARROWED (design choice).** `src/commands/ask/run.rs:152-160`, `is_scrubbed` (`:176`): drops `LOOM_*` and `SESSION_VARS`, passes everything else. The worker (`claude`/`codex`) needs `HOME`, `PATH` and its own auth variables, so an allow-list is a design change, not a bug fix.
- **(e) Un-unlinked tmp payload: REFUTED in practice.** `run.rs:115-170`: the payload is created `create_new`, mode 0600, inside a `tempfile::TempDir` that `run_worker` drops on every exit path. It leaks only if loom is SIGKILLed.
- **(f) `base.join` absolute override: REFUTED.** `src/commands/ask/eval.rs:97` joins `case.paths`; the field is documented to accept an absolute path inside the project (`eval.rs:26-27`), and `payload::locate` (`payload.rs:89-108`) canonicalises and requires the path to be under the project root, refusing symlinks.
**Fix seam:**
- (a) `dispatch_edit_key(state, code)` becomes `dispatch_edit_key(state, key: KeyEvent)`. Insert `Char(c)` only when `!c.is_control()` and the modifiers lack `CONTROL`/`ALT` (`dispatch_key` at `:178` already handles Ctrl+C before this). Call site `dispatch_key` passes the whole `key`.
- (c) Replace `ends_with(&format!(...))` in `filter_stages` with the anchored check used by `stage_from_project_slug`: pass `repo_slug` into `filter_stages` and reuse `stage_from_project_slug(&file.project_slug, repo_slug)` membership in `stages`. `DiscoveryOptions` needs the repo slug, from `ParentSpawns::new`'s same derivation (`WorkDir::new(&resolved).ok()?.repo_root().map(project_slug)`); lift that into a helper.
**Regression test:** (a) `src/commands/config/tui/` has `dispatch_key` tests (`rg 'dispatch_key' src/commands/config/tui`): assert a `Char('\u{85}')` and a `Ctrl+a` key do not change `edit_buffer`. (c) `src/commands/usage/` discovery tests: two project dirs `-repo-a-worktrees-core` and `-repo-b-worktrees-core`, `selected_stages={"core"}` with repo A, assert B is excluded.
**Files a fix touches:** `src/commands/config/tui/mod.rs` (+ its tests), `src/commands/usage/discovery.rs`, `src/commands/usage/parent_spawns.rs` (shared helper), usage tests.
**Size / tier:** (a) S, (c) M. needs-exploration for (c).
**Overlap:** CR-19 (`config/tui/mod.rs`).

---

## CR-59 — Web accept loop blocks on saturation

**Verdict:** CONFIRMED
**Evidence:**
- `src/web_server/serve.rs:107-126`: one thread runs `listener.accept()` then `unserved::spawn_connection` inline.
- `src/web_server/unserved.rs:16-19`: when `Slot::acquire(Lane::Connection)` fails (`MAX_CONNECTIONS = 64`, `limits.rs:12`), `connection::reject_unavailable` runs on that thread.
- `src/web_server/connection.rs:71-89`: `fail` calls `drain_pending` (50 ms read timeout, up to `DRAIN_DEADLINE` 300 ms, 64 KiB) and then `write_response` under `WRITE_TIMEOUT` 5 s per syscall. The doc at `:68` states "Runs on the accept loop" and the drain comment at `:34-38` acknowledges the stall.
- In practice the 503 body is tiny and fits a fresh socket buffer, so the write rarely blocks. The drain costs about 50 ms per ordinary rejection and up to 300 ms for a trickling client, during which no other client is accepted.
- The thread-spawn failure path (`unserved.rs:30`) has the same cost.
**Fix seam:**
- Add `connection::reject_unavailable_nonblocking(stream, body)` (or change `reject_unavailable`, which has only the two callers in `unserved.rs`): `stream.set_nonblocking(true)`, one non-blocking drain read of up to 64 KiB (no retry loop), `set_write_timeout(250 ms)`, `write_response`, `shutdown(Write)`. Keep `drain_pending` (blocking) for the connection-thread callers: `plan_server/request.rs:124`, `commands/status/web/config_api/request.rs:115`, `web_server/http.rs:327`.
- Trade-off: a request that arrives after the single drain pass may leave unread bytes and turn the close into an RST that can discard the 503 on Linux. Best effort is acceptable; the message is advisory.
- Alternative (rejected): hand the rejection to a short-lived thread. That spends a thread exactly when the server is saturated.
- Update the doc comments at `connection.rs:34-38` and `:68-70`.
**Regression test:** `src/commands/status/web/tests/socket.rs:354` fills `MAX_CONNECTIONS` and asserts the "connection limit reached" 503; extend it. Open one extra connection that sends one request byte every 40 ms for 2 s. While it is pending, a second extra connection must receive its 503 within 500 ms. Fails today when the trickler occupies the drain.
**Files a fix touches:** `src/web_server/connection.rs`, `src/web_server/unserved.rs`, `src/commands/status/web/tests/socket.rs`.
**Size / tier:** S-M, judgment (accept-loop behaviour under load).
**Overlap:** none found.

---

## CR-60 — Web terminal keepalive, `pair.slave` CLOEXEC, `TIOCSCTTY`, `Resize` in `Mode::View`

- **(a) No WS ping/pong keepalive: CONFIRMED.** `src/commands/status/web/terminal/bridge.rs:75-121` never sends a ping and never tracks inbound silence. `pump_socket` (`:231`) returns `None` for `Ping`/`Pong` frames (`parse_client_frame`, `tests_bridge.rs:349`). The model to copy is `src/plan_server/ws/conn.rs:23` (`KEEPALIVE = 10 s`) and `keep_alive` (`:130-136`). A silent peer with an idle PTY holds one of `MAX_TERMINALS = 8` (`src/web_server/limits.rs:36`) and a `tmux attach-session` client.
- **(b) `pair.slave` lacks `FD_CLOEXEC`: CONFIRMED.** `src/commands/status/web/terminal/pty.rs:24-51`: `openpty` (glibc) opens both ends without `O_CLOEXEC`. The code sets `FD_CLOEXEC` on the master only (`:34`), after the call, and the slave never. A concurrent `Command::spawn` elsewhere in the process (another terminal, tmux calls) between `openpty` and `drop(pair.slave)` (`:57`) inherits the slave, and for the master during the same window. Effect: another child keeps the slave open, delaying hangup detection. Low severity. Since this record was written, commit `2b5ecd081` moved the open into `pty.rs::open_pty` and made both ends close-on-exec from creation on Linux; the finding now holds only for the non-Linux variant, which still uses `openpty` and flags the master alone.
- **(c) `TIOCSCTTY` result ignored: CONFIRMED.** `pty.rs:46` `libc::ioctl(0, TIOCSCTTY, 0);` result dropped inside `pre_exec`. A failure would start tmux without a controlling terminal with no error.
- **(d) `Resize` accepted in `Mode::View`: CONFIRMED as unguarded.** `bridge.rs:259-261` applies `ClientFrame::Resize` for every mode, while `Input` is gated on `Mode::Control` (`:256`). The loom tmux code sets no `window-size` option (`rg window-size` is empty), so tmux's default `latest` lets a viewer's resize resize the agent's window. Decision between gating and keeping is a product call; recommend gating, since a viewer then sees tmux's own size but cannot disturb the worker. If a viewer's xterm then mis-wraps, the page should render at the server size (page change, out of scope here).
- **(e) "30 s stall + 2 s PTY shutdown hold": NARROWED.** These are deliberate documented bounds (`GATE_STALL_TIMEOUT`, `PtyChild::shutdown` at `pty.rs:118`). Not defects.
**Fix seam:**
- (a) In `run_with` add `last_heard: Instant` (reset in `pump_socket` on every `Ok(message)` including `Pong`, and whenever the gate clears) and `last_ping: Instant`. After the pumps, if `last_ping.elapsed() >= KEEPALIVE (15 s)`, `outbound_stop(socket.send(Message::Ping(Default::default())))`. If `last_heard.elapsed() >= PEER_SILENCE (60 s)` and `pending.len() < MAX_PENDING_INPUT` (gate shut means pongs are unread), break `Stop::Peer`. `tungstenite` queues the pong reply to a client ping itself.
- (b) `pty.rs`, in the non-Linux `open_pty`, right after `openpty`: `fcntl(&pair.slave, F_SETFD(FD_CLOEXEC))` next to the master call. `command.stdin/out/err(Stdio::from(slave.try_clone()?))` already dup2s into 0/1/2, which clears CLOEXEC on the destination.
- (c) In `pre_exec`: `if libc::ioctl(0, TIOCSCTTY as _, 0) == -1 { return Err(io::Error::last_os_error()); }`.
- (d) `pump_socket` (`:259`): `Some(ClientFrame::Resize(size)) if mode == Mode::Control`.
**Regression test:** `src/commands/status/web/terminal/tests_bridge/` (harness above; `Fixture`, `start_bridge_with`, `wait_for_close`, `join_within`).
- (a): a client that stops reading and never answers pings, script `sleep 30`, with a test-only short silence parameter. Needs `run_with` to take a config struct (`stall_timeout`, `keepalive`, `silence`); `start_bridge_with` passes defaults. Assert the server thread joins. Fails today.
- (d): `start_bridge(.., Mode::View)`, send a resize `{"resize":{"cols":100,"rows":30}}`, then read the child's `stty size` output; assert it stays `24 80`. Existing `tests_pty.rs` covers PTY sizing helpers.
- (b)/(c): `tests_pty.rs` can assert `/proc/self/fd` flags of the slave on Linux or spawn `sh -c 'readlink /proc/$$/fd/*'`; low value, optional.
**Files a fix touches:** `src/commands/status/web/terminal/bridge.rs`, `src/commands/status/web/terminal/pty.rs`, `src/commands/status/web/terminal/tests_bridge.rs`, `src/commands/status/web/terminal/tests_bridge/backpressure.rs`, `src/commands/status/web/terminal/tests_pty.rs`.
**Size / tier:** M, judgment (concurrency, fd lifetime).
**Overlap:** D1 (same functions; one worker).

---

## CR-77 — CLI/TUI minor edges, duplication, dead exports

- **Latent underflow in `append_suffixes`: CONFIRMED (latent).** `src/commands/status/ui/tui/ledger/quota.rs:250-258`: with `needed == 0` and `room < SUFFIX_GAP`, `available = 0`, `needed <= available`, and `room -= SUFFIX_GAP + needed` underflows. Every text built in this module starts with `"· "`, so `needed >= 2` today and it cannot trigger. Fix: `if needed + SUFFIX_GAP <= room`.
- **Unquoted `command.join(" ")` in `pressure --dry-run`: NARROWED.** `src/commands/pressure/mod.rs:109-115` and `:119` print `claude_args(..).join(" ")` as display text. It is output for a human, not executed. Arguments with spaces (the marker path, prompts) print ambiguously. A quoting helper exists: `crate::testrun::command::shell_words` (`src/testrun/command.rs`, `fn shell_words<'a>(values: impl IntoIterator<Item = &'a str>) -> String`). Replace the three `.join(" ")` calls. Test in `src/commands/pressure/` dry-run tests.
- **`process::exit(1)` in `request/status.rs:51-53`: REFUTED.** It follows the codebase's idiom (20+ commands use `process::exit` for a status code, for example `commands/knowledge/eval.rs:73`, `commands/plan/check.rs:153`). It runs after a `println!` and no destructor holds state. Leave it.
- **Unbounded `provider_data/sessions.rs:45`: NARROWED.** `fs::read_to_string` of a session markdown that loom wrote under its own state directory. Bounded readers exist (`crate::fs::safe_read::read_to_string_bounded`), so switching is cheap, but the risk is low.
- **Unbounded `telemetry/mod.rs:181-185`: REFUTED as a defect.** The comment at `:180` states the log only grows and a cap would turn its size into a read failure; it holds a shared lock. Revisit if the events file gets rotation.
- **Duplicated `compute_stage_levels`: NARROWED.** Both are two-line wrappers over one shared `crate::plan::graph::levels::compute_all_levels` (`src/commands/graph/levels.rs:7-14`; `src/commands/status/render/graph.rs:44-46`). The real residual is the tree row loop (sort, colour map, `level_counts`/`level_indices`, connector) copied between `src/commands/graph/tree.rs:66-122` and `src/commands/status/render/graph.rs:~215-265`. About 25 lines; extract only with a generic over stage accessors if wanted. Low value.
- **`MAX_STALL_RECOVERIES` duplicated: CONFIRMED.** `src/orchestrator/core/event_handler/recover_hung.rs:55` and `src/commands/stage/skip_retry_silence.rs:24`; the second one carries a "revisit when exported" note. `event_handler` is a private module (`orchestrator/core/mod.rs:17`). Put `pub const MAX_STALL_RECOVERIES: u32 = 2;` beside the field in `src/models/stage/types.rs:341` (or `defaults.rs`), import it in both files, and delete the two privates. S.
- **Unused exports, each checked with `rg`:**
  - `build_graph_display` (`src/commands/graph/display.rs:40`, re-exported `graph/mod.rs:29`): dead outside tests. `show()` uses `build_tree_display`; `graph/tests.rs` imports it via `super::display::`. CONFIRMED dead in production. Removing it removes `display.rs` (91 lines) and about 10 tests in `graph/tests.rs`; `compute_stage_levels`'s re-export would then also lose its production use (still used by `tree.rs`).
  - `RepairResult` (`src/commands/repair.rs:57`): REFUTED as dead. It is the return type of `apply_fixes` and read in `execute`. At most the visibility can drop to private; no outside importer (`rg RepairResult` only hits `repair.rs`).
  - `TuiActivityEntry` (`src/commands/status/ui/tui/state.rs:88`): REFUTED as dead; used in `state.rs:97, 147, 216-244`. Visibility only (`pub` to `pub(super)`), no outside importer.
  - `TuiApp` (re-export `src/commands/status/ui/tui/mod.rs:35`): REFUTED as dead; used by `run_tui` and `run_finished_report` in the same file, and by sibling modules via `super::`. The `pub use` could be `pub(super)`; no importer outside `tui/`.
  - `quota::credentials::keychain_argv` (`src/quota/credentials.rs:23`): REFUTED as dead; called by `keychain_raw` (`:85`) and asserted by `keychain_argv_is_exact`. Visibility only.
  - `quota::credentials::access_token` (`:41`): CONFIRMED dead. Production uses `account` (`poller.rs:144`) and the only other references are the test module's own shadowing `fn access_token` (`credentials_tests.rs:33`) and a doc link at `:69`. Remove it and its doc link, keep `resolve_token`.
**Fix seam:** one small worker for: `append_suffixes` guard, `shell_words` in pressure dry-run, the shared `MAX_STALL_RECOVERIES`, removal of `build_graph_display` (with `display.rs` and its tests) and of `access_token`.
**Regression test:** `src/commands/status/ui/tui/ledger/quota_tests.rs`: call `append_suffixes` with an empty text and `room < SUFFIX_GAP`; assert no panic (fails in debug today). Pressure: an arg with a space prints quoted. Stall const: compile-time (single definition).
**Files a fix touches:** `src/commands/status/ui/tui/ledger/quota.rs`, `src/commands/pressure/mod.rs`, `src/models/stage/types.rs`, `src/orchestrator/core/event_handler/recover_hung.rs`, `src/commands/stage/skip_retry_silence.rs`, `src/commands/graph/{display,mod,tests}.rs`, `src/quota/credentials.rs`, `src/quota/credentials_tests.rs`.
**Size / tier:** M overall (many small S items), mechanical.
**Overlap:** CR-57 (`RepairResult`), CR-58/CR-19 none.

---

## Cross-finding grouping hints for implementation workers

1. Bridge worker: D1 and CR-60(a, b, c, d), all in `src/commands/status/web/terminal/{bridge,pty}.rs` and its tests.
2. Tokens worker: D6, D7, D8 (`src/tokens/*` plus `src/daemon/server/orchestrator.rs`). D6 and D7 both change `FileCursor` and `read_source`.
3. Quota worker: E5 and CR-55 (`src/quota/{poller,cache,model}.rs`).
4. TUI/daemon-client worker: D4, CR-19, CR-57(a), CR-58(a).
5. Completions worker: CR-56.
6. Web server worker: CR-59.
7. CLI cleanup worker: CR-57(b, c), CR-58(c), CR-77.
