# Native Codex Qualification

>

## Qualification Items

No criterion could run a real Codex job (acceptance is confined: no network, no `~/.codex`), so `codex.backend = native`, the default, is unqualified against real jobs; an operator qualifies it on a disposable worktree. The eight items, in the order to run them ([backend design](../architecture/native-codex-workers.md)):

1. `codex --version` matches `0.169.0`; otherwise re-measure the method table in `doc/plans/briefs/native-codex-workers/common.md` section 8.
2. One real job in workspace-write mode (the only mode; hosts where bubblewrap cannot nest are refused): the report arrives, `git status` shows only the briefed files, and `loom usage` attributes the thread.
3. A cancel during a tool call and a job run to its deadline both end with ownership `released`.
4. Whether `codex app-server` honours the proxy variables for its streaming connection.
5. Forwarder tokens saved, measured against matched legacy jobs.
6. `codex app-server` starts and authenticates on the private Codex home and reads the project AGENTS.md.
7. A token refresh inside the boundary updates the host `auth.json`. `auth.json` is a file bind, so a Codex that replaces the file by rename fails with `EBUSY` and strands a rotated refresh token; the supervisor would then have to hold tokens itself through the `account/chatgptAuthTokens/refresh` server request.
8. In `WorkspaceWrite` a briefed write outside the worktree and a briefed `curl` both fail.

Two further questions need a real job: whether codex 0.169 loads `/etc/codex` system config (the boundary hides it) and untrusted project `.codex` config (the boundary binds `W/.codex` read-only), and whether the real standalone release ships `codex-package.json` (without it only the executable binds and codex loses `rg` and its bundled bwrap).

## Exposure Facts

- `auth.json` is readable by every process in the boundary and can leave over the four allowlisted OpenAI domains. It is bound read-write, so a prompt-injected worker can read the refresh token or overwrite it with another account's. This is the same exposure as the legacy lane. Narrowing it means copying auth into the tmpfs and writing back only a validated refresh.
- The allowlist proxy inspects no TLS: it admits CONNECT targets by host and port, so it cannot tell what a worker sends to an allowed domain.
- `sessions/` is bound from the host, so a worker can read and rewrite every operator rollout across projects; usage accounting reads those rollouts.
- Commands are kept off the proxy socket and the bridge's loopback port only by Codex's own seccomp filter and network namespace (checked against codex 0.169; [Boundary and Proxy](../architecture/native-codex-workers.md#boundary-and-proxy)). After a Codex upgrade, repeat that check: run `codex sandbox` nested in a payload boundary against a Unix socket and a loopback port, and expect both refused.
- Auth lock files bind only when they already exist; otherwise each boundary locks on its own tmpfs and concurrent token refreshes across workers and the host Codex are not serialised.

## Reviewed Codex Tool Profile

The boundary loads no operator Codex configuration, and a reviewed profile of which Codex tools a worker may use is not built. A follow-up decides it: execpolicy `allow` rules, hooks and MCP server commands run outside Codex's own sandbox, so until a profile exists the Loom boundary is the only enforcement.

## macOS Support

Not supported. macOS has no pid namespace, so `native_backend_qualified` fails there and the lane is `Unavailable` with the reason; a macOS host that wants Codex sets `codex.backend = legacy` and installs the plugin.

## Unimplemented Review Suggestions

Reviewer findings nobody implemented, grouped by area. File paths are under `loom/`.

**Polling and load cost (grows with invocation history; records are never pruned):**

- `native_worker/admission.rs` `admit_start` loads every stage record three times (`stage_disposition`, `unit_state`, `stage_units`); load once.
- `commands/subagents/wait/evidence.rs` calls `unit_state` per bound worker on every 2 s poll, each loading the whole stage, and replays the lifecycle journal even when every worker is native.
- `native_worker/supervisor/control.rs` `ControlPoller` re-reads and re-parses every `msg-*.json` each poll; skip names already in `reported`.
- `native_worker/runner_recover.rs` loads and folds every record, settled ones included, on every 5 s tick; `settle_lost_supervisor` can stall the tick for `REAP_GRACE_SECS` on each repeat when the ownership append keeps failing.
- `commands/usage/forward_join_native.rs` loads each stage's records twice (`stage_units`, `stage_threads`); `status/data/execution_models.rs` and `status/data/heartbeat_facts.rs` read every invocation's events about once a second per stage against the budget in `execution_models.rs`.
- `native_worker/store.rs` `read_report` reads up to 4 MiB then truncates to the requested bytes.

**Store and protocol:**

- `MAX_RECORD_BYTES` also caps `Terminal` and `Ownership` events, so a log at the cap can never settle; reserve room for closing events.
- `store.rs` checks existence then `renameat` (use `renameat2(RENAME_NOREPLACE)`); a failed create leaves a `.inv-*.tmp` directory; `write_cancel` check-then-publish lets concurrent writers replace each other's cancel.
- `store_io.rs` re-resolves the absolute invocation path with `O_NOFOLLOW` on the last component only; anchor at the work-dir descriptor.
- `protocol/report.rs` retains every turn's items; a large `final_answer` can exceed `MAX_RECORD_BYTES`. `machine/turns.rs` sends `turn/interrupt` for a turn learned from `turn/completed` while closed.
- `process/lines.rs` documents `Ok(false)` at EOF with nothing pending, but a partial final line also returns false and its bytes are dropped.
- `ids.rs` is the fourth copy of the `/dev/urandom` read (`web_server/token.rs`, `daemon/server/tokens.rs`, `plan_server/ids.rs`); `validate_stage_id` re-checks length and charset before `validate_id` and can drift from it.

**Boundary and proxy:**

- `boundary/mod.rs`: `hide_system_config` does nothing when `/etc/codex` exists but is not a directory; the main-checkout branch leaves `W/.worktrees` writable (unreachable today because admission uses `stage_worktree_path`); `native_backend_qualified` caches a failure for the daemon's life.
- `supervisor/preflight.rs` ignores `CODEX_HOME`, which `pressure/codex_run.rs` honours (token discovery also uses `$HOME/.codex`).
- `proxy/mod.rs` `Drop` joins handlers that may block in name resolution (resolver timeout plus up to four 10 s connects).

**Admission and hooks:**

- `inbox_drain/apply.rs` `admit_start` does not require the stage to be `Executing`; a `Blocked` or `NeedsHandoff` stage with a live session can admit workers.
- `loom-hooks/loom-control-subagents.sh` hard-codes the model and effort regexes (drift from `CODEX_MODELS`/`CODEX_EFFORTS` only causes false refusals) and bounds `--text` at 16384 characters while the CLI bounds bytes; control-character text can JSON-escape past the 64 KiB ticket limit and is refused cleanly.
- `loom-hooks/loom-control-complete.sh` `scan_completion_segment` skips only `NAME=` and `env`, so `time`, `nohup`, `exec`, `command`, `timeout N`, `{`, `if` or `!` before `loom stage complete` goes undetected; `loom-control-subagents.sh` resolves the command word with `loom_tokens_command_word_index` and the complete hook needs the same.
- `relay/emit.rs`: a stdout or stderr failure after `write_ticket` makes `relay_request` exit 3 with a ticket that may already be relayed; a message retried with a new id is delivered twice.
- `commands/subagents/native_start.rs` `read_text_file` opens before checking the file type, so `--file <fifo>` blocks the CLI.
- `cli/dispatch.rs`: an operator-shell `loom check` runs criteria locally with no native gate; it changes no stage state but its result can reflect a tree a worker is still writing.
- Start is admitted on a host that cannot qualify the backend and fails later in supervisor preflight; the lead-facing docs could say so.

**Duplication and test gaps:**

- Three duplicate native-worker seeding helpers (`host_verify/submit.rs`, `commands/stage/native_gate_tests.rs`, `commands/subagents/wait/native_tests.rs`); move one to a shared test-support module.
- `stage_disposition(..).blocker(..).is_some()` repeats in `recover_hung_forward.rs`, `hung_latch.rs` and `heartbeat_facts.rs`; a `NativeDisposition::holds()` helper removes it, and `hung_latch.rs` should check `native_held` last.
- `session_settings/contents.rs` `legacy_codex` re-implements `policy.rs::codex_granted`; `codex_backend.rs::native_qualified` duplicates the `OnceLock` in `boundary/probe.rs`; `commands/subagents/native_start.rs` repeats `native_control::work_root`; `start_in`/`message_in` carry unused `_out` parameters; `inbox_drain/worker_control.rs` has an `unreachable!` that matching `Start` with the other variants removes.
- `relay/matrix.rs` `WorkerControl` shares `policy_file_dispute`; give it its own policy function. `fs/permissions/tests/hooks_tests.rs` counts hooks (49 to 50) but asserts neither that `loom-control-subagents.sh` is registered nor that `contents.rs` drops it for non-Stage sessions.
- `tokens/discover.rs` and `status/data/execution_models.rs` use `index::stage_threads`, which silently drops corrupt records; count them into the report's partial diagnostics as `forward_join_native.rs` does. `requested_model` is shown without the `inline_safe` and length bound `append_codex_models` applies.
- The `Knowledge`-branch gate in `commands/stage/complete.rs` has no direct test; add one that completes a knowledge stage with an active native worker and expects refusal.

**Doctrine text:**

- The shared exit-6 text in `signals/format/codex.rs` says "a cancelled codex job needs nothing" and `codex_native.rs` then says "cancel it yourself"; give the native section its own last two lines. `codex_native.rs` says the command "only writes a relay ticket and exits 0" and omits that local refusals exit 3 with no ticket.
- `CLAUDE.md.template` (line 96) says "Codex uses `loom-hooks/codex-forward.sh`" unconditionally; it is frozen by `FROZEN_LINES`, so qualify it by lane when that allows.
- `signals/tests_codex_lane.rs` `assert_ne!(Legacy)` only has teeth on hosts with the legacy plugin; the equality assertion with `codex_lane(&work_dir)` carries the wiring check. Acceptable as is.

## Native Lane Smoke Observations

Two throwaway smoke runs on 2026-11-16 each sent one terra/xhigh unit adding one test to `native_worker/ids_tests.rs`. In the second, the unit ran a full Codex thread in `workspace_write` mode: admitted, thread started 5 s later, one turn of 18 s, report stored, outcome `succeeded`, ownership released; the edit was exactly the briefed one and touched no other file. That covers item 2 except the `loom usage` attribution, which is unchecked; items 3 to 8 have not been run.

That unit could not link its test binary. Units now get a writable `$TMPDIR` ([Boundary and Proxy](../architecture/native-codex-workers.md#boundary-and-proxy)), but no real Codex job has yet confirmed a unit can build and run its tests; the next smoke run should brief a unit to run one.

The first run failed before any thread on the proxy socket directory mode: see [Native Proxy Socket Dir Umask](../mistakes/codex-worker-briefing.md#native-proxy-socket-dir-umask).
