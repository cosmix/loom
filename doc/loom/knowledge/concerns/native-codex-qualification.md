# Native Codex Qualification

>

## Qualification Items

No criterion could run a real Codex job (acceptance is confined: no network, no `~/.codex`), so `codex.backend = native`, the default, is unqualified against real jobs; an operator qualifies it on a disposable worktree. The eight items, in the order to run them ([backend design](../architecture/codex-concurrency.md)):

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
- Commands are kept off the proxy socket and the bridge's loopback port only by Codex's own seccomp filter and network namespace (checked against codex 0.169; [Boundary and Proxy](../architecture/codex-concurrency.md#boundary-and-proxy)). After a Codex upgrade, repeat that check: run `codex sandbox` nested in a payload boundary against a Unix socket and a loopback port, and expect both refused.
- Auth lock files bind only when they already exist; otherwise each boundary locks on its own tmpfs and concurrent token refreshes across workers and the host Codex are not serialised.

## Reviewed Codex Tool Profile

The boundary loads no operator Codex configuration, and a reviewed profile of which Codex tools a worker may use is not built. A follow-up decides it: execpolicy `allow` rules, hooks and MCP server commands run outside Codex's own sandbox, so until a profile exists the Loom boundary is the only enforcement.

## macOS Support

Supported when `boundary/probe.rs::qualify_macos_from` passes its three probes (`sandbox_exec`, `payload_boundary`, `codex_home_honoured`; [Backend and Lane Selection](../architecture/codex-concurrency.md#backend-and-lane-selection), [macOS Boundary](../architecture/codex-concurrency.md#macos-boundary-seatbelt)). A host whose Codex is a script wrapper, or a process already inside a sandbox that forbids nesting `sandbox-exec`, is `Unavailable` with the probe's reason; such a host sets `codex.backend = legacy`.

**Kernel and host checks still awaiting a real host run.** The stage sandbox denies `sandbox-exec` (a nested profile exits 71; a syntax error exits 65, so profile syntax is checkable), the boot-id sysctl and tmux sockets, so the operator runs these from a plain macOS shell with native Codex installed:

- `LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --manifest-path loom/Cargo.toml --lib -- --include-ignored open_issues_macos_completion_kernel native_macos_` selects seven tests: the completion kernel fixture `open_issues_macos_completion_kernel_default_binds_nothing_and_keeps_ancestor_cargo_files_hidden`, `native_macos_worker_lifecycle`, `native_macos_quiescence_finds_and_kills_a_fresh_session_and_its_children` and four `seatbelt_profile_tests::kernel::native_macos_*` cases. Without the strict flag they print SKIP and report `ok`.
- `cargo test --test subagent_owned_wait native_progress` fails in the sandbox with `failed to size macOS boot ID` (`process/boot_id.rs`).
- `host_verify::tests_child_base::provision_runs_before_the_criterion_and_a_failure_stops_the_job` and the five other confined tests there, and four of the seven real-tmux `tmux::tests_history` tests, skip silently in a sandbox; the strict flag turns a skip into a failure.
- The Linux `cfg` paths of the boundary and `verify/criteria/confine_host.rs` (`ancestors`, `seatbelt_files` declared without `cfg`) were not compiled for Linux on the macOS host that built them (the rustup Linux target lacks `core`); the hosted Linux lint and test jobs are their first compile.

**Host-dependent doctrine tests.** `exploration_doctrine_contract_tests.rs` formats signals through `codex_lane()`, which is `Unavailable` wherever `native_backend_qualified` fails (a stage session on macOS, a host without a usable Codex): the native signal then carries `push_codex_unavailable_fallback`, not `push_codex_intro` (`format/codex.rs`), so the native surface is only checked on a host that qualifies. `push_codex_intro` is shared by Legacy and Native (`codex_native.rs`).

**Open macOS limits** (reviewer suggestions, none implemented): auth copies are read-only (`private_root.rs`), so a Codex token refresh cannot persist and may invalidate the host `auth.json`; no `/dev/ptmx` or file-ioctl grant (`seatbelt_profile.rs`), so PTY-based Codex exec may fail; no writable Cargo home or stage-cache environment (`macos_spec.rs`), so cargo's `.package-cache` lock under a read-only `CARGO_HOME` may break worker builds; worker suites binding `127.0.0.1:0` fail by design (no bind grant); `preflight.rs` reruns the full qualification, including a real Codex app-server launch, in every supervisor; `teardown.rs::stop_unrecorded` keeps the control channel so `retire_mac` waits the whole EOF grace; `macos_procs.rs` allocates `arg_max` per process on every 25 ms census; `boundary/mod.rs` omits `.codex` from the read-only list; `supervisor/mod.rs` comments that Codex's sandbox always runs on Linux; `retire_parts` (`macos.rs`) and `teardown_mac::retire_mac` duplicate the kill-retire-reclaim sequence; `loom_program.rs::TestOverride` is process-wide, so a non-serial test could see it; `seatbelt_profile.rs::quote` uses `to_string_lossy` (a non-UTF-8 path would fail to match, so refuse it) and `network_rules` canonicalises only the socket's parent; `spellings` partly duplicates `confine_ancestors::seatbelt_spellings`; the listener and control fds are passed by number, so a daemon with fd 0, 1 or 2 closed could have them overwritten (assert both are 3 or higher); `quiescence_scan.rs` links `judge`, which is `cfg`-gated off non-test Linux builds; there is no kernel test for hard links into read roots, a failed rollout write or sync leaves its `.loom-partial` file in the host sessions tree until a retry, each rollout holds a clone of its directory descriptor (descriptors grow with the number of rollouts), and a rollout nested deeper than `codex_discovery::MAX_DEPTH` is removed with the root without a log line. `loom main` runs the terminal recovery and update check for `subagents worker-exec` (`main.rs` `MACHINE_PROTOCOL_COMMANDS`, `UPDATE_SILENT_COMMANDS` do not list `subagents`), so the bridge inside the profile may fork an update refresh and touch stdout before Codex.

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
- `session_settings/contents.rs` `legacy_codex` re-implements `policy.rs::codex_granted`; `commands/subagents/native_start.rs` repeats `native_control::work_root`; `start_in`/`message_in` carry unused `_out` parameters; `inbox_drain/worker_control.rs` has an `unreachable!` that matching `Start` with the other variants removes.
- `relay/matrix.rs` `WorkerControl` shares `policy_file_dispute`; give it its own policy function. `fs/permissions/tests/hooks_tests.rs` counts hooks (49 to 50) but asserts neither that `loom-control-subagents.sh` is registered nor that `contents.rs` drops it for non-Stage sessions.
- `tokens/discover.rs` and `status/data/execution_models.rs` use `index::stage_threads`, which silently drops corrupt records; count them into the report's partial diagnostics as `forward_join_native.rs` does. `requested_model` is shown without the `inline_safe` and length bound `append_codex_models` applies.
- The `Knowledge`-branch gate in `commands/stage/complete.rs` has no direct test; add one that completes a knowledge stage with an active native worker and expects refusal.

**Doctrine text:**

- The shared exit-6 text in `signals/format/codex.rs` says "a cancelled codex job needs nothing" and `codex_native.rs` then says "cancel it yourself"; give the native section its own last two lines. `codex_native.rs` says the command "only writes a relay ticket and exits 0" and omits that local refusals exit 3 with no ticket.
- `CLAUDE.md.template` (line 96) says "Codex uses `loom-hooks/codex-forward.sh`" unconditionally; it is frozen by `FROZEN_LINES`, so qualify it by lane when that allows.
- `signals/tests_codex_lane.rs` `assert_ne!(Legacy)` only has teeth on hosts with the legacy plugin; the equality assertion with `codex_lane(&work_dir)` carries the wiring check. Acceptable as is.

## Native Lane Smoke Observations

Two throwaway smoke runs on 2026-11-16 each sent one terra/xhigh unit adding one test to `native_worker/ids_tests.rs`. In the second, the unit ran a full Codex thread in `workspace_write` mode: admitted, thread started 5 s later, one turn of 18 s, report stored, outcome `succeeded`, ownership released; the edit was exactly the briefed one and touched no other file. That covers item 2 except the `loom usage` attribution, which is unchecked; items 3 to 8 have not been run.

That unit could not link its test binary. Units now get a writable `$TMPDIR` ([Boundary and Proxy](../architecture/codex-concurrency.md#boundary-and-proxy)), but no real Codex job has yet confirmed a unit can build and run its tests; the next smoke run should brief a unit to run one.

The first run failed before any thread on the proxy socket directory mode: see [Native Proxy Socket Dir Umask](../mistakes/codex-worker-briefing.md#native-proxy-socket-dir-umask).

## Open-Github-Issues Review Suggestions: Codex Lane

Reviewer findings of the bounded-exploration and startup-deadline work nobody implemented. Paths are under `loom/` unless noted.

- `native_worker/supervisor/mod.rs:124` checks startup expiry with the pre-`recv` clock reading, and `recv` blocks up to `RECV_TIMEOUT` (200 ms), so a real handshake frame that arrives up to 200 ms past the deadline is accepted. Use `RECV_TIMEOUT.min(startup_at - now)` as the receive timeout while startup is armed.
- `orchestrator/signals/format/codex.rs` repeats the bounded-exploration paragraph verbatim in both branches of `push_codex_intro`; emit it once after the `if`/`else`.
- `native_worker/supervisor/startup_tests.rs`: `a_thread_started_in_time_disarms_the_startup_deadline` passes even without `self.startup_at = None` in `apply`, because the machine ignores `StartupDeadline` once `InTurn`; it overlaps the frozen startup contract and adds another `#[path]` testkit copy.
- `commands/subagents/wait/mod.rs` throttles human progress lines with the supervisor's `PROGRESS_EVENT_INTERVAL_SECS`; a dedicated constant would decouple the two cadences.
- `skills/loom-orchestration/SKILL.md` states the watch prints one initial and one terminal record; human mode also prints per-worker progress lines. Note it if the `BLOCK_C` pins allow.
- `skills/loom-plan-writer/references/codex-implementers.md` omits the "equivalent sandbox" condition that `BLOCK_B` and the signal state for the bounded-exploration license.
- `exploration_doctrine_contract_tests.rs`: `signals()` formats only a mixed claude+codex stage, so the codex-only branch of `push_codex_intro` is unpinned (a codex-only `Implementers` for both lanes would pin it); `OLD_WORDING` rejects fixed phrases, so a reworded hand-off of debugging to Codex would pass, and the positive reservation check (`assert_full_reservation`) is the real guard.
- `native_worker/supervisor/lifecycle_tests.rs` sends a real `turn/start` through the real proxy where the spec asks for an offline `initialize` or session operation; `retire_parts` and `teardown_mac::retire_mac` duplicate a sequence ([macOS limits](native-codex-qualification.md#macos-support)).
- `process/owned.rs`: the `wait_unreaped` test calls `owned.retire()` after `try_wait` already reaped the child, so `killpg` targets a reaped pid; `Drop` retires anyway, so the call can go.

## Long Codex Runs Starve the Loom Heartbeat

A foreground Codex forward can outlast the session heartbeat because PostToolUse runs after a blocking call. Sandboxed output, process names, job files and logs do not prove that a worker is the authorized launch, so the hold rests on what the sandbox cannot reach (`orchestrator/monitor/direct_worker.rs`, `codex_lifecycle.rs::forward_is_fresh`):

- `codex-forward-guard.sh` runs outside the sandbox, mints an invocation id, records it in a ledger the sandbox cannot write, writes it into the tool call it authorizes, and refuses every other tool command quoting a minted id. The authorized launch is the one direct launcher child of the PID-verified agent whose kernel-visible argv holds the id; two such children are refused.
- **Direct lane:** only CPU under the supervisor (`python3 _codex-direct.py`) counts, excluding the supervisor's own polling. No worker under it means no hold.
- **Companion lane:** the launch's own `status <job> --wait` names the job, and the job's log must be fresh, so a planted job and a touched log hold nothing without the authorized wait.
- A forward with no receipt yet tries both; one finished by its receipt holds nothing while a process lingers. The window is `RECEIPT_WINDOW_SECS` (1560 s) from `authorized_at`, with one idle budget.

Limits:

- Where the launcher runs the command with no outer process outside the sandbox (macOS `sandbox-exec`, an unsandboxed session), a stage that reads an id from the ledger can `exec` that launcher into a matching argv. That earns only the bounded CPU hold any foreground command earns. Revisit if the harness exposes a per-tool-call host identity.
- Inside a real authorized companion launch, the job file and log stay stage-writable, so a stage can stretch the hold to the end of that launch, inside the same window. Closing it needs a host-owned progress signal.
- A real bubblewrap-launched direct forward's argv has not been observed; the check fails closed. The macOS `ps` argv path has not been executed, and macOS tick totals exclude reaped children.

## `loom status` Badge Follows the Stage Budget

The status activity badge and the monitor share the same foreground evidence and bounded idle and wall-window decisions. Direct receipts alone do not authorize a progress hold. Transcript data supplies only the display label: a live process with fresh observed progress shows Busy while an idle process expires. Native macOS execution remains a CI check. Rules: [status-data-model.md](../architecture/status-data-model.md#heartbeat-and-activity).
