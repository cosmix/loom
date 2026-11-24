# Review Remediation Followups

> Open review suggestions and findings left out of the remediation plan

## Merge Landing and Quarantine

Reviewer suggestions left open on the operator-checkout landing ([Merge Flow](../architecture/merge-flow.md#merge-checkout-state)):

- **A file that changed during the move stays in the quarantine** and pauses the stage (`checkout_files/displace.rs` `unwind`); `linkat`-ing it back (it never replaces) and returning `UncommittedOverlap` would not.
- **`discard` trusts the post-move comparison.** It removes the quarantined copies without comparing again, so a writer holding an open descriptor can change an inode after `verify_moved`. Re-compare each entry with its blob before `remove_entry` and keep any that differ.
- **A failed `remove_attempt` after a landed merge is only a `tracing` warning.** The leftover copies of the merged blobs later block any merge of a stage with the same id (ids such as `integration-verify` recur across plans), and the remedy text tells the operator to move stale copies over current files. Report it on the success path or word the held block for that case.
- **`land` calls `pending.commit()?` after displacement.** It is memoised and infallible today; passing the commit id from `land_displacing` would make "no `?` after displacement" structural (`checkout_apply.rs`).
- **The held block repeats its remedy.** With no failures and `detail` set, `tree.rs` renders `kept in X ()` plus the remedy, and `retry_note` and `report_merge_block` print it again; an empty failure list with no detail still renders `kept in X ()` (no production path builds it). Special-case the empty list or shorten the held text.
- **The inbox `merge --resolved` handler has no early `holds_displaced_files` check** (`inbox_drain/merge_resolved.rs`), unlike `commands/stage/merge.rs`: a relayed resolution runs the whole `verify_resolved_completion` pipeline before `merge_stage` refuses on the quarantine.
- **`loom stage complete` on a held stage replaces the recorded per-file failures** with the held block's empty list (`progressive_complete/outcome.rs`); the detail survives only in `issue_history`. Keep the recorded block when it names the same quarantine.
- **Test gaps.** No test that `merge --resolved` refuses while the quarantine exists and charges nothing; none for the `states_its_remedy` gating in `report_merge_block` (inverting it would pass); the fail-closed root tests use a regular file (`ENOTDIR`), not a `chmod 000` root (same match arm in `held.rs`).
- **`main_repo_root` falls back to the cwd** when a linked worktree's admin gitdir is gone and `toplevel` fails, skipping the linked-worktree refusal, so `loom clean` would treat the stale worktree directory as the root (`commands/common/mod.rs`).
- **`loom init` and `loom clean` duplicate worktree-id collection plus `cleanup_multiple_stages` plus the warning print** (`init/cleanup.rs`, `clean/worktrees.rs`) with diverging error handling (init refuses read errors, clean flattens them); a shared helper would keep them in step.

## Planner Input Return

Open points on [returned input](../architecture/planning-session-bridge.md#recovery-and-its-do-not-destroy-rule):

- **`on_open` persists before the frame is handed over** (`chat/returned.rs`; the frame is built in `chat/mod.rs` through `Hub::open_frames`), the reverse of `on_returned`, which sends first so a crash delivers twice and never loses. A crash between the save and the send loses the input. Hand the frame over first, or document the in-process window.
- **Recovery treats returned input as saved unless a listener says otherwise** (`chat/mod.rs`): a failed `Weak<PlanContext>` upgrade during shutdown, or a listener panic caught in `hear()`, means no `input_unsaved` call, and `recovery.rs` removes `pending.json`. Require a positive save confirmation.
- **`return_input` bypasses the flushing claim** (`bridge/recovery.rs`): it publishes directly while the driver may be flushing resumed sessions, so the module contract "in order, from one thread at a time" (`bridge/mod.rs`) no longer holds. It also clears `UNSAVED` before `flush_event` drains events queued earlier, so a listener that reports unsaved or panics on an earlier event marks a cleaned session unsaved, keeps its directory and returns its texts again on the next restart (errs toward keeping data). Drain the outbox under the claim first.
- **A resumed session keeps `pending_saved` empty**, so `pending.json` keeps the already-returned texts through the final write and a second restart returns them again; setting `session.flow.pending_saved = texts.clone()` would let the next tick clear the file.
- **A tab closing between `connections_of` and `send_to_browser`** makes `deliver` reach no tab, and the drained entries are then removed from disk. Queuing a frame is not an acknowledged delivery anyway (known limit).
- **`queue_returned` frames go to every tab of the holding browser** (`pressure/driver/chat.rs`); with the intent check skipped, each open tab refills its own composer with the same texts.
- **Size limits.** `persist` clones every `Entry` into `EntriesFile` (a borrowing `Serialize` struct over `&[Entry]` avoids the copy), and once the entries pass `MAX_BYTES` `load` fails to read the file (a warning) and the next `persist` overwrites it, dropping older entries; hypothetical at realistic sizes, but the limit deserves a comment.
- **Accepted as written:** the commented `#[allow(clippy::let_and_return)]` on the frozen contract module in `pressure/tests.rs`, and `Settled` read only by tests in `pressure/baseline.rs` (a test hook).

## Dashboard and Runtime

- **Origin rules differ.** The planner's `plan_server/gate.rs::origin_matches` accepts an exact `http://` only; `web_server/access.rs::same_origin` accepts `http` and `https`, case-insensitively. The terminal lane's `origin_matches_host` (`terminal/upgrade.rs`) calls `same_origin` itself, so on a remote bind it accepts an `https://` `Origin` naming the socket, which `AccessPolicy::origin_allowed(Required)` refuses (`access_tests.rs`). Calling `policy.origin_allowed(local, head, OriginRequirement::Required)` in the lane would make the two consistent and let `origin_matches_host` go; align the planner wording or predicate.
- **`ProxyHandle::drop` cannot interrupt a handler in `connect_public`** (`native_worker/proxy/tunnel.rs`): it can wait `MAX_UPSTREAM_ATTEMPTS` x the connect timeout (about 40 s) plus DNS. Check the stop flag between attempts.
- **`process/owned.rs`:** `wait` and `wait_unreaped` are identical now (collapse them); `try_wait` still reaps (`host_verify/runner.rs`), leaving the pid-reuse window `wait` closes.
- **`daemon/deadline_transport.rs`:** `socket()` then `F_SETFD FD_CLOEXEC` is not atomic; use `SOCK_CLOEXEC | SOCK_NONBLOCK` on Linux, as `pty.rs` does.
- **After an orchestrator join timeout** `daemon/server/lifecycle/threads.rs` takes the token tracker thread, so the scheduler's later `stop_and_join` returns at once and its final scan can overlap a tracker pass.
- **`lsof -F` escapes non-ASCII path bytes as `\xNN`**, so a worktree path with non-ASCII characters never matches the parsed cwd (`orchestrator/terminal/native/pid_tracking.rs`).
- **Native worker journals** (`native_worker/`): a stage with a permanently corrupt record re-runs `repair_lost_tails`, with identity checks, on every runner tick (`runner_recover.rs`); a journal with no newline at all passes `has_torn_tail`, so `supervisor_is_gone` may tombstone an old unclaimed record and repair returns `Ok(false)` without a warning (`store_repair.rs`; check for a newline, or confirm `create_invocation` writes the first event atomically).
- **The poison-tolerant lock helper has about a dozen copies** (`remote_control.rs`, `proxy/mod.rs`, `broadcast.rs`, and others); one shared helper would replace them.
- **Test and comment nits:** `PathGuard` in `terminal/tests_upgrade/spawn_failure.rs` duplicates the one in `remote_control_tests.rs`; with the test override set, `tmux_program.rs` skips the socket-exists stat and the probe timeout (test-only); `web/src/api/terminal.ts` names `bridge.rs` for the 64 KiB cap, which is `MAX_INBOUND_BYTES` applied in `upgrade.rs`.
- **Seatbelt host-job socket location** (`verify/criteria/confine_scratch.rs::sccache_socket_dir`): on macOS the `h.sock` sits in the command's fresh scratch under the temp roots, where the record recommended a job directory under the state root. Whether a macOS Seatbelt profile can write that root and pre-bind `h.sock` is not settled statically; read the macOS hook job.

## Orchestration, Permissions and Verification Evidence

- **`crash_handler.rs`:** the not-answerable eviction also drops a crashed merge resolver's handle (merge sessions emit `SessionCrashed` for `MergeConflict` stages); confirm no merge path relies on that handle staying.
- **`fs/permissions/sync.rs::extract_permissions`** still collects and parses a deny vector that `fold_back_approvals` discards (`.0` only) and duplicates `git/worktree/settings.rs`. Return only the allow list or share one helper; the tests `test_extract_permissions` and `test_extract_permissions_empty` assert on the deny vector, so the change drops assertions and was left.
- **`inbox_drain/worker_control.rs`:** the post-lock re-check of `checked_stage` has no test for a status that changes between the two checks.
- **`host_verify/verdict_reuse.rs`:** a job directory whose `record.json` is missing (`Ok(None)`) is skipped silently, as the brief said (`save_slot` writes `record.json` first, so it needs a crash before the first write or a deletion outside loom). Treat it as unordered history like an unreadable record, so an older pass cannot stand for a newer job.
- **Tests and cost:** `tests_apply_gate_lock.rs` cannot tell the outside gate check from the gate run under the lock by the hand-back reason (same prefix; its strength comes from where the hook sits); `verify/review/verdict_records.rs::update` canonicalizes `work_dir` twice (pass the locked dir to the closure); each runner tick reads every stage's `current.json` twice (`host_verify/runner.rs`, acceptable at today's stage counts).
- **Relay line cap** (`commands/hook/relay.rs`): the 16-line predicate counts any existing `.req` entry (`symlink_metadata().is_ok()`), so a session able to inject output into the lead's call and plant 16 ticket files or links can crowd the lead's real control line out (dropped, never forged). Filtering on a regular file without following links would narrow it; same reach as the known limit in `relay/admission.rs`.

## Tokens, CLI and Plan Validation

- **Checkpoint over `MAX_CHECKPOINT_BYTES`** (`tokens/source_read.rs`): each pass sheds idle agents' tables and the next write rebuilds that agent from offset 0, so an active session that idles one pass re-reads its whole transcript, possibly repeatedly. Totals stay correct; rare (about 12 full tables). Limit rebuild frequency or log restarts. The anchor check also runs on every transcript every 5 s pass, unchanged files included (two opens plus up to 8 KiB of SHA-256 each); gating it on a saved mtime/ctime would keep unchanged files at one stat.
- **Completions** (`completions/dynamic/mod.rs`): the stage `contracts`/`review` leaf arm also offers stage ids after `freeze --test|--runner|--cwd` and `restore --contract`, whose values are not stage ids (harmless wrong candidates).
- **`plan/amendment/apply.rs`:** `std::path::absolute` runs before `plan_parent`, so the empty-parent fallback is unreachable at both call sites; if `absolute` predates the change, `relative_path_tests.rs` does not fail on the old code.
- **`commands/init/execute.rs`:** the tuple `if let` silently prints an empty-workspace summary if the preflighted plan and `base_branch` ever disagree; pass the preflighted plan into `prepare_workspace`, or `expect`.
- **`plan/graph/cycle.rs`:** `path.iter().position` is O(depth) per edge to an already-visited node; an on-path `HashSet` keeps detection linear on wide DAGs.
- **Test comment:** the doc comment of `break_session_discovery` (`daemon/server/final_capture_contract_tests.rs`) mentions a symlinked tokens directory the test does not create.

## Hooks, CI and Delivery

- **`loom-relay-guard.sh`:** `_common.sh` is not sourced when the payload check runs, so the `declare -F` branch never records a `bad-payload` ledger row for this guard, unlike the other nine. `loom-hooks/_bounded.sh`'s fallback jq-missing text duplicates `loom_jq_missing_message` in `_common.sh`. `loom-hooks/tests/pre-compact-handoff-bounded.sh` repeats the `timeout`/`gtimeout` shim setup of `session-end-reason.sh`; a shared `_`-prefixed helper would remove it.
- **`worktree-isolation.sh` checks only tokens containing `..`** (`traversal_kind`): an absolute token or `cd /main/checkout` is never class-checked, so absolute reads of the main checkout pass Bash while `worktree-file-guard.sh` blocks them for Read, Grep and Glob.
- **CI:** `.github/workflows/ci.yml` has a `shell` entry redundant with `.github/workflows/**` in the `changes` filter; with `if: !cancelled()` the binary-requiring hook tests also run after a failed build (gate on the build step's outcome as the test job does with `steps.build.outcome`), and the `hook-syntax` build step has no `if: !cancelled()`, so a parse or tool-install failure skips the build and the three gated steps that need the binary. `.github/workflows/release.yml` `MINISIGN_LINUX_SHA256` could not be checked against the published 0.11 tarball (no network): cross-check it on the release page before tagging. `test_rust_filter_covers_embedded_assets` scans only the `build/assets.rs` constants; extend it to `include_str!` / `include_bytes!` paths that leave the crate (`web/src/api`, `web/src/plan/api`, `loom-hooks`, `install.sh`, skills, agents, templates).
- **No-follow readers:** `context/source_graph/read.rs::open_regular` duplicates `context/census/classify.rs`'s opener (`fs::safe_read::open_regular_no_follow` is close but also enforces a single link), and `plan_server/summaries/codex_spawner.rs::log_tail` still opens with `File::open`, which follows symlinks; reuse `pressure::codex_run::log_tail`, as `pressure/codex.rs` now does.

## Web Frontend Sockets and Editor Drafts

- **Reconnect loops.** `web/src/plan/api/socket.ts::applyHello` resets `attempts`, so a frame the client rejects right after every hello causes a 1 s reconnect loop with no limit (dialog flicker, probe and resync each time). `web/src/api/ws.ts` `onopen` still resets `attempts`, so a socket that opens but never gets a usable snapshot reconnects every 30 s without reaching `offline`; `unprimedSince` is set when the attempt starts, so handshake time counts against the 30 s window, and each rejected frame restarts it, so a steady stream of rejected frames keeps the watchdog from firing.
- **Draft clearing** (`web/src/plan/editor/`). `edit-flow.ts` captures `adopted` for the editor's lifetime, so every later Discard or clean save clears that editor id's newer drafts, deleting a still-open tab's recovery copy (clear the adopted id once). With a requested edit, stored drafts are skipped and never cleared on a clean save, so the next open goes straight into a merge with the old draft (older behaviour). `draft-writer.ts::beforeWrite` runs even when `saveDraft` returns `too_large` without touching storage, so a first write over 1 MiB deletes the adopted restored copy with nothing replacing it; remove the replaced editor id only after `setItem` succeeds and skip it when it equals the writer's own id.
- **Test harness naming:** `edit-mode-kit.tsx` differs from `components/terminal/terminal-view-test-helpers.tsx`.
- **`web/src/api/ws.test.ts` is 429 lines**, over the 400-line limit, and no gate enforces it for `web/`. Splitting it is blocked by the test-integrity move detector (see below). `maps.contracts.test.tsx` "dismiss needs a reason and posts the status" failed once in a full vitest run under three concurrent cargo builds and passed alone and on the next run (a timing flake under CPU load).

## Contract and Check Tooling

- **A `loom check --contracts --replacement` trial failed every time** with "the tree does not match the request" (digest mismatch between submission and host job), so the replacement-dispute route could not be trialled for a frozen file that failed clippy `let_and_return`; the stage added a commented `#[allow(clippy::let_and_return)]` on the `mod` line in `pressure/tests.rs` instead.
- **The test-integrity move detector cancels a lost assertion only when its innermost enclosing named source-graph `Function` moved verbatim** (`verify/integrity/moves.rs`). Vitest and jest tests written `it('...', () => {...})` have anonymous callbacks, so a byte-identical move of a block into another vitest or jest test file always raises `TI-edit`. A split of `ws.test.ts` was reverted for this reason. Split a TS test file only with a `dispute-integrity` filing ready, or teach `moves.rs` to identify `it`/`test` callbacks by their title.
- **vitest 5 selects `-t` against the describe and test titles joined by `" > "`**, not one space, so every contract of a stage reported `NotSelected`. The `loom-typescript` skill's "joined by single spaces" rule is stale for vitest 5; either the adapter joins with `" > "` for vitest 5 or a writer restructures each contract as a top-level `it()` carrying the full plan name.
- **`loom knowledge sync` inside a knowledge-stage sandbox** updates `INDEX.md` but fails rebuilding the derived context catalog under `.loom/cache` (read-only file system); rerun it outside the sandbox.
- **A completion-command hook false positive:** `loom-control-complete.sh` refuses any Bash command whose text reads as a stage completion, even inside a heredoc, a Python edit or a heading that contains `completed_commit`. Write such text with the Edit or Write tools or read headings from a file.
- **Integration-verify's host job runs in place in the stage worktree**, so editing files while `loom check integration-verify` runs changes what later criteria see; wait for it to finish before a fix round.

## Knowledge and Graph

- **Per-checkout reconcile leases** let N worktrees on one `HEAD` with a missing base run N identical full base builds concurrently (previously serialised); a plan decision, revisit if memory pressure reappears (`reconcile_graph/lock.rs`). One lease file per checkout accumulates in the context cache with no cleanup; prune `reconcile-*.lock` in `loom clean` later.
- **Dirty deleted paths bypass the attribute filter** (`context/refresh/source_graph/layer.rs`), so `files_deleted` and `files_enumerated` can count linguist-generated files; the persisted overlay stays correct through `persist_layer` pruning.
- **`stage_cache/developer.rs`:** the doc comment naming `RETRY_AFTER` cannot link it (`RUSTDOCFLAGS='-D warnings' cargo doc` rejects a public item linking a private one); no test covers the poison recovery after a lookup that panics.
- **`scripts/retrieval-ab` and `doc/RETRIEVAL-AB-METHOD.md`** name the per-checkout lease file.

## Findings Left Out of the Remediation Plan

Each was left out of the November 2026 review-remediation plan for the reason given.

| Finding | Why it is left out |
| --- | --- |
| CR-48: prompt-hook retrieval parses a 399 MB view | A storage redesign; needs its own plan |
| CR-14 part 3: 28 TSX test files fail extraction | No newer grammar exists; tolerant extraction reverses a recorded design choice and adds a coverage state every consumer must handle |
| C6: correction-case tuning | Needs a measured run against the live corpus with a rebuilt binary, which a confined stage cannot do; run `loom knowledge eval` after installing the binary. The message precision, the generated-bundle pollution and a regression test are fixed |
| CR-20 part b: planner socket liveness watchdog | Needs a server heartbeat frame the planner socket does not have |
| CR-47: Swift Testing and Gradle skipped-only runs | Both need output captured from a real toolchain; the hand-written fixtures cannot settle them |
| CR-32: the `SUN_PATH_MAX` move and the `handoff`/`orchestrator` cycle | The move touches files four stages of that plan owned; the cycle is accepted ([Core Abstractions](../architecture/core-abstractions.md#layering-violations-known-issues)) |
| CR-68: process substitution under Seatbelt | The denial is recorded for the write form only; the read form needs a measured run on macOS |
| CR-71(c), CR-71(d) | Widening the size scanner adds ledger rows, a ratchet loosening that needs its own reviewed change; the coordinator doctrine conflict is a product decision |
| CR-29(b, c, d), CR-21 in its native-window form | The native terminal backend is being removed |
| CR-61(c-f), the rest of CR-76, CR-58(d), CR-19's config-TUI crash marker | Accepted limits already recorded, or design gaps with no settled fix |
| CR-78: signal module splits, the duplicate proposal document, the tree-row duplication | Already in the size ledger; the document's twin is an untracked local file; both copies of the loop sit in other stages' files |
| P1: attachments in the returned queue | Queue entries carry text only; attachments are a larger change |

After installing the rebuilt binary, run the macOS hook job once and read the Seatbelt result for the pinned state directory (CR-26) and for process substitution (CR-68).
