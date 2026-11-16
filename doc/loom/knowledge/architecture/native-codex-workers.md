# Native Codex Workers

>

## Backend and Lane Selection

A stage's main agent starts a Codex worker with `loom subagents start --provider codex`; no Claude session sits between the lead and Codex. The daemon admits the request, starts one supervisor process per worker, and the supervisor runs `codex app-server` inside a bubblewrap boundary with its own pid namespace. The backend is the config key `codex.backend` (`native` default, `legacy`; `user_config/keys.rs`, per-key project tier; `codex_backend.rs::configured_backend`).

`codex_backend.rs::codex_lane` returns `CodexLane::{Unavailable(reason), Legacy, Native}`. No codex executable gives `Unavailable`; `Legacy` needs the companion plugin; `Native` needs `boundary::native_backend_qualified()` (Linux and a working bubblewrap pid namespace; a `OnceLock` caches a failure for the daemon's life, so installing bwrap later needs a daemon restart). A configured native backend never turns into the legacy lane silently. The signal's launch text follows the lane (`orchestrator/signals/format/codex.rs`, `codex_native.rs`). macOS has no pid namespace, so the native lane there is `Unavailable` and a host that needs the forwarder sets `codex.backend = legacy` ([qualification](../concerns/native-codex-qualification.md)).

Plans name the implementer (`codex`), never the mechanism; only `codex.backend` picks it, resolved at the execution layer, never from plan or signal text. `loom hook codex-backend` prints `configured_backend` for the hooks: in a live stage session with a native backend `spawn-guard.sh` denies a `loom-codex-forwarder` or `codex:*` spawn (`_spawn_policy.sh::loom_native_lane_spawn`) and `codex-forward-guard.sh` refuses any Bash call of `codex-forward.sh` (`_codex_forward.sh::loom_native_lane_forward`), both naming `loom subagents start --provider codex`; with a legacy backend `loom subagents start` refuses and names the forwarder spawn (`native_start.rs`). A missing binary or failed query keeps legacy behavior. None of these routes transparently: a hook cannot turn an Agent call into a Bash call, `loom subagents start` is lead-only while the forwarder is a subagent, and a forward's invocation id is minted only for a forwarder identity.

## Record Store

`<work_dir>/native-workers/<stage>/<invocation>/` (`native_worker/store.rs`, `store_io.rs`), directory 0700, files 0600. The boundary never mounts it and a stage session cannot write under `.loom`. It is separate from `lifecycle.jsonl` because a new `WorkerIdentity` variant would make every reader and three shell hooks reject the journal (`deny_unknown_fields`).

| File | Writer |
| --- | --- |
| `manifest.json`, brief.md | daemon, once, at admission (brief.md is a byte copy of the brief) |
| `events.jsonl` | daemon writes `Admitted`; then the supervisor only; the daemon again once the supervisor is proven gone |
| `supervisor.json` | the supervisor, first act, `O_CREAT\|O_EXCL`; or the daemon as a tombstone (`pid` 0). Whoever creates it owns the invocation |
| `boundary.pending` | the supervisor, empty, just before `boundary::spawn` (`store::mark_spawn_begun`) |
| `boundary.json` | the supervisor, once, before `initialize` is sent |
| `control/cancel.json`, `control/msg-<id>.json` | daemon or operator CLI |
| report.md, `provider.log`, `supervisor.log` | supervisor; the provider log is capped at 4 MiB |

Writes of manifest, boundary, control and report files are temp-file-then-rename with `fsync`; `events.jsonl` is append-only with `O_NOFOLLOW`, one fsynced line per append. Reads use `O_NOFOLLOW`, require a regular file owned by the current uid and read at most `MAX_RECORD_BYTES` (4 MiB). Constants live in `ids.rs`: six active workers per stage, a 1500 s deadline, 256 KiB briefs, 1 MiB frames, 16 queued messages of at most 16 KiB, interrupt grace 5 s, end-of-input grace 2 s, reap grace 2 s, a progress event every 30 s. Unit and message ids match `^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$` (no dots); invocation ids are `inv-` plus 32 lowercase hex digits; stage ids follow `validation::validate_id` (up to 128 characters, `ids::validate_stage_id`).

`model.rs` folds events into `NativeUnitState` (phase `Admitted`..`Terminal`, outcome, ownership `Held`/`Released`/`Unverified`); `index.rs` is the only read API for consumers. `NativeEvent` is internally tagged: serde ignores extra fields on unit variants even with `deny_unknown_fields`, so an unknown-field test needs a struct variant (`model_tests.rs`).

## Boundary and Proxy

`boundary/mod.rs::spawn` runs bubblewrap with `--die-with-parent --unshare-all --new-session`, a read-only root, an allowlisted environment and no network beyond loopback. The worktree is the only writable project path; `W/.loom`, `W/.git`, `W/.claude` and `W/.codex` stay read-only. The Codex home is a private tmpfs holding only `sessions/` (created 0700 when missing), the auth files and AGENTS.md, so config.toml, rules, hooks, MCP servers, plugins and skills never load. `/etc/codex` is hidden (`hide_system_config`). The decisions behind the mount rules, each pinned by a boundary test:

- A symlinked `W/.git` is refused. A real `W/.loom/work` directory is hidden with `--tmpfs` plus `--remount-ro`; a `.loom/work` symlink that resolves into `W` is refused.
- The codex package directory is bound whole only for a standalone release (`<release>/bin/codex` beside `codex-package.json`); any other layout binds the executable alone. A package root that contains HOME, the codex home or the worktree is refused, so a codex at `~/bin/codex` cannot expose `~/.ssh`.
- `PATH` gains `<CARGO_HOME or HOME/.cargo>/bin` ahead of `/usr/local/bin`, and the boundary reuses `CargoScratch` (`bind_read_roots`, `pub(crate)`) for a private writable Cargo home; the probe passes no HOME so uv and bun overlays are skipped.
- `nested_sandbox_supported()` decides the sandbox mode: bubblewrap nests, then `WorkspaceWrite`; otherwise `danger-full-access` with the `externalSandbox` turn policy and the Loom boundary carries the whole policy. The mode is recorded in `BoundaryCreated`. Codex's own sandbox is defence in depth: execpolicy `allow` rules, hooks and MCP commands run outside it.

`boundary::verify_quiescent` is the only proof of release: the boot id matches and no process visible in `/proc` carries the recorded pid-namespace inode (a different boot id means released). A `PermissionDenied` readlink counts as not-a-member only when `/proc/<pid>/status` shows a foreign real and effective uid or a single `NSpid` entry; trusting the directory owner would let a member hide with `PR_SET_DUMPABLE 0`.

`proxy/` serves an allowlist CONNECT proxy on a Unix socket. `connect_target_allowed` splits at the last colon, requires port exactly 443, lowercases the host and strips one trailing dot, refuses IP literals, empty or bracketed hosts and any character outside `[a-z0-9.-]`, and matches exact entries or `*.suffix` (never the bare suffix). The proxy refuses a resolved loopback, private or link-local address. The allowlist is `crate::codex::CODEX_SANDBOX_DOMAINS`. Inside the boundary `bridge.rs` (`loom subagents worker-exec`) exposes the socket on loopback TCP and sets the proxy variables. The proxy inspects no TLS ([exposure facts](../concerns/native-codex-qualification.md#exposure-facts)). Dropping the handle joins handlers that may block in name resolution, which delays supervisor exit.

## Supervisor, Runner and Settlement

`supervisor/mod.rs::run_session` drives a pure reducer (`machine/`: `Machine`, `Input`, `Action`) over a `Transport` and a `Clock`, so tests use a scripted peer. Each iteration polls control files (`control.rs`, about 200 ms), receives one frame, then feeds `Deadline` and `GraceElapsed` inputs and writes a `Progress` event after 30 quiet seconds. `PipeTransport::send` uses `try_send` on a 64-frame queue; a full queue is `TransportClosed`, so a provider that stops reading stdin never stalls cancel or deadline handling. A provider request for user input gets an error reply and a `ServerRequestRefused` event. Usage comes from rollouts under the shared `~/.codex/sessions` (the supervisor opts out of `thread/tokenUsage/updated`).

Teardown (`supervisor/teardown.rs`) closes the app-server's input, waits `EOF_GRACE_SECS`, kills the outer bubblewrap process, runs `verify_quiescent`, then appends `Terminal` and `OwnershipReleased { by: "supervisor" }` or `OwnershipUnverified`. A failure before any namespace exists (proxy start, bwrap spawn) settles `Released` without a check.

`runner.rs::NativeRunner` (daemon side) spawns one `loom subagents supervise` per invocation only when `supervisor.json` is absent and this process has not spawned for it, never retries a spawn, and holds children as plain `Child` because a supervisor outlives the daemon. A reaped child is settled by the next recovery pass (`runner_recover.rs`, every 5 s, claim rules), not in `reap`, because a supervisor from an earlier daemon may still own a young record; a spawn failure tombstones `supervisor.json` before writing `Failed` and `Released`. `settle_lost_supervisor` releases without proof only when `boundary.pending` is absent; marker present and `boundary.json` absent records `Failed` and `OwnershipUnverified`.

## Admission and Ingress

`RequestKind::WorkerControl` (wire name `subagents:control`, `relay/kind.rs`) is a control kind allowed for `SessionType::Stage` only; `payload_native.rs` carries `Start`, `Message` and `Cancel`. `inbox_drain/worker_control.rs` calls `native_worker::admission` under the existing `fs/stage_admission` lock, which closes the check-then-launch race. `admit_start` checks in order: ids, model in `CODEX_MODELS`, effort in `CODEX_EFFORTS`; the stage's `implementers` includes codex; the brief (absolute, opened `O_NOFOLLOW`, regular, canonical path inside the stage worktree, at most 256 KiB, UTF-8, sha256 match); `stage_disposition` is not `Unverified`; an identical request for a running unit returns `AlreadyAdmitted` and a different one is refused (idempotency by unit id, no caller key); fewer than six non-terminal invocations. It does not require the stage to be `Executing` ([concerns](../concerns/native-codex-qualification.md#unimplemented-review-suggestions)). Start is admitted on a host that cannot qualify the backend and fails later in supervisor preflight.

The CLI (`commands/subagents/native_start.rs`, `native_control.rs`) is `start`, `message`, `cancel`, `release` (operator only), `status`, `result`, plus hidden `supervise` (refuses when `LOOM_SESSION_ID` or `LOOM_STAGE_ID` is set) and `worker-exec`. `start`, `message` and in-session `cancel` validate locally (refusals exit 3 before any ticket exists), write one ticket, print the relay line and exit 0, because a non-zero exit after submission would skip the relay hook and strand the ticket against the 32-ticket quota. `loom-hooks/loom-control-subagents.sh` pins the exact command spelling, accepts only the stage's main agent and accepts `--unit` as an alias of `--unit-id`. Ledger notes: `native worker <unit> admitted as <inv>`, `... already admitted as <inv>`, `message <id> accepted for <inv>`, `cancel requested for <inv>`.

## Consumer Gates

`native_worker::index::stage_disposition(work_dir, stage_id)` is stage-wide (it ignores the session id, so a worker from an earlier attempt still counts) and returns `None`, `Active(units)` or `Unverified(cause)`. `blocker(stage_id)` renders the refusal: `Active` names `loom subagents cancel`; an unproven-ownership cause (exactly `<inv-id>: processes not proven stopped`) names `loom subagents release --stage <s> --invocation <id>`; any other cause is a damaged record or stage directory (a stage-level cause starts `stage <id>:`) and the remedy is manual removal. `Unverified(String)` stays a string variant because `park.rs` matches it with `if let`.

Every gate calls it, and `--force` cannot bypass any of them: `commands/stage/native_gate.rs::ensure_no_native_workers` (called from `commit.rs`, `complete.rs` Knowledge branch, `complete_pipeline.rs`, `skip_retry.rs`, `orchestrator/skip.rs`), `orchestrator/core/stage_spawn.rs`, `inbox_drain/commit.rs`, `host_verify/submit.rs` and `apply.rs`, and `loop_recovery/park.rs`. Monitors read it too: `monitor/hung_latch.rs` (`native_held`) and `status/data/heartbeat_facts.rs`. Waits bind a `codex:<unit>` selector to the native record when one exists (`wait/native.rs::outcome_for`: `Unverified` ownership is `Unknown`, a non-terminal record quiet past the stall budget is `Stalled` and exits 6, `Terminal` with ownership `Held` is still `Active`). Token attribution reads `index::stage_threads` (`tokens/discover.rs`, `commands/usage/forward_join_native.rs`), which carries each thread's `unit_id` and manifest effort onto its codex `AgentFlow`. Status reads the index twice: `status/data/execution_models.rs` lists `model/effort (requested)` per thread, and `status/data/codex_jobs.rs` turns `index::stage_jobs` (`native_worker/index_jobs.rs`) into the stage's `codex_jobs`, one entry per invocation including those that never started a thread. The web stage modal (`web/src/lib/codex-jobs.ts`, `stage-token-lanes.tsx`) and the TUI tokens pane (`ledger/tokens_jobs.rs`) render each job by unit id with its outcome and its duration against the deadline; a job without a thread gets a row with no tokens.

## Session Grants

`sandbox/settings/policy.rs::codex_granted` is `implementers.includes_codex() && backend == Legacy`, so a native-backend session gets no `~/.codex` write grant, no OpenAI domains and no readable `auth.json` ([Where a Session's Write Grants Come From](security-and-isolation.md#where-a-sessions-write-grants-come-from)).

## Testing Limits

Acceptance runs in a confined host job with no network and no `~/.codex`, so every test uses a scripted peer. Inside a sandboxed subagent `UnixListener::bind` fails with `EPERM` (proxy server tests) and bubblewrap cannot create a namespace, so `skip_unless(confined_commands_runnable())` boundary tests skip there and about 58 unguarded `verify::criteria` confine tests fail regardless of code; the `loom check` host job is the judge. Runtime contract helpers live at `native_worker::runtime_contract_tests::testkit` through a `#[path]` attribute; `native_worker/mod.rs` declares no `testkit` module. Seed test invocations with `sha256_hex(b"brief")` (`store::create_invocation` validates the digest against the brief bytes).

## Tracing Native Workers

`nativeBackend` in `web/src/components/stage-modal.tsx:104` means `session_backend === 'native'`, a native terminal window; it has no relation to a native Codex worker (`loom/src/native_worker`). Search `native_worker`, `codex.backend` and `WorkerControl` when tracing the lane.
