# Pressure review: autonomous recovery

The plan needs revision before execution. Two ownership/interface gaps block implementation; the remaining findings concern recovery that can stop the daemon, lose a landing action, or accept verification that currently fails. Findings are anchored to stage IDs and symbols. The plan was re-read immediately before this report; the findings below remain present.

Source paths below are repository-relative. This was a source and plan review; no build, tests, or live restart was run. Only this report was written.

## Blocking execution

### 1. Completion edits target CLI wrappers instead of the shared implementation

**Anchors:** `check-routes` W1; `gate-waivers` W2; `stall-recovery` W2; sibling `live-test-lane` / `live-checks` R2.

`run_checks`, `run_after_checks`, and `aggregated_wiring` now live in `loom/src/verify/completion/mod.rs:64`, `:144`, and `:272`. Aggregated reachable verification lives in `loom/src/verify/completion/complete_verification_v2.rs:98`. The proposed `commands/stage/complete_verification.rs` edit target is a wrapper that calls the shared implementation (`loom/src/commands/stage/complete_verification.rs:14`); its v2 companion re-exports `run_v2` (`loom/src/commands/stage/complete_verification_v2.rs:3`). This is committed code, including commit `1a4d22e1`, not an assumed future refactor. Host completion imports the shared implementation directly (`loom/src/host_verify/child_complete.rs:25`).

Neither `check-routes.files` nor `gate-waivers.files` owns `src/verify/completion/**`. Adding helpers to the old wrappers can satisfy the helper contracts while leaving actual host completion unchanged. The pinned wiring in the old v2 wrapper also encourages the wrong placement. The upstream live plan has the same gap: its `live-checks` R2 worker and stage YAML assign the live sink to the CLI wrapper, not the shared module. A `block_refusal` existence check does not establish that the live gate runs in host completion.

**Change:** make amendment of the sibling's `live-checks` YAML, wiring, and R2 brief an explicit blocking dependency. Assign the real shared files to this plan's W1/W2, update public export paths, contracts, filters, and wiring, and preserve CLI wrappers as adapters. Prove that a host completion executes both the live gate and waiver/address handling. Do not rely on a sibling overview promising that wiring.

### 2. Hook repair promises writes its stage cannot own and lacks the repair-budget input

**Anchors:** `spawn-recovery` public surface and W2; G5 atomic replacement/two-build rows; `w2-preflight-repair.md` / “Two hazards the repair must close.”

The brief correctly requires atomic replacement, but the installer it mandates calls `fs::write` directly (`loom/src/fs/permissions/hooks.rs:72-90`). `src/fs/permissions/hooks.rs` is absent from both the W2 worker territory and `spawn-recovery.files`. The stage therefore cannot implement the promised same-directory rename at the shared owner.

The same brief requires a persistent `<work_dir>/hook-repairs.json` budget. Its pinned `require_confined_host_repairing(facts, daemon_current, replaced)` receives no work directory. `HostFacts` has only writable roots, hooks directory, binary, hook PATH, Python, and Python hook paths (`loom/src/sandbox/config/preflight.rs:102-119`); the public surface explicitly retains those fields. None identifies the authoritative work directory. Inferring it from the daemon cwd or writable roots is not an equivalent contract.

**Change:** add the installer and focused installer tests to W2 and stage ownership; declare their artifact/behavior proof. Pass an explicit work directory or repair-state context into the pinned API and all contracts. Add a contract that performs a first repair, reconstructs the caller from disk, and refuses a second drift within ten minutes. Atomicity must be tested through the real installer, not a replacement helper only used by tests.

## Silently broken behavior

### 3. The restart delay authorizes a stale daemon to overwrite new hooks

**Anchors:** Decisions 13–14; `spawn-recovery` W1/W2; `daemon_binary_replaced`; `require_confined_host_repairing`.

W1 defines `daemon_binary_replaced()` to return `None` until a replacement fingerprint has remained stable for ten seconds across two checks. W2 passes `replaced.is_none()` as `daemon_current`. Immediately after installation, a spawn therefore treats the old daemon as current and reinstalls its embedded hooks over the new ones. Missing or still-changing replacements have the same ambiguity. This directly contradicts the G5 promise that a replaced binary never triggers hook reinstall.

The existing owner records the running binary path in `loom/src/orchestrator/terminal/native/launch/host.rs:179-192`; the shared installer overwrites differing hooks at `loom/src/fs/permissions/hooks.rs:75-83`. The error is in the proposed composition of those owners, not in either helper's isolated behavior. The current contracts supply `daemon_current=false` by hand and cannot catch it.

**Change:** separate immediate binary-current/stale detection from restart readiness/debouncing. Once the fingerprint differs, every spawn must refuse repair immediately; only execution of the successor waits for stability. Test the actual preflight call path at the first observation, before ten seconds, and after stability.

### 4. The Land job explicitly weakens the existing detector failure policy

**Anchors:** `stall-recovery` W2 / “Interface (pinned)” / `land::run_child`.

The brief requires `run_checks` with `fail_closed: false`. Current daemon-owned resolved landing explicitly uses `true`; only the operator-shell variant uses `false` (`loom/src/verify/completion/resolved.rs:17-35`, `:54-66`). Ordinary host completion also sets `fail_closed: true` (`loom/src/host_verify/child_complete.rs:142-149`). The field controls whether a detector that cannot run fails completion (`loom/src/verify/completion/mod.rs:29-31`).

Moving verification off the scheduler must not turn a source-graph/detector failure into a warning followed by an automatic merge. The existing Land contracts cover manufactured result records, so they would pass with this regression.

**Change:** pin `fail_closed: true` for Land jobs, preserve the operator-only exception, and add a child-path contract where a detector cannot execute: the Land result must fail and the target ref must remain unchanged. Use the shared resolved-verification owner identified in finding 1.

### 5. Land application lacks durable delivery and a consistent retry state machine

**Anchors:** Decision 15; G7 restart/branch-moved rows; `stall-recovery` W2; `Applied::Landable` / `Applied::LandFailed`.

The plan makes `apply_finished` return an action for `tick_host_jobs` to perform, but current application is not a callback channel:

- `apply_finished` writes the `applied` marker before returning its value (`loom/src/host_verify/apply.rs:75-95`).
- `apply_record`/`try_apply` return no action and merely log `Applied` (`loom/src/host_verify/apply_retry.rs:74-117`).
- The scheduler calls those wrappers both before polling new results and for newly ended jobs (`loom/src/orchestrator/core/host_jobs.rs:43-57`). Startup also applies pending results (`:117-130`).

Without changing that shared delivery path, the new actions are discarded. Merely returning them from a wrapper still leaves a crash window between the durable `applied` marker and landing/review. A passing job can be reverified unnecessarily after restart; a failed job can lose its review transition and be submitted again. The worker table omits `apply_retry.rs`, although the stage's broad `host_verify/**` scope permits assigning it.

The failure policy is also contradictory. A moved tip is specified as `LandFailed` and “verified again,” but W2 routes every `LandFailed` to human review. Restart converts a running job to `Interrupted` (`loom/src/host_verify/signals.rs:124-145`), and initialization applies terminal jobs before spawning replacement work (`loom/src/orchestrator/core/run.rs:60-84`). Treating that interruption as an ordinary failure prevents the promised resubmission while the stage remains in merge resolution.

**Change:** specify one durable, idempotent Land state machine covering result application, landing acknowledgement, retryable interruption/stale-head outcomes, and terminal verification failure. Assign `apply_retry.rs` and startup recovery explicitly. Do not acknowledge a scheduler side effect before it is durably recoverable. Add scheduler-level contracts for normal success/failure, restart after result application but before landing, interrupted jobs, and moved heads. The current submit/apply helper contracts and `rg Applied::Landable` gate do not prove these behaviors.

### 6. A bounded lock error can terminate the scheduler instead of retrying next tick

**Anchors:** Decision 16; G7 lock-holder row; `stall-recovery` W1 / bounded lock callers.

The brief says existing `Result` callers already log a timeout and retry next tick. Several propagate it out of the scheduler. For example, `apply_retry_if_due` returns the stage update error (`loom/src/orchestrator/core/recovery_sync.rs:66-75`), `sync_authoritative_stages` propagates it (`:24`), and `run_tick` propagates the sync error (`loom/src/orchestrator/core/run.rs:97-98`). `run_until_stopped` propagates `run_tick` (`:43`); `run` stops host jobs on that exit (`:20-24`). The daemon supervisor records scheduler failure and sets shutdown (`loom/src/daemon/server/orchestrator.rs:60-82`).

Thus replacing an indefinite wait with a 30-second error can turn temporary contention into daemon termination, before the 900-second watchdog helps. `stage-lock-wait-is-bounded` tests only the primitive's elapsed time.

**Change:** define a typed lock-timeout outcome and explicitly handle it at safe scheduler boundaries, preserving unfinished work without continuing dependent operations on stale state. Assign affected callers or the shared tick boundary to W1. Add a contract showing a lock timeout leaves the daemon running and the operation succeeds on a later tick after release.

### 7. The takeover path can hang before reaching its bounded joins

**Anchors:** Decision 14; `spawn-recovery` W1 / “Flow”; `stall-recovery` W3.

W1 relies on the existing five-second thread joins before the predecessor exits. However, `run_server` first calls `client_pool.shutdown()` (`loom/src/daemon/server/lifecycle.rs:188-194`), which unconditionally joins every running client worker (`loom/src/daemon/server/pool.rs:50-61`). That join has no deadline. A handler already inside `dispatch.serve` cannot observe the shutdown flag until it returns (`loom/src/daemon/server/client.rs:257-266`). The later bounded orchestrator join cannot resolve a handler stuck on state the scheduler holds.

The successor's specified 60-second lock deadline can expire while the predecessor has stopped accepting clients but still owns the lock. This is different from the acknowledged residual risk of a replacement binary crashing after takeover. Neither restart stage owns `daemon/server/pool.rs`.

**Change:** settle the restart-specific client-drain policy and its deadline before implementation, add the required pool/handler ownership, and keep the singleton held until all abandoned in-process writers are gone. Test a successor that reaches `WAITING_BYTE` while a client handler remains blocked; assert bounded termination/takeover or an explicit return to serving, rather than only testing a successor that exits before the handshake.

## Under-specified contracts and verification

### 8. The block guard cannot construct exact addresses from its stated input

**Anchors:** Decision 9; `gate-waivers` W3 / `dispute_routes`; `block-refused-on-failing-goal-backward`.

W3 is told to use the current checkpoint's failing labels to return one exact command per `[list n]` or `[owner/list n]` gap. A checkpoint criterion stores only `id` and `passed` (`loom/src/handoff/schema/completion.rs:35-39`); `VerificationCheckpoint` adds environment data, not gap addresses (`:48-54`). The current reader extracts only IDs (`loom/src/stage_service/control_block.rs:73-87`). The live sibling's promised `latest_failing_labels -> Vec<String>` does not supply the missing owner/index information either.

Detailed outcomes are a separate channel: `check_outcome` puts error text in a bounded job-result summary (`loom/src/host_verify/child_complete.rs:167-177`). The gate-judge brief already specifies reading a job through the checkpoint's `evidence_nonce`, but the guard brief does not pin that join, validation, truncation handling, or behavior for completion records without such a job. Its contract fixture names only a failing `goal-backward` label, so an unusable placeholder command can pass.

**Change:** define the authoritative source and representation of route addresses, with current-session/job binding and missing/truncated-evidence behavior. Either persist structured addresses or explicitly specify the validated job-result reader and its limits. Test multiple gaps with distinct owners and indices, the actual production handler `handle_agent_block_for_session`, and a record that has a known label but no recoverable address.

### 9. Applying an impact waiver before the sink destroys the raw observation

**Anchors:** `gate-waivers` W2 / steps 1–2; `checked_or_waived`; `observe_impact`.

W2 requires wrapping the impact sink's argument with `checked_or_waived`, while also requiring the existing host child to keep observing the raw failure with no edit. These requirements conflict. The child calls `observe_impact` on the result received by the sink (`loom/src/host_verify/child_complete.rs:153-155`). An `Ok(())` resets the observation to the empty default (`:73-83`). Converting `Err` to `Ok` upstream therefore erases the observed failing tests and replaces the structured outcome with a passing summary.

This matters after a persistent impact waiver: a later commit can produce real named test failures. The plan explicitly keeps waiver decisions auditable, but this ordering discards the raw evidence its own brief promises to preserve. The helper-only `waived-gate-passes-completion` contract cannot detect it.

**Change:** preserve raw gate outcomes separately from the effective waived decision, and apply the waiver after observation (or pass both values through a typed outcome). Assign `host_verify/child_complete.rs` if needed and update stage dependencies/ownership. Add a contract with a waived impact gate returning named failures and another failing gate: the completion policy may honor the waiver, but the observed failures and waiver audit must both survive.

### 10. Implementation acceptance omits the canonical gate and key composition proofs

**Anchors:** all five implementation stages; `integration-verify`; `artifacts` and `contracts` coverage.

`CONTRIBUTING.md:73-89` defines the full gate: format, all-target Clippy, rustdoc with warnings denied, audit, all-target tests with `--no-fail-fast`, flake-check, and Markdown lint. Each implementation stage instead runs selected test filters plus build/Clippy/format. Full Rust tests and rustdoc appear only in integration; audit and flake-check are absent there too. The changes touch locks, process lifecycle, shared enums, and completion, so a compiled but unexecuted consumer is material. In particular, the proposed filters do not exercise the actual `verify::completion` owner from finding 1.

The behavioral claims also exceed the pinned proofs: `waived-gate-passes-completion` calls a helper; the Land contracts submit/apply records without driving the scheduler; the lock contract proves only the timeout primitive; the watchdog contract proves only age arithmetic. Each can pass with the broken compositions described above. The artifact lists omit promised implementation owners such as the audit-section module and shared completion changes; file existence/wiring strings alone cannot close the behavioral gaps.

**Change:** put the full canonical gate in every code stage with adequate timeouts and a concrete plan for its confined environment, rather than silently substituting module filters. Map each promised capability to its owning artifact and executable behavior proof. Keep the helper tests, but add the composition contracts specified in findings 1 and 3–9; integration review should confirm settled behavior, not choose the lifecycle design.

## Cross-plan ownership inventory

The live plan's executable YAML does explicitly describe the confinement split, facility API, Live job, block-refusal signature, and merge live-receipt route. These are planned upstream surfaces, not present-code facts. Its completion-owner amendment in finding 1 remains blocking.

Other same-directory overlaps need explicit scheduling/preservation rules in “Run order and sibling plans”:

| Sibling and stage | Shared ownership/seam | Required plan treatment |
| --- | --- | --- |
| `PLAN-live-test-lane.md`: `confinement-hardening`, `live-gate-and-boundary`, `live-profile`, `live-checks` | Criteria, CLI, host jobs, dispute enums, completion, doctrine | Existing serial ordering is appropriate; amend the shared completion owner and its executable gate before relying on it. |
| `PLAN-model-router-hooks.md`: `router-core`, `wiring`, `integration-verify` | `loom/src/cli/types.rs`, `loom/src/cli/dispatch.rs`; `loom/src/fs/permissions/hooks.rs` once finding 2 is fixed; broad `loom-hooks/**` | Serialize the shared writers; preserve Router registration and hook installation when adding takeover/atomic replacement. |
| `PLAN-web-host-graft-followthrough.md`: `implement-host-and-context`, `integration-verify` | `loom/src/cli/**`, including dispatch refactoring | Specify ordering and rediscover `dispatch_run` in the merged module; a frozen `dispatch.rs` assumption is insufficient. |
| `PLAN-loom-ui-mods.md`: `loom-ui` | `src/cli/dispatch*.rs`; its installer uses the same asset system | Serialize dispatch edits and preserve its registration. Its new plugin namespace is disjoint from recovery modules. |
| `IN_PROGRESS-PLAN-planning-trace.md`: `trace-server` | `skills/loom-plan-writer/SKILL.md` | Preserve its doctrine additions; include this active sibling in ordering, rather than treating all planner work as web/server-only. |

These overlaps are grounded in the siblings' stage `files`/worker tables, not their overview promises. Current source owners include `loom/src/fs/permissions/hooks.rs:47`, `loom/src/orchestrator/terminal/native/launch/host.rs:187`, and the completion owners cited above. The router, UI, web-host, and trace plans do not build the missing recovery seams for this plan. The trace-screen redesign has no relevant Rust recovery ownership.

The declared crate-relative working directories and existing acceptance output paths are consistent with in-worktree builds; no outside-worktree `allow_write` fix is indicated. No third-party API or package change is required by the reviewed work. Remaining work is to repair the ownership, interfaces, lifecycle rules, and proofs above.

