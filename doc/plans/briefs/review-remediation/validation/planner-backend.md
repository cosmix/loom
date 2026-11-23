# Validation: planning server, session bridge, pressure, run handoff, plan parser, memory entries

Revision `62be2a3d2`. All paths are repo-relative. Rust sources are under `loom/src/`, browser sources under `web/src/`.

## P1 — Pressure Start drops the queued texts returned to the starter

**Verdict:** CONFIRMED
**Evidence:**
- `plan_server/pressure/driver/chat.rs:23-34` ends every open authoring chat with `sessions.end(..)` and passes the drained texts to `return_queue`. `chat.rs:57-62` sends `ChatFrame::QueueReturned { texts }` to the starter's browser through `hub.send_to_browser(&self.run.browser, ..)`.
- `web/src/plan/chat/model.ts:335-341`: the `queue_returned` case breaks unless `pendingInterrupts` for the store is non-empty, and consumes one intent per frame. Only `commands.ts:157-163` registers an intent (`markInterruptPending`), for chat `interrupt`, `end` and `set_aside_kickoff`.
- `web/src/plan/chat/pressure/use-pressure.ts:37` (`startPressure`) is a plain `apiPost`. It registers no intent, so the frame is discarded. The composer cleared the texts when the sends were accepted (`composer-hooks.ts:240-249`), so the user loses them.
- The Rust side is covered by `plan_server/pressure/tests/authoring.rs:55-72`, which asserts the frame is emitted. Nothing tests the browser accepting it.
- The wire frame carries `Vec<String>` only (`plan_server/wire/chat.rs:19`, `web/src/plan/api/schema-chat.ts:249-252`, a `strictObject`). The queue entries themselves also carry text only (`isQueued` comment, `model.ts:63-70`), so "preserves attachments" is a larger change than the Pressure fix. Keep it out of this item.
**Fix seam:**
- Server: add an optional discriminator to the frame, `QueueReturned { texts, #[serde(default, skip_serializing_if = "Option::is_none")] cause: Option<QueueReturnCause> }`, with a single variant `Pressure`. Set it only in `driver/chat.rs:57-62`. `plan_server/chat/dispatch.rs:166` keeps `None`.
- Browser: `schema-chat.ts:249` gains `cause: z.literal("pressure").optional()`. `model.ts:335` accepts a frame whose `cause === "pressure"` without consuming an intent, and falls back to the intent rule otherwise. Unsolicited frames without the cause are still dropped, as the review asks.
- Consumers of `queueReturnedAtom`: `chat/stop-control.tsx:15-34` (`useQueueRefill`) already refills the draft. Other consumers: `web/src/plan/api/fixtures/chat-frames.json:79-82` and the wire contract test `plan_server/wire/tests_chat.rs:227-233`.
- Option not recommended: have `pressure-dialog.tsx:192` call `markInterruptPending` before `startPressure`. It is simpler, but it leaves a stale intent when the chat was empty, and the next unrelated frame is then accepted. The explicit cause is correct and small.
**Regression test:**
- Rust: extend `plan_server/pressure/tests/authoring.rs` (the `returned(&tab.frames())` helper at line 74) to assert `frame["cause"] == "pressure"`. Extend `plan_server/wire/tests_chat.rs` for the serde round trip.
- Web: `web/src/plan/chat/model-commands.test.ts` (`markInterruptPending` cases around line 68). Add a case: a `queue_returned` frame with `cause: "pressure"` and no registered intent sets `queueReturnedAtom` (fails today), and one without a cause still does not.
**Files a fix touches:** `loom/src/plan_server/wire/chat.rs`, `loom/src/plan_server/pressure/driver/chat.rs`, `loom/src/plan_server/wire/tests_chat.rs`, `loom/src/plan_server/pressure/tests/authoring.rs`, `web/src/plan/api/schema-chat.ts`, `web/src/plan/api/fixtures/chat-frames.json`, `web/src/plan/chat/model.ts`, `web/src/plan/chat/model-commands.test.ts`.
**Size / tier:** S-M (about 40 lines across both sides); mechanical.
**Overlap:** none. `concerns/plan-chat-followups.md#chat-web-follow-ups` mentions that `markInterruptPending` also covers End; this change touches the same function pair.

## P2 — Accepted but undelivered chat messages are lost on a server restart

**Verdict:** CONFIRMED
**Evidence:**
- `plan_session/bridge/commands.rs:311-335` (`Session::send`): when the session is not idle, a message is pushed to `self.flow.queue` (memory only) and `SendOutcome::Queued` is returned. When idle, it becomes `flow.delivering`, also memory only until pasted.
- `plan_server/chat/dispatch.rs:222-232` returns success to the browser, which clears the composer. The `SessionRecord` (`bridge/registry.rs:38-56`) has no queue or pending-delivery field.
- `bridge/recovery.rs:176-205` (`resume`) rebuilds a session from the record, spool and transcript, then immediately plans a `FinalWrite` end. It has no input to replay. A message never pasted has no transcript entry.
- Browser: `web/src/plan/chat/model.ts:298-309` drops a send that was marked `queued` once it leaves the queue unmatched, so after the reconnect snapshot no "Resend" is offered. Only kickoff sessions keep text, in the draft (`plan_server/chat/draft_cmd.rs:65-69`, text only, kickoff only).
- Not recorded under `doc/loom/knowledge/concerns/` (searched for queue loss on restart: no hit).
**Fix seam:**
- Open design choice. The recovered session is closing (`FinalWrite`, `EndCause::Draining`), so replaying the queue into it is not meaningful.
- Recommended: persist the undelivered queue (text, formatted message and attachment references, each with its queue id) next to the record as `queue.json`, using `crate::fs::safe_replace::replace_private(root, rel, bytes)` (`fs/safe_replace.rs:~74`, `pub(crate)`) so each write is atomic. Write it from `Session::publish_queue` and when `delivering` is set, and remove or rewrite it at confirmation or end.
- On `resume` (`recovery.rs:176`), load the file and return the items to the holder through a recovery notice or the draft, as P1's return frame does. Do not replay them. Mark an item that was `delivering` at the crash as "uncertain": show it, but do not auto-resend.
- Alternative (smaller, weaker): refuse to acknowledge a send until the record file is written. This does not fix an in-flight paste.
- Callers and consumers: `plan_session/bridge/flow.rs` (queue), `plan_server/chat/dispatch.rs`, `plan_server/chat/mod.rs` (`publish`), the web composer and `model.ts` send tracking.
**Regression test:** `plan_session/bridge/tests_recovery.rs` (harness `recorded(h, kind, pid)` at line 30, `Harness` in `bridge/harness.rs`). Queue a send while busy, build a second bridge over the same session directory, and assert the recovered report carries the text. Fails today because nothing is persisted.
**Files a fix touches:** `loom/src/plan_session/bridge/commands.rs`, `loom/src/plan_session/bridge/flow.rs`, `loom/src/plan_session/bridge/recovery.rs`, `loom/src/plan_session/bridge/registry.rs`, `loom/src/plan_server/chat/dispatch.rs`, wire frame and web files, tests.
**Size / tier:** L; judgment (data loss, replay-twice hazard, wire change).
**Overlap:** P3 (same registry write path and the same atomic-replace helper), P1 (the return-to-user frame), P6 (draft recovery).

## P3 — The session record is truncated in place; a failed write loses the last good identity

**Verdict:** CONFIRMED
**Evidence:**
- `plan_session/bridge/registry.rs:85-90` (`write_record`) calls `safe_locked_write_in_workdir` (`fs/safe_fs/write.rs:17-34`): open locked, `ftruncate(0)`, write, fsync. A crash or `ENOSPC` between the truncate and the write leaves an empty or partial file.
- `bridge/recovery.rs:208-224` (`record_step`) sets `session.record_dirty = false` before `write_record`, and on error only logs a warning. The change is not retried until something dirties the record again.
- `bridge/recovery.rs:119-125`: an unreadable record is left alone ("unreadable session record"), so the session's tmux server and Claude process are never reclaimed.
- Other readers hit the same window without a lock: `plan_session/hook_cli.rs:50-62` (`watch_target`, the owner watch, which the concerns file already records at `concerns/planner-ui-repair-followups.md#planning-sessions-can-outlive-their-server`; its `fs/safe_fs.rs:341-345` cite is stale, now `fs/safe_fs/write.rs:17`).
- Other callers of `write_record`: the provisional and complete records at `bridge/start.rs:193` and after spawn. A failure there is handled (`fail_start`).
**Fix seam:**
- `registry.rs:85`: replace the body with `crate::fs::safe_replace::replace_private(dir.path(), Path::new(RECORD), &bytes)` (`pub(crate) fn replace_private(root: &Path, rel: &Path, bytes: &[u8]) -> Result<()>`, `fs/safe_replace.rs`). It writes a temp file in the same directory and renames it, creating or replacing, mode 0600, with no symlink following. Keep `SessionDir` and the error context. The singleton lock is no longer needed because the writer is the driver only (the `record_step` doc already says so).
- `recovery.rs:208-224`: on `Err`, set `session.record_dirty = true` again (take the lock, find the session) so the next tick retries; keep the warning. Do not clear the flag before the write succeeds, or re-set it on failure.
- `restore_no_follow` and `replace_no_follow` refuse a missing target, so `replace_private` is the right one.
**Regression test:** `plan_session/bridge/tests_recovery.rs` plus `bridge/registry.rs` unit tests. Make the session directory unwritable (chmod 0500) for one tick, assert the on-disk record still reads (`read_record(&dir)`) and a later tick persists the newest generation. Fails today: the file is truncated or the flag is lost. A direct unit test can also assert `write_record` leaves no `session.json` of length 0 when the temp write fails.
**Files a fix touches:** `loom/src/plan_session/bridge/registry.rs`, `loom/src/plan_session/bridge/recovery.rs`, `loom/src/plan_session/bridge/tests_recovery.rs`.
**Size / tier:** S; mechanical (the helper exists), but it protects recovery identity, so have the diff reviewed.
**Overlap:** CR-61 (owner watch reads `session.json` during truncation: fixed as a side effect), P2.

## P4 — An abrupt pressure-server death loses the Codex configuration baseline

**Verdict:** CONFIRMED
**Evidence:**
- `plan_server/pressure/codex.rs:46-60` (`CodexHalf::start`): `before = snapshot(root)` is held in the struct field (memory only) while Codex runs.
- `codex.rs:147-175` (`finish`) and `codex.rs:252-270` (`configuration_changed`) compare `before` with a fresh snapshot, and `block()` (`codex.rs:277-286`) persists `StartBlocks::block`. Controlled stops reach `finish`.
- `pressure/install.rs:178-188` registers `sessions.on_recovered(..)` calling `interrupted_runs` (`pressure/recover.rs:59-75`), which only deletes the Codex report and prints a line. No snapshot comparison.
- `StartBlocks` (`planning/guidance/blocks.rs`) is a single `config-block.json` in the runtime directory. Every session start refuses while it exists (`planning/guidance/mod.rs:117-121`).
**Fix seam:**
- Persist the baseline, not a block. Writing a block before launch is wrong: it would refuse the run's own Claude sessions (`guidance/mod.rs:117`) and show a block in the options while the run is live.
- Add `ConfigSnapshot` (de)serialization (`pressure/config_guard.rs:48`, a `BTreeMap<String,String>`) plus `save(runtime) / load / clear` in the runtime directory (`config-baseline.json`), written with the same `safe_write_with_mode_in_workdir` as `blocks.rs:36`.
- `CodexHalf::start` writes it after the snapshot; `finish` and every controlled-abandon path clear it once the comparison is done (the places that call `configuration_changed`).
- In the `on_recovered` closure (`install.rs:178-188`), when a baseline file exists: re-snapshot, compare with `changed()`, and call `StartBlocks::block` with `CONFIG_CHANGED: ...` when it differs or cannot be read. If the file is unreadable, block with "cannot be checked". Then clear it. Recovery has already terminated Codex by then (`bridge/recovery.rs:clean` calls `terminate_children`), so the later snapshot is final.
- Option for a sturdier variant: write the baseline into the session directory. Rejected: recovery removes that directory (`clean` -> `remove_session_dir`) before `on_recovered` runs.
**Regression test:** `plan_server/pressure/tests/duties.rs:137` (`a_codex_configuration_edit_blocks_launches_until_confirmed`, helpers `f.codex("config", "")`, `call_on(&restarted, "/api/pressure/options", ..)`). Add: leave a baseline, edit `.claude/` config, build the recovery report for a pressure session, and assert `config-block.json` exists; plus a clean case with no edit leaves no block.
**Files a fix touches:** `loom/src/plan_server/pressure/config_guard.rs`, `loom/src/plan_server/pressure/codex.rs`, `loom/src/plan_server/pressure/install.rs`, `loom/src/plan_server/pressure/recover.rs`, tests.
**Size / tier:** M; judgment (security guard, restart semantics).
**Overlap:** none in the listed set.

## P5 — A refused handoff commit leaves the index changed

**Verdict:** CONFIRMED
**Evidence:**
- `plan_server/run/commit.rs:68-85` (`run_sequence`) runs `git add -- <addable>` for every ticked path that exists, before `git commit -- <paths>`. A tracked file with partial staging becomes fully staged; an untracked file stays staged.
- `commit.rs:95-103`: `index.reset(root, paths, "HEAD")` runs only when HEAD moved. A pre-commit refusal leaves HEAD in place, so nothing is undone.
- `plan_server/run/commit/tests.rs:164-191` (`failing_pre_commit_streams_both_streams_and_stops_later_hooks`) asserts the streams and HEAD but never the index.
- The commit itself uses explicit pathspecs (`--only` semantics, comment at `commit.rs:93`), so `git add` of tracked paths is not needed: git commits their working-tree content. Only paths git does not know need adding. The settle path already does exactly that: `plan_server/trace/settle_commit.rs:135-148` (`untracked`, then `run_literal(root, ["add","--"], &added)`, then `unstage` on refusal via `trace/settle_head.rs:118-132`, `git rm --cached -q -f --ignore-unmatch`).
**Fix seam:**
- `run_sequence`: replace `addable_paths` with the untracked subset, and on any non-moved HEAD or failed `git add`, call the unstage helper for just those paths.
- Reuse by promotion: `untracked` (`settle_commit.rs`, private) and `unstage` (`settle_head.rs:118`, `pub(super)`) move to `git/user_git.rs` (next to `run_literal` at line 59 and `IndexBefore`) as `pub(crate)`. Keep their signatures: `untracked(root: &Path, paths: &[&str]) -> Result<Vec<String>>` and `unstage(root, added: &[String], reason: String) -> String`. Update `trace/settle_commit.rs`, `trace/settle_head.rs` imports.
- A deletion or rename source in `paths` stays only on the commit pathspec, as today.
- Success path unchanged: `index.reset(.., "HEAD")` after HEAD moves.
**Regression test:** `plan_server/run/commit/tests.rs` using `Repo::new()`, `repo.put`, `repo.hook`, `repo.git_text`. Extend the failing-hook test: stage part of a tracked `doc/a.md`, add an untracked `doc/new.md` and an unrelated staged file, make the hook refuse, then compare `git diff --cached --name-only` and `git diff --cached` output before and after. Fails today: `doc/a.md` becomes fully staged and `doc/new.md` stays staged.
**Files a fix touches:** `loom/src/plan_server/run/commit.rs`, `loom/src/git/user_git.rs`, `loom/src/plan_server/trace/settle_commit.rs`, `loom/src/plan_server/trace/settle_head.rs`, `loom/src/plan_server/run/commit/tests.rs`.
**Size / tier:** M; judgment (it touches the user's index).
**Overlap:** CR-76 (`IndexBefore::reset` empty-target guard and `committed_bytes` in the same files).

## P6 — Kickoff continuation can lose the confirmed target; draft failures are only logged

**Verdict:** CONFIRMED
**Evidence:**
- `plan_server/chat/draft_continue.rs:65-84` (`continue_draft`): `drafts::create` with a `Seed` that has no target (`drafts.rs:90-95`; `Draft::started` sets `target: None`, `drafts.rs:107-119`). Then `on_confirm(ctx, state, &id, target)` (`draft_cmd.rs:71-75`) writes the target in a second update. `drafts::update` returns false and only logs on a write failure (`drafts.rs:247-274`). The caller ignores it and calls `drafts::remove(.., &draft.session)` at line 84. The old draft's target is gone while the live session holds the grant.
- The first half of the finding holds too: `StartDraft::record` (`draft_cmd.rs:47-53`), `on_send` (`draft_cmd.rs:65-69`) and `kickoff_cmd.rs:39` all log or ignore failure. The browser is not told.
- Guard that limits it: a failed `drafts::create` does end the new session and refuse (`draft_continue.rs:73-79`), so only the target write is exposed.
**Fix seam:**
- `drafts.rs:90`: add `target: Option<String>` to `Seed`; `Draft::started` uses `seed.target`. Callers of `Seed {..}`: `draft_cmd.rs:39` (`None`), `draft_continue.rs:65` (`target.map(str::to_owned)`), tests (`tests_drafts.rs:24,69,254`).
- `continue_draft`: drop the separate `on_confirm` for the continuation, keep `record_owner`. Do not remove the old draft unless `create` returned Ok (already true).
- Surfacing recovery failure to the user (start/send path) is a product choice with no existing UI: recommend a one-line `tracing::warn` stays, plus a returned flag only if the browser already has a notice slot. Treat it as optional, not part of the fix now.
**Regression test:** `plan_server/chat/tests_dispatch/draft_commands.rs` (continue cases at lines 88-144; `plan_draft(f, written)` at line 23; helpers `continue_cmd`, `draft_file`). Add: continue a draft whose target document exists, then assert the new draft file already holds `target` straight after `create`, and, with the drafts directory made unwritable after `create` (or an injected failure), the old draft still holds it. Fails today: the target is written in a separate step.
**Files a fix touches:** `loom/src/plan_server/chat/drafts.rs`, `loom/src/plan_server/chat/draft_continue.rs`, `loom/src/plan_server/chat/draft_cmd.rs`, `loom/src/plan_server/chat/tests_drafts.rs`, `loom/src/plan_server/chat/tests_dispatch/draft_commands.rs`.
**Size / tier:** S; mechanical.
**Overlap:** P2 (draft recovery), `concerns/planner-ui-repair-followups.md#planner-review-fixes-unchecked-here-and-reviewer-suggestions` (Drafts bullet, a different defect).

## P7 — Run admission reads state before it takes the job guard

**Verdict:** NARROWED
**Evidence:**
- `plan_server/run/start.rs:88-102` (`accept_start`): `state_view` and `admit` run first, `jobs(..).begin(RunJob::Start)` after (line 101-103, `child.rs:159-168`). A job that finishes, or begins and ends, in between is invisible to the decision.
- `RunOnly` (`start.rs:36-37`, `sequence` at `start.rs:195-199`) runs bare `loom run`, so the child cannot verify which plan it resumes.
- Guard that narrows it: `begin` is a mutex check, so two plan-server jobs cannot overlap, and an active job at read time makes `admit` refuse or reflect the transient state. The wrong-plan outcome needs a second plan-server job (abandon, clean, start) to finish and replace the active plan inside the few-millisecond window. The CLI run outside the server is outside the guard in any case.
**Fix seam:** in `accept_start`, call `begin` first, then `state_view` and `admit`, and let the guard drop on refusal (`JobGuard::drop`, `child.rs:183`). Return the existing `409 busy` when `begin` fails. Moving the `begin` above `state_view` also covers P7's "bind the identity": the state read now happens under the guard. Passing the selected plan to `loom run` is not needed; skip it.
**Regression test:** `plan_server/run/start/tests.rs` and `tests_refusal.rs` (`state(..)` helper from `super::tests`). A barrier-based test is hard because the window is in one function; assert instead that a start refused by `admit` leaves no job active, and that a start while a job is active gets `busy` before any state read (inject a counter).
**Files a fix touches:** `loom/src/plan_server/run/start.rs`, `loom/src/plan_server/run/start/tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** none.

## CR-44 — The metadata fence scanner closes at an inline triple backtick

**Verdict:** CONFIRMED
**Evidence:**
- `plan/parser/extraction.rs:102-109`: `metadata_section[yaml_content_start..].find(&closing_fence)` matches the first run of backticks anywhere, including mid-line in a YAML string such as `description: "wrap code in ``` fences"`. The body is cut and parsing fails or loses content.
- The opening fence is found by `find_yaml_fence` (`extraction.rs:~125-150`), also line-position blind.
- `plan/amendment.rs:256-278` (`splice_metadata_yaml`) re-uses the original opening fence length and appends `suffix` from the original closing fence. If an amendment's serialized YAML itself contains a line starting with a backtick run of at least that length, the written plan no longer parses.
- Consumers of the extractor: `plan/amendment/apply.rs:55,96`, `plan/amendment_catch_up.rs:128`, `orchestrator/core/merge_handler/auto_merge_dispatch.rs:137`, `plan_server/files/plan_view.rs:45`, and the parse path (`extract_yaml_metadata`).
**Fix seam:**
- `extraction.rs`: add a helper `find_closing_fence(section, from, fence_len)` that scans line by line and accepts a line whose content after at most 3 leading spaces is a backtick run of at least `fence_len` and nothing but whitespace after it (CommonMark close). Keep byte offsets and CRLF behaviour (the doc comment promises byte-for-byte ranges). Use it at line 102.
- `splice_metadata_yaml`: compute `needed = max(fence_len, longest line-leading backtick run in new_yaml_body + 1)`. When it exceeds the original length, rewrite the opening and closing fences with the wider run. Because the signature returns `String` (not `Result`), widen rather than refuse.
- Decision: do not forbid backticks in YAML. Widening is what the review recommends and is what CommonMark supports.
**Regression test:** `plan/parser/extraction.rs` tests (module `tests`, `test_extract_yaml_metadata` at line 182 builds the content) plus `plan/amendment/tests.rs`. Case 1: a YAML string with an inline ``` before the real closing fence parses fully (fails today). Case 2: an amendment whose new text contains a line of backticks round-trips through `splice_metadata_yaml` and `extract_yaml_metadata_with_ranges`.
**Files a fix touches:** `loom/src/plan/parser/extraction.rs`, `loom/src/plan/amendment.rs`, `loom/src/plan/amendment/tests.rs`.
**Size / tier:** M; needs-exploration (fence arithmetic and CRLF).
**Overlap:** CR-46 (same `amendment` path).

## CR-46 — `apply_amendment` and a bare relative plan path

**Verdict:** NARROWED
**Evidence:**
- Confirmed part: `plan/amendment/apply.rs:36-41`: `plan_path.parent().canonicalize()` on `Path::new("plan.md")` is `Path::new("").canonicalize()`, which fails with `ENOENT`.
- Refuted part: lock identity. `fs/locking.rs:37-48` (`lock_plan`) already maps an empty parent to `.` and canonicalizes it before hashing, so `plan.md` and `./plan.md` take the same lock.
- Reachability: both production callers pass a resolved path. `commands/stage/amend.rs:81-83` and `orchestrator/adjudication/apply.rs:259-261` take it from `fs::resolve_source_path`, which joins a relative `source_path` onto the canonical project root (`fs/mod.rs:166`). The bare relative path occurs only for a direct API caller or a test.
- Confirmed part: `apply.rs:56` (`prepare_content`) uses `fs::read_to_string`, unbounded. `fs::safe_read::read_bounded(root, relative, max_bytes)` (`fs/safe_read.rs:59`) exists.
**Fix seam:** in `checked_plan_path`, `let parent = match plan_path.parent() { Some(p) if !p.as_os_str().is_empty() => p, _ => Path::new(".") }`. In `prepare_content`, read through `read_bounded` with the plan size cap used elsewhere. The lock call needs no change. Low value; do it only with CR-44 if that item already edits this module.
**Regression test:** `plan/amendment/tests.rs` (or `retry_tests.rs`): `apply_amendment(Path::new("plan.md"), ..)` with the current directory set to a temp project. Fails today with a `No such file` error from `canonicalize`.
**Files a fix touches:** `loom/src/plan/amendment/apply.rs`, `loom/src/plan/amendment/tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** CR-44.

## CR-61 — `plan_server` / `plan_session` boundaries (three bullets, six sub-claims)

**Verdict:** NARROWED (per sub-claim below)

- **61a. A re-upload of identical bytes under another media type leaves an orphan blob counted against the 256 MiB cap.** CONFIRMED. `plan_server/uploads/store.rs:82-103` (`put_within`): the data name is `<id>.<ext>` but the sidecar name is `<id>.json`, so a second media type adds a second blob and overwrites the sidecar. `stored_bytes` counts both. Already recorded in `concerns/planner-ui-repair-followups.md`, heading "Planner review fixes: unchecked here and reviewer suggestions", bullet "Uploads". Worth fixing now, cheaply: in `put_within`, when the sidecar exists and its `media_type` differs, remove the old blob (`safe_remove_in_workdir`) before publishing, or reject with the stored metadata. Test: `plan_server/uploads/` store tests (`put_within` takes a cap). Size S, mechanical.
- **61b. The sweep runs only at server start.** CONFIRMED, small. `uploads/mod.rs:50-58` (`install`) calls `store::sweep` once. With `KEEP_SECONDS` at 30 days and a long-lived server, expired files stay until restart; the cap is the only bound. Not recorded under concerns. Fix: call `sweep` at the start of `put_within` when the cap check fails (under the lock already held) and retry once. Size S. Judgment optional: accepted limit if the server is expected to restart; recommend the retry-on-full variant.
- **61c. `ServerLock::write_record` truncates in place while the owner watch reads without the lock.** NARROWED. `plan_server/lock.rs:123-130` (`set_len(0)`, write) and `plan_session/owner_watch.rs:51-75` (`read_holder`, `O_NOFOLLOW`, unlocked). A read inside the window returns a parse failure, so one poll sees "owner not alive". The watch only tears down after `OWNER_GRACE` (15 minutes) of continuous absence (`owner_watch.rs:30-41`), and the next poll refreshes `last_owner`. The window is microseconds, so it is harmless. Not worth fixing; the same truncation pattern for `session.json` (`hook_cli.rs:50-62`) is fixed by P3.
- **61d. The owner watch is single-shot and kills only the tmux socket, not detached children.** CONFIRMED as described, ACCEPTED. `owner_watch.rs:96-100` (`teardown_socket`) and the watch started once from the launch script. Recorded at `concerns/planner-ui-repair-followups.md`, heading "Planning sessions can outlive their server" (single-shot watch, one `kill-server` attempt). Children (a pressure Codex process) are terminated only by recovery of the next server (`bridge/recovery.rs:clean` -> `terminate_children`). Not a fix for now.
- **61e. The `PreToolUse` guard omits `Bash`** (`plan_session/sandbox/mod.rs:53`, `PRE_TOOL_USE_MATCHER`). REFUTED as a defect; it is the specified design. Bash is held by layer 1 (OS sandbox) and layer 2 (`sandbox/permissions.rs` allow list of `loom ...` commands). Recorded at `concerns/planning-session-sandbox-limits.md`, heading "Accepted limits the implementation confirmed" ("Bash writes inside `doc/loom/`", "Locked mid-session, for Bash", "New top-level paths"). No fix.
- **61f. Check-to-write TOCTOU in the guard** (`plan_session/guard/mod.rs:165-194`, `evaluate` then `check_write`/`confined_file`). CONFIRMED, ACCEPTED. Recorded at the same heading, bullet "Check-to-write symlink swap". No fix.

**Fix seam (61a, 61b only):** `loom/src/plan_server/uploads/store.rs` (`put_within`, `sweep`). Helpers: `safe_remove_in_workdir(dirfd, &Path)` already imported there; `stored::locate(&dir, id)` (`uploads/stored.rs`).
**Regression test:** `plan_server/uploads/store` tests (existing `put_within` cap test): put the same bytes as `text/plain`, then `text/markdown`, assert one data file remains and `stored_bytes` equals the body size. Fails today with two blobs.
**Files a fix touches:** `loom/src/plan_server/uploads/store.rs`, `loom/src/plan_server/uploads/mod.rs`, its tests.
**Size / tier:** S; mechanical.
**Overlap:** P3 (61c's sibling window).

## CR-75 — plan and memory minor edges (four sub-claims)

**Verdict:** NARROWED

- **75a. `EntryBuilder::build` trims `content`, `MemoryEntry::validate` compares exactly.** CONFIRMED, fails closed. `fs/memory/types.rs:198-206` compares `restored.content != self.content`; `types.rs:283` (`build`) stores `content.trim()`. `commands/memory/handlers/record.rs:97` passes the raw CLI text to `MemoryEntry::new`, with no trim anywhere in `commands/memory/`. So `loom memory note "text\n"` is refused with "does not round-trip through the journal", and `validate_content` (`persistence.rs:80`) checks the untrimmed length. No corruption. Worth fixing now (it hits a normal shell argument with a trailing newline): trim `content` (and `context`, which the parser also trims at `parser.rs:94`) once in `MemoryEntry::new` and `with_context`, so the stored value and the round trip agree. Callers: `record.rs:97`, `handoff/session_content.rs:163`, `commands/hook/review_harvest.rs:339`, `orchestrator/adjudication/prompt/freeze_refusal.rs:117`, `commands/memory/handlers/pending_groups.rs:134`. Test: `fs/memory/mod.rs` tests (`MemoryEntry::new(.., "text\n")` then `append_entry` succeeds; fails today). Size S, mechanical.
- **75b. `.contains("..")` rejects valid names like `foo..bar.rs`.** CONFIRMED, fails closed at plan validation. Four sites, not three: `plan/schema/validation.rs:351` (`working_dir`), `:403` (artifacts), `:420` (wiring source), and `:545` (`regression_test.file`). Fix: one helper `has_parent_component(path: &str) -> bool { Path::new(path).components().any(|c| c == Component::ParentDir) }` (the pattern used in `commands/hook/worker_brief.rs:293`), used at all four. Test: `plan/schema/tests/` validation tests (a wiring source `src/foo..bar.rs` is accepted, `a/../b` still rejected). Size S, mechanical.
- **75c. `criterion_hazards.rs:149-165` uses raw `.contains("loom map")`.** CONFIRMED, and stronger than a warning. The same function drives the v2 lint at `plan/schema/validation/v2_lints/sandbox_capability.rs:154-166`, which is an error (`LintFinding::in_stage(.., true)`) unless the policy is after-stage confined. So `rg -n 'loom map' doc/x.md` or `echo "loom knowledge context"` as a criterion is refused. Fix at the shared owner (`criterion_hazards.rs:149`): take lexed argv instead of text and match the command words, reusing `runs_loom` and `visit_stage_argvs` (`v2_lints/mod.rs`) and `shell_lex::Word`, as `v2_lints/loom_subcommands.rs` does. The warning caller at `validation.rs:622` (`warn_ungrantable_acceptance`) also needs the lexed form (`shell_lex::lex`, `validation/base_tree.rs:20` shows the call). Test: `plan/schema/tests/v2_lint_command_tests.rs` (new case: a criterion that only greps for the phrase yields no finding). Size M, needs-exploration.
- **75d. Wiring regexes compile without `RegexBuilder::size_limit`** (`validation.rs:440`, not 689-699). REFUTED in effect. `Regex::new` applies the crate's 10 MiB default limit and the pattern is capped at 500 characters (`validation.rs:432-437`). The verifier's tighter limit is already enforced as an error lint: `plan/schema/validation/v2_lints/regex_patterns.rs:28-47` (`PATTERN_SIZE_LIMIT`), tested at `plan/schema/tests/v2_lint_command_tests.rs:227`. No fix.

**Files a fix touches:** `loom/src/fs/memory/types.rs`, `loom/src/plan/schema/validation.rs`, `loom/src/plan/schema/validation/criterion_hazards.rs`, `loom/src/plan/schema/validation/v2_lints/sandbox_capability.rs`, tests under `loom/src/plan/schema/tests/` and `loom/src/fs/memory/`.
**Size / tier:** S for 75a and 75b, M for 75c; mechanical for 75a and 75b, needs-exploration for 75c.
**Overlap:** none in the listed set.

## CR-76 — own-write race, settle error masking and smaller items (eight sub-claims)

**Verdict:** NARROWED

- **76a. `OwnWrites` pre-rename registration consumed by an earlier pending batch.** CONFIRMED mechanism, low impact. `planning/summary/write.rs:157-166` registers before the rename; `planning/watch.rs:184-214` (`drop_own_writes`) calls `own.take(rel)` for every reported summary path, matching or not ("matching or stale", by design). If a pending batch already holds the same path and is flushed between `register` and the rename, the registration is consumed against the old hash and the engine's own write is later reported as an external change: one spurious notification, no data loss. The window is the microseconds between `register` and `rename`, so skip unless cheap: keep the registration when the hash differs from the current one (drop on the first match, expire by age). Test: `planning/watch` tests (register, walk, rename, walk). Size S, judgment (design reversal of a stated rule). Recommend: accepted limit.
- **76b. `committed_bytes` maps a non-zero `git cat-file` to `Ok(None)`.** CONFIRMED, worth fixing. `plan_server/trace/settle_commit.rs:115-121` returns `output.status.success().then_some(stdout)`, so a corrupt repository, a locked database or a bad object all read as "no such file". The doc comment promises `Err` when git could not answer. The caller `trace/settle_job.rs:129-131` (`commit_holds_settled`) turns `Err` into "holds" (`map_or(true, ..)`) but `Ok(None)` into "does not hold", so `changed_since` reports `HOOK_CHANGED` (blames hooks). Fix: first decide presence with `git ls-tree -z <sha> -- ./<path>` (success with empty output means absent; any failure is `Err`), then `cat-file blob` and treat a non-zero exit as `Err`. Test: `plan_server/trace/` settle tests: point `committed_bytes` at an unknown sha and assert `Err`. Size S, mechanical.
- **76c. `IndexBefore::reset` has no empty-targets guard.** CONFIRMED latent. `git/user_git.rs:~145-150` builds `targets` from `ours` and `others` and runs `git reset -q --` with no pathspec when both are empty, which resets the whole index. Both callers pass at least one path (`run/commit.rs:97`, `trace/settle_head.rs:96`), so it is unreachable today. Fix: return early with an empty `IndexReset` when `targets.is_empty()`. Test: a unit test in `git/user_git` tests with `paths = &[]` and a staged file that must stay staged (fails today). Size S, mechanical.
- **76d. `PublicId::mint().expect(..)`** (`plan_server/ids.rs:49-55`). ACCEPTED. It panics only if `/dev/urandom` cannot be read; the `uuid` v4 used across the server has the same dependency. No fix.
- **76e. A tokenized URL in the browser argv** (`plan_server/browser.rs:55-67`, from `plan_server/mod.rs:282`; the URL is `http://127.0.0.1:<port>/?token=...`, `plan_server/startup_tests.rs:26`). CONFIRMED, accepted by design: the same URL is printed on the terminal and the server is loopback by default. On a shared host another user can read it from `/proc/<pid>/cmdline` while the browser runs. Mitigating it needs a one-time exchange URL, which is a product change. Not for this plan.
- **76f. Unbounded `presence::State.browsers`** (`plan_server/presence.rs:55-60`, `gate.rs:80-83`). NARROWED. A `BrowserId` is any 32-hex string (`ids.rs:20-30`), but `admit` requires the access cookie first (`gate.rs:108-114`), so only a token holder can grow it, one small entry per id. The module doc (`presence.rs:1-7`) states the bound. Accepted.
- **76g. Unguarded `batch_hooks`** (`plan_server/watch.rs:69-71`, `Watcher::spawn` loop in `planning/watch.rs:151-165`). CONFIRMED, low. A panic in any hook kills the `loom-plan-watch` thread, and the planner stops refreshing with no log. Hooks are in-tree code. Fix: wrap the hook call in `catch_unwind` with a `tracing::error` (S). Recommend: small, optional.
- **76h. Blocking `flock` in `planning/summary/lock.rs:109`** (`lock_exclusive` in `acquire`). CONFIRMED, accepted. It waits for any other holder, including a hung `loom plan summary` CLI; callers are engine threads. A bounded try-lock loop would be the fix; no incident is recorded. Not for this plan.

**Fix seam (76b, 76c, optionally 76g):** `loom/src/plan_server/trace/settle_commit.rs:115`, `loom/src/git/user_git.rs` (`reset`), `loom/src/plan_server/watch.rs:69`.
**Regression test:** as listed per item.
**Files a fix touches:** `loom/src/plan_server/trace/settle_commit.rs`, `loom/src/plan_server/trace/settle_job.rs` (tests), `loom/src/git/user_git.rs`, `loom/src/plan_server/watch.rs`.
**Size / tier:** S overall; mechanical.
**Overlap:** P5 (same `user_git.rs`, `settle_commit.rs`, `settle_head.rs`); promote helpers once, in one stage.
