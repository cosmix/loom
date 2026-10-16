# Loom bug, duplication, and architecture sweep — 2026-10-15

Reviewed source revision: `d273a4386dcc6098d6a15cd83e934476b079a1bc`. The historical findings below describe that revision; original source line references may have moved. All five required native macOS Rust gates pass after the host-diagnosis and strict-helper fixture repairs: libtest reports 8,509 passes across all 47 targets, zero failures, seven explicit ignores. Six Linux-only runtime guards are included in the pass count; their Linux bodies did not execute. Doctests separately pass three executed and three compile-only examples with seven existing ignores. See the [verification continuation](astra-verification-2026-10-16.md) for exact commands, source boundaries, failed strict attempt, and target accounting, and the [macOS host report](macos-test-host-2026-10-16.md) for diagnosis and metrics. The repair handoff and historical chronology below remain unchanged. The unrelated untracked `doc/RETRIEVAL-AB-METHOD.md` remains untouched.

Final gate commands from `loom/`: `cargo test --all-targets --no-fail-fast -- --test-threads=4`; `cargo test --doc --workspace --all-features -- --test-threads=1`; `cargo fmt --all -- --check`; `cargo clippy --all-targets -- -D warnings`; `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps`. All exited 0; the latest bounded all-target run took 220.07 s. Local `main` contains the earlier reviewed fixes through `cdf26982`; latest verification also covers the strict-helper test repair described in the continuation. No push occurred.

## Historical repair handoff — 2026-10-16

**At this handoff, all 13 original outcomes passed; full final-tree verification remained incomplete.** The follow-up audits all 35 findings and implements the remaining B09 resolver verification plus missing-proof guards across automatic merge, blocked retry, finalization and restart recovery. Passing subsets do not certify the complete suite.

This follow-up rechecked the 11 failed and two unreported outcomes, audited all 35 repair claims, corrected diagnosed failures, and invoked full tests after implementation. The repairs are committed in three implementation groups, with this report and knowledge updates in a fourth documentation group. The operator ended further diagnosis and directed committing the reviewed fixes. This report distinguishes source-audited implementation from executed verification. The unrelated `doc/RETRIEVAL-AB-METHOD.md` remains excluded.

### Repair inventory

| Findings | Implementation and verification state |
| --- | --- |
| B01–B04, B06, A01 | Locked authoritative runtime snapshots, capacity-preserving launch dispositions, durable cause-aware blocks, scheduler supervision/readiness, strict stage loading and empty-output propagation. Library coverage plus focused recovery rechecks; no remaining concrete failure reported in this group. |
| B05, A06 | Ordered prepared/applied amendment journal, atomic audit publication, recovery after partial writes, preservation of newer policy, consolidated stage/session transition rules. Fault and exhaustive transition tests included in the library run. |
| B07–B08, B23, A02 | Shared assertion evaluator, typed complete-output evidence, fail-closed analyzer health/truncation and Go no-test handling. Both previously unreported zero-test cases pass. Fixtures use installed-shell builtins; Go text/JSON empty/mixed-package coverage preserves unknown versus zero counts. Integrity review strengthens live tests to require exit 0, no timeout and the specific semantic rejection; capture-cap coverage also requires incomplete capture and its marker. |
| B09–B11 | Exact assigned checkout/branch, committed-input checks, full policy/amendment identity and locked snapshots. Resolver relay, CLI and exit run the shared completion verifier before replacing proof; landing pins that commit. Automatic merge refuses missing proof. Integrity review restores the resolved-success Completed assertion and requires failed acceptance to preserve target HEAD and original proof with the precise acceptance error. Operator, relay, mutation and missing-proof regressions cover the other paths. |
| B13–B16, A04 | Exact streamed Git bytes, capped retained source with full hashes, graph schema 3, extractor revision 5, graph-backed duplicate/wiring checks and repository-relative nested-package coordinates. Source/detector regressions passed in the library run. |
| B12, B18, A03 | Shared owned-process retirement for generic runner, criterion executor and quota; bounded pipe cleanup and direct-forwarding lifecycle ownership. All 14 quota tests, including the nine previous failures, pass in the focused run. Installed-shell fixtures preserve deadlines and descendant-retirement assertions. |
| A05, A07, A08 | Shared bounded JSONL spool, stable envelopes, pre-effect dispute reservations and atomic stage receipts; shared session inventory, neutral stage service, daemon-state and default-policy helpers. Replay fault regressions and library coverage included. |
| B17, A11 | Nullable direct-exec turn identity, shared bounded hook input and event handling. Copied fixtures now include `_bounded.sh` and both Python skill-trigger installers include `_bounded_input.py`. Host hooks pass 102/102, syntax 159/159 and partial-staging checks 6/6; final Rust integration verification is blocked before harness startup. |
| B19, A10 | Verified release before candidate execution, shared minisign policy, atomic staged binary replacement and rollback. Committed; installer suite 9/9 passes. Release workflow/updater end-to-end execution is not claimed. |
| B20, A12 | Neutral provider inventory/normalization, latest whole-observation replacement/retraction, nested cache counts and checkpoint-v3 rescan. Live/offline/status library regressions passed. |
| B21–B22 | Settings refetch after overlapping writes and lifecycle-safe bootstrap/WebSocket status. Committed; dashboard check passes 646 tests across 59 files and production bundle builds. |
| A09 | Fresh-host Seatbelt/bwrap, private Cargo cache and authenticated inherited macOS confinement. The host canary now uses installed Bash builtins with identical unconfined positive controls for sibling read/write and TCP connection; denied operations and allowed worktree writes remain mandatory. Linux execution remains outside this Mac's coverage. |

### B09: resolved content must earn fresh completion proof

Normal landing from an initialized stage store, including MergeConflict resolution, requires the branch tip to equal `completed_commit`. The shared resolver verifier checks registered worktree/branch and original ancestry, captures content and policy, runs acceptance and the complete verification pipeline, then validates current state under the stage lock before pinning the resolved commit. CLI `--resolved`, authenticated relay and resolver exit use it; clean resolved worktrees can also enter ordinary operator completion. Missing proof no longer acquires authority from automatic HEAD capture.

Completion snapshots bracket verification and bind full stage gates/contracts, source/config/freeze bytes and monotonic amendment revision. Generated exemptions are `.loom/cache/`, `target/`, `loom/target/`, `web/node_modules/`, exact Claude settings and three spool paths. State/CLAUDE exemptions require symlinks. Declared file/artifact/contract/wiring/regression/harness/ratchet inputs override exemptions. Locked validation avoids nested stage locks; broker/knowledge completion records `completed_commit` explicitly.

New operator CLI tests cover successful resolved landing, failed current acceptance and concurrent policy mutation; relay and automatic-exit tests cover both verification outcomes and missing proof. Existing producer/broker checks remain separate from operator landing; a universal producer→broker→landing fixture is not claimed. Pre/post snapshots cannot detect transient mutate-and-restore or arbitrary generated-dependency consumption. Resolver gates run synchronously in their trusted caller. Explicit authenticated operator skip commands remain deliberate overrides; recording their commit does not certify that tests ran. Immutable execution and universal scheduler-latency bounds are not claimed.

### A09: actual confinement and platform limits

Fresh host commands use Seatbelt on macOS or bubblewrap on Linux with restricted runtime/toolchain reads, assigned worktree writes and denied network. Shared Cargo registry/git sources are read-only; private Cargo home and scratch resources remain owned through process retirement. macOS runtime allowances include root-directory metadata, selected Xcode Contents and timezone data; these do not grant arbitrary descendant writes. Provisioning prepares the sandbox at the assigned worktree root and changes to the validated entry directory inside it.

macOS inheritance requires positive kernel sandbox and protected-state write-denial checks, authenticated daemon socket PID/start time, live session ancestry and reciprocal stage ownership, canonical worktree containment and a protected enabled capsule. Environment/token/settings claims alone grant nothing. Inheritance retains the approved capsule permissions, including any broader cache/network grants; it does not impose the stricter fresh-host policy. New capsules grant the exact daemon socket; **old running sessions require relaunch**. Tests use actual Seatbelt/socket peers, not a full Claude runtime launch. Linux bubblewrap was not executed on this Mac.

### Verification evidence and outstanding coverage

- Dashboard: `bun run check` passed typecheck, lint, formatting and **646 tests across 59 files**; production build and both dashboard/terminal smoke scripts passed. Installer: `python3 scripts/tests/test_install.py` passed **9 tests**. Host hooks passed **102/102**, syntax **159/159** and partial-staging checks **6/6**.
- Before the integrity audit, static Rust checks passed: `cargo fmt --check`, all-target Clippy with warnings denied and all-features rustdoc with warnings denied. Formatting also passes on the integrity-audited source. The maintainability ledger removes four obsolete allowances; none grows. Its eight checks passed the completed 47-target run. The separate `cargo test --doc --workspace --all-features -- --test-threads=1` attempt reached `Doc-tests loom`, then rustdoc stalled before harness entry (sampled at `_dyld_start+0`). Its 300-second watchdog retired the process tree with exit 124 after 301.65 seconds. Executed, compile-only and ignored doctest counts remain unreported.
- Historical broad library runs reported **7,675 passed / 129 failed / 1 ignored**, then **7,778 passed / 25 failed / 1 ignored / 2 without outcomes**. Both were incomplete, terminated and reaped after executable startup stalls. Historical focused evidence included 47 completion/merge/recovery/Bun cases plus a separate 1/1 negative-proof recheck, provisioning 15/15 and completion replay 17/17; these do not certify the follow-up tree.

**Original unresolved outcomes (13):** nine quota cases (`leader_exit_with_stdout_inheritor_without_reply_is_bounded`, `leader_exit_with_stdout_inheritor_and_reply_is_bounded`, `a_child_that_closes_stdin_before_the_request_still_yields_its_reply`, `a_flood_of_notifications_after_the_reply_does_not_hang_the_reader_join`, `a_json_rpc_error_reply_surfaces_the_message`, `a_hanging_server_times_out_and_the_child_is_killed`, `a_successful_exchange_parses_both_windows_and_the_plan`, `garbage_and_an_over_long_line_are_skipped_before_the_reply`, `the_child_exiting_without_ever_replying_is_reported_precisely`); `actual_host_launcher_denies_sibling_read_write_and_network`; `resolved_nested_executable_change_misses_after_neighboring_hit`; and unreported `zero_test_criterion_fails_in_v2` / `zero_test_criterion_passes_in_v1`. All 13 passed an initial 37-test focused run; a subsequent broad run reproduced the canary and executable-cache startup timeouts, prompting further fixture corrections. All 13 also passed the serial library run (7,811 passed, 0 failed, 1 ignored), and their exact passing outcomes were independently confirmed in the recovered-host run (7,809 passed, 2 failed, 1 ignored). Both separate failures also pass in the final integrity-audited library run (7,811 passed, 0 failed, 1 ignored).

The follow-up broad library run reported **7,787 passed, 23 failed, 1 ignored** before the next executable stalled; it was stopped and reaped. Twenty failures exposed legacy merge fixtures relying on fabricated/stale proof or non-runnable acceptance, one exposed a stale socket inherited by concurrent forks, and two were startup timeouts. Corrections preserve negative assertions and use explicit verified fixture commits, a bind-only stale socket, installed-shell canaries and shell fallback for the real executable-byte cache fixture.

The first complete verifier run passed **7,809 library tests, with 1 ignored**, but failed **171 integration tests**: `codex_evidence` 21, `integration` 140, `worker_evidence` 10. Their copied hook trees omitted `_bounded.sh`; ten fixture installers now include it. Additional proof-path fixes followed that run. An intermediate run found four fixture prerequisite mistakes (missing proof, repeated create-only stage writes and an empty recovery graph), all corrected without changing their intended assertions.

The completed 47-target run reported **8,479 passed, 27 failed, 6 ignored**. Twenty-five skill-trigger failures exposed the missing Python helper; the other failures were an offline-Cargo timeout and formatter startup failure. Both host checks pass in the later serial library run (**7,810 passed, 1 failed, 1 ignored**), whose remaining heartbeat failure assumed elapsed time within one clock tick. Its timestamps are now explicit. The serial run was stopped and reaped after a copied guard stalled before its body; that helper now invokes installed Bash explicitly. The final full serial invocation compiled successfully, waited before harness startup, then passed the library (**7,811 passed, 0 failed, 1 ignored**) and main (**5 passed**) before its 40-minute watchdog expired while `adjudication_e2e` waited to start. It was reaped with exit 124. The continuation launcher and process diagnostics also stalled. After host execution recovered and temporary state was cleared, a fresh all-target run reported **7,809 library tests passed, 2 failed, 1 ignored**. `verify::contracts::format_gate::tests::a_formatted_file_gives_no_problem` failed after another sampled formatter startup delay; `verify::criteria::confine::host::tests::actual_host_launcher_builds_cargo_offline_with_private_cache_state` timed out after 60 seconds with empty output, without a contemporaneous loader sample. Both failures are superseded by concrete passing outcomes in the subsequent integrity-audited library run (7,811 passed, 0 failed, 1 ignored, 133.76 seconds), including the strengthened Go and resolver assertions. Full-target coverage remains separate. A separate Codex-evidence target was reaped before harness entry; a separate integration attempt timed out after 240 seconds without any test starting. The full run then stalled before the main harness (sampled at `_dyld_start+0`) and its 30-minute watchdog reaped the process tree with exit 124 after 1,801.56 seconds. Doctests also timed out before harness entry. The integrity-audited full rerun passed the library but remained before the main harness; the independent doctest gate also did not enter its harness. The operator then ended further diagnosis and directed committing the reviewed fixes. The doctest processes were retired; the all-target and final Clippy sessions were no longer available at handback. The remaining 46 all-target binaries, doctest counts and latest Clippy outcome remain unreported. A byte-identical main executable at a fresh temporary inode also stalled before listing tests. The host execution-policy service showed over five CPU cores of activity; its root cause remains unproven. No further host diagnosis is requested. No completed final-suite pass is claimed.

### Commits and knowledge

Prior repair commits include `22701b16` (web), `f1e4265d` (installer), `08a738e1` (hooks), `56f4d32d` (core) and `46cde0cd` (knowledge/report). Follow-up repairs are committed as `f94aa14a` (`fix(merge): enforce completion proof across landing paths`), `483c2e93` (`test(runtime): stabilize process and confinement fixtures`) and `bc5adc8b` (`test(hooks): include shared helpers in copied fixtures`). All three commits passed the normal hooks, including formatting and all eight maintainability tests; warnings-denied rustdoc passed where triggered. Earlier hook startup stalls were retired, and automatic approval review rejected a proposed hook override. No override was used. The integrity audit is complete and its two assertion gaps are corrected.

The documentation group includes this report and all task-owned knowledge changes, including `INDEX.md`. Final knowledge replacement and sync completed; `loom knowledge check` reported **0 issues, 335 review notes**. Earlier stalled CLI attempts were retired. Review notes do not establish corpus-wide semantic freshness.

The follow-up audit traced every B01–B23/A01–A12 repair against current producers and consumers. The additional concrete omissions in B09 resolver proof and missing-proof manufacture through automatic merge, blocked retry, landing finalization and restart recovery are implemented. Broad architectural directions, such as eliminating every derived runtime view, are not represented as universally complete. Implementation inventory does not replace the verification outcomes above.

Verification logs under `/private/tmp/loom-final-*` and `/private/tmp/loom-sweep-*` are temporary. The final report records material outcomes so continuation does not depend on those files.

## Original review — main results at the reviewed revision

The report contains **35 findings: 23 behavior/resource defects and 12 duplication/architecture findings**. Some architectural entries explain the same underlying mechanisms as individual defects; they are repair groupings, not 12 additional independent exploits. Most findings are newly identified in this sweep; A09, A11, and A12 are known risks reconfirmed in current code, and B21 is a previously documented possibility now shown reachable.

The highest-priority correction is to make verification certify the exact code and policy that will land. The present implementation can test unstaged fixes while merging the unfixed commit (B09), accept a failed dead-code analyzer (B07), accept negative assertions over truncated output (B08), and retain evidence across a contract amendment (B10). Runtime priority follows: held-stage starvation (B01), intentional blocks undone at restart (B02), missing scheduler supervision (B03), skipped malformed stages (B04), and incomplete amendments reported applied (B05).

The clearest external-contract defect is the direct Codex lane's mandatory missing `turn_id` (B17). The strongest executed repository-level reproduction is the source graph's wrong committed-byte cache and resulting false stale-window error (B15). The most consequential duplication lies in assertion evaluators, process retirement, source analysis, provider normalization, and transactional spool/install behavior. Fixes in one copy are already absent from another.

## Scope, method, and reading guide

The review used a team divided across runtime/state, verification/Git/sandbox, web/hooks/accounting/installation, and retrieval/source graphs. Independent follow-up reviews challenged the most consequential findings. Reviewers started with the knowledge index, used source-graph navigation, traced callers and consumers, inspected relevant tests, and compared current code with documented concerns. This is a broad, risk-directed source audit, not a claim that every line was manually reviewed or every execution environment was tested.

The inventory contains 1,934 Rust files and 396,100 lines under `loom/src`, including embedded tests; 201 files/32,403 lines under `loom/tests`; 160 files/19,746 lines under `web/src`; and 146 script files/22,558 lines under `loom-hooks`. These counts cover `.rs`, `.sh`, `.py`, `.ts`, and `.tsx`, as applicable. The published graph covered 3,965 files, with 60% symbol-level file coverage and 53% symbol-level byte coverage; shell and other lexical-only files needed direct inspection. Neither the graph nor a textual clone count proves semantic completeness.

**Severity:** **CRITICAL** means a concrete operational defect; **SILENT_FAILURE** means success or healthy status can conceal missing/incorrect work; **MAJOR** means consequential architectural debt, an advisory-check defect, or a bounded correctness problem. Priority **P1** calls for prompt correction; **P2** follows after the P1 fixes. A known, deliberately accepted trust boundary is identified as such rather than presented as a newly discovered exploit. Confidence describes evidence, not a claim that a full test suite passed.

Unless an entry explicitly says **reproduced**, its evidence is source tracing and its regression scenario is a proposed test. Source locations use repository-relative paths and the reviewed revision. “New” means not found in the knowledge topics consulted, not a claim that no previous discussion exists anywhere.

## Runtime, persistence, and scheduling defects

### B01 — A held stage can starve unrelated runnable stages

**P1 · CRITICAL · New · High confidence.** `loom/src/orchestrator/core/stage_executor.rs:101-140` calculates available slots and splits the ready list before attempting launches. `start_stage` returns `Ok(())` without launching for a held stage at `:175-181`; `core/target_hold.rs:258-269` supplies that disposition. `loom/src/plan/graph/mod.rs:121-141` gives the list deterministic dependency-depth/ID ordering.

With parallelism one, independent queued stages `a` and `b`, and `a` held, every tick selects only `a`. It declines to launch, while `b` remains outside the selected slice. The daemon has unused capacity indefinitely, and its scheduling report can describe `b` as concurrency-limited with zero running sessions. Multiple early held stages generalize the failure; an early knowledge stage subject to a target hold can have the same effect.

**Fix and regression:** scan ready candidates until actual capacity is consumed. Return an explicit launch/adopt/defer outcome, and assign concurrency-limit diagnoses only after capacity is exhausted. Exercise the complete scheduler with one held earlier-ID stage and one runnable later-ID stage, then with several held stages and a target-held knowledge stage. The runnable stage must start without requiring the hold to be removed.

### B02 — Restart recovery can undo an intentional block

**P1 · CRITICAL · New · High confidence.** Startup calls orphan recovery before normal monitor/block retirement (`loom/src/orchestrator/core/run.rs:45-72`). Recovery admits current dead sessions for `Executing`, `NeedsHandoff`, and `Blocked` without distinguishing the block cause (`core/recovery.rs:866-934`). `core/orphan_adoption.rs:104-135` then queues or hands off the stage and replaces its reason. In contrast, `core/event_handler/blocked.rs:12-47` explicitly treats `Blocked` with no `failure_info` as an intentional agent/operator block or rejected review.

The trigger is a block persisted while the daemon is unavailable, or a daemon exit after the block write but before the assigned session is retired. Once the dead session is beyond the orphan grace period, restart can requeue a stage with no commits ahead, or change it to handoff when work exists. The original prerequisite/reason is lost, and work can restart without the requested human intervention. The later block handler cannot restore a status startup has already changed.

**Fix and regression:** share a cause-aware recovery disposition with block handling. Retire stale session ownership while preserving intentional `Blocked` status and reason; apply retry budgets/backoff to crash blocks. Restart fixtures should cover an intentional block, rejected human review, zero/positive commits ahead, and a crash block with an exhausted retry budget.

### B03 — Scheduler initialization failure leaves an apparently healthy daemon

**P1 · SILENT_FAILURE · New · High confidence.** The outer thread wrapper only logs an early `run_orchestrator` error (`loom/src/daemon/server/orchestrator.rs:26-38`). Graph/configuration/constructor failures at `:48-88` occur before the shutdown/completion path at `:98-126`. `daemon/server/lifecycle.rs:167-186` announces readiness before starting that thread; the accept loop at `:186-219` watches the shutdown flag rather than worker completion.

A cyclic persisted graph or another initialization failure after readiness can therefore terminate the scheduler while leaving the socket/server alive. Earlier CLI preflight catches some configuration failures; those are outside this finding. Status and ping can still succeed, but no stage progresses, no scheduler restart occurs, and the normal completion marker is absent. The error is logged, so the failure is not diagnostically invisible. Socket readiness currently proves less than the operator is likely to infer from it.

**Fix and regression:** supervise every return and panic at the outer worker boundary. Publish an explicit scheduler failure and shut down or enter a deliberate retry state. Announce full readiness after scheduler initialization, or expose separate server/scheduler readiness. Inject graph-build and constructor failures and assert the daemon cannot remain indefinitely healthy without its essential worker.

### B04 — Malformed stage records can disappear from final success accounting

**P1 · SILENT_FAILURE · New · High confidence.** `loom/src/fs/stage_loading.rs:120-131` warns and skips malformed records. `loom/src/plan/graph/loader.rs:65-75` accepts a nonempty surviving list as authoritative. Monitor enumeration also skips parse errors (`loom/src/verify/transitions/persistence.rs:224-234`), as do recovery paths. Final accounting iterates graph nodes (`core/run_result.rs:24-46`), and the daemon prints success from that result (`daemon/server/orchestrator.rs:100-116`).

Before restart, corrupt the frontmatter of an independent or leaf stage while another stage remains valid. The omitted stage never becomes a graph member, so completing the surviving stages can produce “All stages completed successfully.” A surviving dependency on the omitted node can instead produce a wait. This concerns skipped parse/ID errors; filesystem read errors propagate, and an entirely empty surviving list takes a different fallback path. Work-directory integrity validation checks layout, not completeness of stage contents. A plan-file finalization check may refuse a final rename; that does not repair the false success report or missing scheduler coverage.

**Fix and regression:** fail graph construction on malformed expected state, or carry a completeness-error ledger that prevents successful completion. Compare the loaded IDs with the initialized plan manifest. Test valid completed `a` plus malformed independent `b`, malformed dependencies, invalid IDs, and repair while running. The existing unreadable-stage result test starts with the node already in the graph and misses this omission path.

### B05 — An amendment audit row can certify writes that never completed

**P1 · SILENT_FAILURE · New · High confidence.** `loom/src/plan/amendment.rs:641-649` appends an audit row before replacing the live plan and updating the stage. On retry, a matching stage/dispute audit row returns success immediately at `:451-465`. `orchestrator/core/verdict_apply.rs:53-68` retries an application error on later ticks without restarting. Adjudication then resynchronizes whatever policy is currently on disk, requeues the stage, and can write `applied.marker` (`orchestrator/adjudication/apply.rs:141-225,245-261`).

If the audit append succeeds but either following write fails transiently, the next attempt treats “recorded intent” as “committed operation.” An accepted criterion change is reported applied while the stage still has its old criterion. Startup recovery at `plan/amendment.rs:745-819` can help only after a restart and considers the latest audited snapshot; another successful amendment can make the incomplete operation cease to be the latest one.

**Fix and regression:** distinguish prepared/audited from applied, and finish the exact recorded snapshot under the amendment lock before returning idempotent success. Do not replay index-based insert/delete patches blindly. Inject failures after audit append and after live-plan replacement, retry without restarting, and require both plan and stage to match before an applied marker appears. Include an intervening amendment for another stage.

### B06 — Removing the final output does not remove it from downstream briefs

**P2 · SILENT_FAILURE · New · High confidence.** The public output-removal command persists an empty vector (`loom/src/commands/stage/output.rs:107-118`), but graph synchronization replaces outputs only when the new vector is nonempty (`orchestrator/core/recovery.rs:362-366`). Dependency status copies the stale graph vector (`core/stage_executor.rs:570-591`) into the signal (`core/stage_spawn.rs:202`, `signals/format/helpers.rs:60-66`).

Have the daemon observe `endpoint=old` on upstream stage `a`, remove its last output, then release/start dependent stage `b`. Direct output listing is empty, but `b` still receives the deleted endpoint until daemon restart. This can preserve obsolete paths or decisions after the operator explicitly removed them.

**Fix and regression:** synchronize empty vectors too, or construct dependency outputs from the authoritative record. `ExecutionGraph::set_node_outputs` already supports empty vectors (`plan/graph/mod.rs:331-339`). Test nonempty-to-empty synchronization and verify the generated downstream brief, not only the output command's persisted record.

## Verification and completion defects

### B07 — Dead-code verification passes when its analyzer fails or times out

**P1 · SILENT_FAILURE · New · High confidence.** `loom/src/verify/goal_backward/dead_code.rs:32-86` runs the command and searches retained lines for fail patterns, but does not incorporate exit status, success, or timeout into its result. The returned gaps feed ordinary completion verification through `goal_backward/mod.rs:60` and `commands/stage/complete_verification.rs:57`.

For a fail pattern `unused`, `command: "exit 1"` returns no matching warning and therefore no gap. Missing tools, compilation failure before analysis, and timeout without the expected warning can also pass. This certifies absence of dead code when no successful analysis occurred; it is a routine infrastructure-failure path.

**Fix and regression:** require successful analyzer execution by default and model tool-specific diagnostic exit codes explicitly. Timeout, signal death, incomplete output, and unavailable analysis must not become passes. Cover success/no warning, warning, exit 1, exit 127, signal exit, and timeout at both the analyzer helper and completion entry point.

### B08 — The first acceptance run can pass on incomplete output

**P1 · SILENT_FAILURE · New · High confidence.** Capture retains the first 10 MiB per stream and discards the rest (`loom/src/verify/criteria/executor.rs:179`). `cache_contract.rs:125-149` computes `passed` without requiring its separate `output_complete` flag. `runner.rs:142` uses that pass for the live result. Cache publication subsequently rejects incomplete evidence at `cache_contract.rs:224`, but that does not revoke the original pass.

An exit-zero criterion can emit over the cap and place a forbidden marker only in the discarded suffix. `stdout_not_contains` then passes. If all count-bearing test output is beyond the cap, zero-test evidence can also disappear before `zero_tests.rs:27` parses it; absent/unrecognized summary is not treated like an explicit zero. Losing only Cargo's final footer is insufficient if its earlier `running N tests` line survived. Reader failure/timeout markers create another incomplete-evidence path. The cache is stricter than the live gate it is meant to accelerate.

**Fix and regression:** stream-evaluate predicates and test counts over all output while retaining bounded diagnostics, or make incomplete required evidence an explicit non-pass. Negative assertions cannot be proved by an incomplete prefix. Test a forbidden suffix and zero-test summary just beyond the cap on the first execution, plus capture failure. Assert the live verdict, not merely cache eligibility.

### B09 — Completion can verify unstaged fixes and merge the unfixed commit

**P1 · SILENT_FAILURE · New · High confidence; integration reproduction recommended.** Acceptance and verification use the live worktree (`loom/src/commands/stage/complete.rs:434-453,641-649`). Completion evidence binds `HEAD` through `handoff/completion/identity.rs:34`, while the daemon's added dirty-index check uses only `git diff --cached --quiet HEAD` (`daemon/server/completion_evidence.rs:151,185-192`). Merge resolves the stage branch's committed tip (`git/merge/mod.rs:139`).

Commit code that fails its criterion, then leave an unstaged tracked fix that makes it pass. Keep the index equal to HEAD. The checks observe the fix, but the attested revision and landed branch still contain the failing code. A v2 review of the live fixed tree can satisfy the observer fingerprint without closing this mismatch. Untracked inputs can cause the same divergence. A dirty-worktree cleanup refusal happens after landing and cannot make the landed commit verified.

**Fix and regression:** verify an immutable tree and bind both review and merge to that tree. At minimum reject relevant tracked worktree differences and untracked inputs before verification and evidence acceptance, with explicit treatment of generated scaffolding. Extend `daemon/server/control_complete_tests.rs:191`, whose existing case stages its change, with an unstaged fix, an untracked required input, and mutation after verification. Completion must refuse until the verified content is the content that will land.

### B10 — Contract changes are absent from completion evidence's definition hash

**P1 · SILENT_FAILURE · New · High confidence in omitted binding.** `loom/src/handoff/completion/identity.rs:13-26` hashes selected verification fields but omits `stage.contracts`. Contracts are amendable (`plan/amendment.rs:66`, `plan/amendment_fields.rs:230-235`). The daemon checks that incomplete hash at `daemon/server/completion_evidence.rs:143`; its observer rechecks integrity/review, rather than rerunning contracts (`daemon/server/observer.rs:78-88`). Contracts run earlier in `commands/stage/complete_verification_v2.rs:31-34`.

Completion loads its Stage snapshot before potentially long checks (`complete.rs:325`). An operator can amend contracts during that run (`commands/stage/amend.rs:29-67`): the amendment does not reject Executing status, restart the session, or invalidate pending evidence. Completion checks its old snapshot, then the daemon reloads the changed stage but accepts the same incomplete definition hash. A verified-pending-ack checkpoint offers a related window. This is a concurrent-amendment/recovery defect, not arbitrary replay: nonce, session, status, HEAD, and other bindings still apply. Other omitted gate-affecting fields deserve the same projection audit without assuming every omission has an equally reachable bypass.

**Fix and regression:** define one typed, versioned projection of the complete verification policy and hash it at both producer and verifier. Include contracts and all effective gate inputs, and invalidate pending checkpoints on relevant amendments. Extend the acceptance-only hash test at `handoff/completion/shared_tests.rs:75` with a contracts-only change, then test daemon refusal of the old checkpoint without changing HEAD.

### B11 — A missing assigned worktree can fall back to another stage's checkout

**P1 · SILENT_FAILURE · New · High confidence in helper; operator-path integration test recommended.** `loom/src/commands/stage/acceptance_runner.rs:104-140` drops the expected worktree if the directory is absent, then accepts any current Loom worktree in the `(Some(cwd_root), None)` branch at `:131`. Its stage-ID comparison runs only when both locations exist. Completion uses this directory for checks, but merges the requested stage branch. A normal operator shell without session environment does not use the broker/HMAC path (`control_session.rs:148-150`).

Keep stage A's branch and executing record but remove its assigned worktree. From stage B's worktree, invoke operator completion for A. The fallback allows A's commands to run against B while merge still targets A. The v1/local path is the clearest case; v2 has additional observer checks that can reject some variants. The helper also serves contract/review operations, so identity belongs in the helper rather than only one caller.

**Fix and regression:** require a matching stage identity for every selected checkout, including fallback paths. Missing assigned worktrees should fail with a recovery instruction rather than silently substitute another stage. Use two real worktrees, remove only A's directory, and assert completion from B rejects before running a marker command or merging A.

### B12 — Verification timeouts allow additional execution while output is collected

**P2 · MAJOR · New · High confidence.** `loom/src/verify/criteria/executor.rs:88-108` waits for the process deadline, records duration, and collects stdout/stderr before killing a timed-out process. `OutputReaders::collect` at `:148-150` waits sequentially; each stream may wait ten seconds (`:168-171`). Both waits occur while the child can still be alive and holding its pipes.

A command that exceeds its configured timeout can continue running and changing files for approximately another twenty seconds. Reported duration is captured before that delay. A successful direct shell exit with a descendant retaining output pipes is another cleanup case the implementation should specify; it was not promoted here into a separately reproduced process-leak finding.

**Fix and regression:** terminate/reap the owned process group immediately on timeout, then collect output under one shared cleanup deadline. Measure duration after cleanup. A disposable child holding both output descriptors should finish near the configured bound and leave no owned child running. Do not solve this with another independent process supervisor; see A03.

### B13 — The duplicate detector uses the wrong path root for nested packages

**P2 · SILENT_FAILURE · New · High confidence.** `loom/src/commands/stage/complete_verification.rs:162-166` passes `acceptance_dir` to duplicate detection despite retaining a separate worktree root. `verify/duplicate_detection.rs:222-245` asks Git for repository-relative changed paths, then `:180-184` joins them to the supplied directory and silently skips unreadable files.

With `working_dir: loom` and changed file `loom/src/new_helper.rs`, the detector attempts `<worktree>/loom/loom/src/new_helper.rs`. Its ordinary read failure becomes “nothing to check.” This project's nested Rust package layout is a direct example of the affected shape. The detector is advisory: this is lost diagnostic coverage, not a bypass of a mandatory acceptance proof. The unwired-file check receives the same acceptance directory (`complete_verification.rs:115-119`) and has related coordinate/search-scope inconsistencies; those should be tested independently rather than assumed identical.

**Fix and regression:** use the worktree root for repository-wide diffs and source scans; reserve the package directory for package commands. Alternatively request explicitly relative Git output and enforce one coordinate system throughout. Test root and nested-package invocation against the same newly introduced duplicate, and surface unexplained failures to read a changed regular file.

### B14 — Changing the original helper's file hides a newly copied helper

**P2 · MAJOR · New · High confidence.** Every lookup receives the entire changed-file list (`loom/src/verify/duplicate_detection.rs:197`), and `find_symbol_in_codebase` excludes all those files at `:361-364`, not merely the definition currently under examination.

Add a copy of `normalize_settings` in `new.rs` and make any unrelated edit to `old.rs`, where the original resides. Both definitions are excluded from the other's lookup. Two new files with the same helper are also mutually invisible. The detector therefore misses exactly the edit-and-copy pattern this audit was asked to find. This detector is advisory; its failure is not equivalent to the false acceptance gates above.

**Fix and regression:** compare each candidate with every other definition except itself, emitting one result per unordered pair. If new versus inherited definitions matter, compare symbol/body identities with a merge-base graph instead of excluding whole files. Test one unchanged original, both files changed, and both files added.

## Retrieval and source-graph defects

### B15 — A dirty first build corrupts the cached representation of committed source

**P1 · CRITICAL · New · Reproduced in an isolated repository.** Dirty tracked files in a base build are read with `git show HEAD:<path>` through `run_git_checked` (`loom/src/context/refresh/source_graph/layer.rs:236-252`). That helper applies lossy UTF-8 decoding and `.trim()` (`loom/src/git/runner.rs:174-208`). A clean base reads raw bytes. Base reuse at `context/refresh/snapshot.rs:165-192` checks schema/parser currency, so the altered base can remain valid indefinitely by those checks.

**Observed:** commit `src.rs` containing two blank lines, `pub fn original() {}`, and a final newline; edit the function name; run `loom map --outline src.rs --json`; restore the exact committed bytes and query again. The cached clean outline reports the function on line **1**, although it is on line **3**. `loom map --window src.rs#function:original --json` exits **3** with `changed since snapshot: src.rs`. The installed CLI reproduced this and the reviewed source explains it. Even a usual final newline changes the stored content hash; leading whitespace additionally shifts spans.

**Fix and regression:** add/use a checked raw-byte Git API for blobs, keeping the trimmed text API only for textual scalar output. Preserve NUL-delimited path bytes where applicable. Invalidate affected cached bases when shipping the repair. Compare a cold clean build against a cold dirty build followed by exact restoration: full layer, content hash, spans, and readable windows must agree. Include leading/trailing whitespace and non-UTF-8 blob content.

### B16 — The source extraction cap is applied after unbounded reads

**P2 · MAJOR · New · High-confidence resource-boundary defect; no OOM attempted.** `loom/src/context/refresh/source_graph/generation.rs:41-63`, `refresh/source_graph/layer.rs:123-145,236-252`, and `context/worktree_graph.rs:309-315` read complete eligible files with `read_bounded(..., usize::MAX)`. `fs/safe_read.rs:48-80` preallocates from file size and reads to the end. Only afterward does `context/extract/mod.rs:311-324` enforce the 512 KiB extraction limit.

A large tracked or untracked artifact outside excluded roots can consume its full size in memory and be read/hashed again even though extraction ultimately records only `Oversized`. Working-tree freshness checks also run on retrieval reads, so large dirty files can impose recurring cost, not just a one-time index build. This is source-demonstrated exposure to memory pressure and latency; the audit did not deliberately exhaust memory.

**Fix and regression:** stream complete hashes through bounded buffers while retaining only capped source bytes. Represent oversized files with hash, size, and coverage without owning their whole body. Share that reader between refresh and verification worktree graphs. A sparse oversized-file fixture should assert bounded allocation, stable generation/hash, and explicit oversized coverage.

## Hooks, dashboard, accounting, installation, and adapter defects

### B17 — Direct Codex forwarding requires a field the installed exec protocol does not emit

**P1 · CRITICAL · New · Reproduced with the actual parser and version-pinned protocol.** `loom-hooks/_codex-direct.py:73-86` records `turn_id` only if a turn event provides it; `_normal_result` at `:309-324` requires it for success. `loom-hooks/codex-forward.sh:277-286` independently rejects a missing turn ID. Installed Codex is `codex-cli 0.162.1`; its [release-pinned exec event definitions](https://github.com/openai/codex/blob/092d3acd6bec3e3a14bdc7e7a2810ab628ab759d/codex-rs/exec/src/exec_events.rs#L42-L53) define an empty turn-started event, completion with usage, and failure with error, without a turn ID. The [official noninteractive interface](https://learn.chatgpt.com/docs/non-interactive-mode#make-output-machine-readable) describes this exec event stream.

The direct lane is selected when nested macOS Seatbelt is refused (`codex-forward.sh:114-119,242-258`). A normally successful exec run then cannot satisfy Loom's completion contract. Tests invent a `turn_id` (`loom-hooks/tests/codex-forward-direct.sh:34-41`, `codex-forward-wrapper.sh:240-246`), so they certify the wrong provider protocol. App-server/rollout identity appears to have been conflated with exec's output format.

**Observed and fix:** a local child emitted real-schema thread/start/completion events through the actual Python helper, producing `state:unknown`, `turn_id:null`, `exit_code:125`. Process inspection was restricted in that fixture, so a second probe used the actual `EventState`/`_normal_result` with a cleanly retired process double: the same rejection persisted with `ownership_retained:false`. No inference request was made. Use a transport that supplies authoritative turn identity, or define exec evidence around authorized invocation/thread/terminal events without inventing a provider turn ID. Replace synthetic protocol assumptions with version-pinned or captured fixtures for success, failure, and timeout.

### B18 — Codex quota teardown can hang after its immediate child exits

**P2 · CRITICAL · New · Source-confirmed with an OS pipe-lifetime probe.** `loom/src/quota/codex.rs:97-165,205-249` creates a process group and a blocking stdout reader. Teardown at `:298-314` kills the group only if the immediate child has not exited, then unconditionally joins the reader. Dropping the channel receiver does not interrupt `BufReader::fill_buf()` on an open pipe. The single poller services both providers (`quota/poller.rs:70-84`).

If an app-server executable/wrapper exits while a helper retains inherited stdout, teardown skips group kill and can wait forever for EOF. The nominal RPC deadline no longer bounds completion, and both providers stop updating. A disposable process-group fixture confirmed that leader exit can coexist with a live group and no pipe EOF; it explicitly killed and drained its child afterward. The audit did **not** reproduce this with the installed stock Codex process or execute Rust `poll_once`, so the helper/wrapper condition is material to likelihood.

**Fix and regression:** retire the owned group and bound reader completion even after leader exit. Reuse the generic runner's handling of this case (`loom/src/process/mod.rs:235-253`). Test parent exit with a stdout-inheriting helper for both reply and no-reply cases; assert bounded `poll_once` return and no surviving owned child.

### B19 — Installation removes the working executable before replacement is safe

**P2 · CRITICAL · New · High confidence; installer not executed.** Local installation unlinks `~/.local/bin/loom` before copying/chmodding its replacement (`install.sh:219-231`); remote installation unlinks before moving/chmodding (`:277-303`). Capability validation runs only after replacement (`:354-366`). A failed copy, full disk, permission failure, interruption, or wrong downloaded executable leaves no rollback path. `set -e` stops the script after the damage.

The existing Rust updater has staging/rollback machinery (`loom/src/commands/self_update/install.rs:11-62`), while shell installation independently implements weaker replacement. Avoiding ETXTBSY does require a new inode; it does not require removing the final pathname before a replacement is ready. There is also a window in which concurrent CLI invocation simply finds no executable.

**Fix and regression:** stage on the destination filesystem, finish writing and validate the candidate, set permissions, then atomically rename over the target with a retained rollback strategy. Inject copy/chmod/validation failures in a disposable installer fixture and require the old executable to remain usable. Signature verification is a separate requirement, covered in A10.

### B20 — Live and offline token normalization disagree on the same supported inputs

**P2 · MAJOR · New drift in duplicated parsers · High confidence; provider-production frequency not measured.** Live usage parses in `loom/src/tokens/claude_line.rs:21-66` and accumulates in `tokens/accumulate.rs:96-117,213-241`. Offline usage independently parses in `commands/usage/claude_usage.rs:6-40,74-89` and merges in `transcript_merge.rs:10-31`. Nested-only cache creation is explicitly supported by `commands/usage/transcript_tests.rs:273-279`: offline reconstructs the flat total from the 5m/1h split, while live usage treats the absent flat field as zero.

For input/output 5/2 and nested cache writes 4+6, offline counts 17 fresh tokens and live counts 7. The engines also differ on revisions: offline selects the latest whole vector, while live retains componentwise maxima. A single message revised from `(input,write,read,output)=(10,100,0,4)` to `(5,0,100,8)` becomes `(10,100,100,8)` live, a vector never present in either observation. That second example is a conditional input-contract divergence, not evidence that the live provider currently emits such corrections. Partial observations are handled differently too.

**Fix and regression:** share provider parsing and stream-revision semantics below both consumers. Retain separate live buckets/checkpoints and offline provenance/reporting. Cross-surface fixtures should agree on normalized totals for nested cache creation, partial rows, downward revisions, and reordered observations. This is a stronger consolidation target than coincidentally similar formatting helpers because the same source bytes already produce conflicting accounting.

### B21 — Cross-scope settings responses can overwrite newer displayed values

**P2 · CRITICAL · Reproduced reachable form of a previously dismissed risk.** Pending writes use `(scope,name)` keys from `web/src/components/settings-model.ts:262-272`, consumed by `settings-dialog.tsx:176-201`; controls disable only the relevant scope (`settings-control.tsx:230-280`). Every server response contains the complete entry, including both scopes (`loom/src/commands/status/web/config_api.rs:108-129`), and `settings-dialog.tsx:357-362` replaces that entire entry without a version check.

Write user scope and delay its response after persistence. While that cell is pending, write project scope; it remains enabled. Apply the newer project response first, then the old user response. The latter restores the old project value **in the UI** even though both disk writes succeeded. A probe executing the actual `useSettingsWrites` logic with deferred responses observed `new-project` followed by `old-project`. Existing per-cell guards do not protect two scopes of the same key. This is not a disk configuration rollback.

**Fix and regression:** serialize writes per key across scopes, use server entry versions to reject stale snapshots, or reliably refetch after concurrent mutations. A client invocation counter alone does not establish server application order. Exercise actual independently enabled cells, resolve responses in reverse order, and require displayed user/project/effective values to match persisted state.

### B22 — Delayed HTTP bootstrap can mark a disconnected status feed live

**P2 · CRITICAL · New · Reproduced with the actual socket class and transport doubles.** `web/src/api/ws.ts:56-71` races an HTTP status fetch with WebSocket connection. Shared `receiveSnapshot` at `:119-149` marks the connection live for either source; the outstanding HTTP continuation has no cancellation/generation guard. The timestamp check in `web/src/state/apply.ts:10-22` protects data contents, not transport phase.

Open then close the socket while HTTP bootstrap remains pending: phase becomes `reconnecting`. Resolve the old HTTP request: phase becomes `live` despite no live WebSocket. The actual transpiled class produced exactly that sequence. A later successful reconnect can correct it, but a slow/hung handshake extends the false indication. A late response after disposal is another unguarded continuation. No snapshot-content rollback is claimed.

**Fix and regression:** separate “data available” from “live transport.” Only the active WebSocket should establish live health. Cancel or generation-guard bootstrap requests and all post-disposal continuations. Test delayed bootstrap after disconnect, reconnect generation change, and disposal, including an old snapshot rejected by the timestamp gate.

### B23 — Go packages with no test files evade the explicit zero-test guard

**P2 · SILENT_FAILURE · New · Real Go output reproduced; Loom parser path source-traced.** The Go adapter's summary parser does not recognize `[no test files]` (`loom/src/testrun/adapters/go_test.rs:62-63,132-145`). A successful empty-package invocation therefore yields no parsed test count rather than zero, and the zero-test guard permits unknown counts. This is distinct from B08: the output can be small and complete.

An isolated temporary Go module, with network/toolchain downloads disabled, returned exit zero and `?\texample.invalid/astra/empty\t[no test files]` for `go test ./...`. Such a command cannot demonstrate a contract test executed. The audit ran the Go command and inspected the adapter/guard; it did not execute a full Loom completion for the fixture.

**Fix and regression:** recognize the no-test package result and aggregate package evidence explicitly. All recognized packages reporting no tests must produce a known zero, while mixed tested/no-test packages should preserve the actual execution evidence. Feed captured `go test` text and JSON variants through the adapter and the contract/acceptance gate, asserting that exit zero with no tests does not certify success.

## Unnecessary duplication and architectural issues

### A01 — Lifecycle authority is distributed across mutable copies

**P1 architectural repair direction · MAJOR.** Persistent stages, `StageNode` copies, `active_sessions`, `verified_merged`, `merge_retry_attempted`, and monitor history independently influence scheduling and completion. B01, B02, B04, and B06 show the resulting policy/synchronization gaps. `loom/src/orchestrator/core/recovery.rs` is 1,455 lines; `sync_graph_with_stage_files` spans `:294-795` and mixes reads, status changes, merge evidence, retries, outputs, persistence, and merge execution. `plan/amendment.rs` is 1,031 lines and its main application function is 226 lines.

Make scheduling a pure decision over a complete authoritative snapshot; apply guarded lifecycle changes through a shared service with explicit causes, including recovery. Derived views should be disposable rather than another place that decides policy. Fix the concrete bugs first, then separate these responsibilities along transaction and ownership boundaries. Splitting files solely to meet the line cap would not remove the coupling. The inventory found 41 Rust files over 400 lines, including tests; size alone is not counted as a bug.

### A02 — Three assertion engines implement the same rules differently

**P2 · MAJOR · Demonstrated duplication and semantic drift.** `loom/src/verify/goal_backward/truths.rs:18-134`, `goal_backward/wiring_tests.rs:21-131`, and `criteria/cache_contract.rs:125-168` plus `criterion_eval.rs:80-140` independently implement timeout, exit status, and positive/negative output assertions. Wiring adds stderr assertions, but that does not require a second execution/verdict algorithm.

Truth/wiring convert a missing exit code into `-1` (`truths.rs:44-46`, `wiring_tests.rs:49-51`); acceptance correctly requires `Some(expected_exit)`. A signal-killed command can therefore match an explicitly configured `-1` in the former engines. The acceptance regression at `criterion_eval.rs:149` does not protect the other surfaces. The incomplete-output defect in B08 also spans these duplicated loops. Extract one pure evaluator over a typed execution result and common assertion specification, with per-surface presentation adapters. Run the same case table through every entry point, including signals and incomplete capture.

### A03 — Multiple process supervisors carry different cleanup guarantees

**P2 · MAJOR · Demonstrated duplication.** The generic bounded process runner (`loom/src/process/mod.rs`), verification executor (`verify/criteria/executor.rs`), Codex quota launcher (`quota/codex.rs`), and direct-forward Python supervisor (`loom-hooks/_codex-direct.py`) all own some combination of deadlines, stdout/stderr drainage, process groups, termination, and result classification. Different protocols justify different decoders, not four unrelated definitions of process completion.

B12 and the quota teardown finding show fixes failing to propagate between supervisors. Establish a documented ownership contract: direct-child exit, descendant policy, pipe deadline, group termination, reaping, and partial output. Reuse the Rust process primitive where possible, with protocol streaming as an adapter. Keep the Python boundary where required, but test it against the same lifecycle fixtures. Simply sharing a timeout constant would not address the behavioral duplication.

### A04 — The duplicate and wiring guards maintain a second source-analysis system

**P2 · MAJOR · Concrete drift.** `loom/src/verify/duplicate_detection.rs:100-152` keeps Rust/TypeScript/Python/Go regex extractors alongside `context/extract` and the source graph. Its Rust pattern misses private and restricted-visibility helpers such as `pub(crate) fn`. Its Go producer recognizes `func`, but the lookup builder at `:391` searches `(fn|def|function)`, so it cannot find the corresponding Go definition. `wiring_detection.rs:288-303` separately approximates imports with textual matching that can count comments/unrelated names.

Use the existing graph for candidate definitions/references, retaining explicit degraded/uncertain results where graph support is incomplete. Text fallbacks should be small, language-specific adapters with their limitations exposed. Do not claim semantic equivalence solely from matching names: overloads, trait implementations, and independent domain methods are legitimate. Tests should cover supported-language parity, graph-unavailable behavior, restricted Rust visibility, Go functions, and same-name unrelated methods. B13/B14 are additional defects in the independent detector's path and comparison logic.

### A05 — JSONL spool algorithms are copied across domains

**P2 · MAJOR · Demonstrated duplication; conditional replay risk.** Compare `loom/src/fs/memory/spool.rs` append/read/drain at `:62-169` with `fs/stage_request/spool.rs:69-199`: mkdir/open, exclusive/shared locking, size checks, JSON serialization, line parsing, sink invocation, and final truncation are separate implementations. The request writer adds a per-record serialized-size guard absent from the memory writer. Both read/drain paths use unbounded `read_to_string`; their append limits constrain only cooperating writers.

Domain payload validation differs legitimately. The storage and delivery mechanics should have one implementation with explicit semantics. `StageRequest` has no stable request ID (`fs/stage_request/types.rs:20-42`); if an early sink succeeds and a later sink/truncate fails, retry can replay the successful effect. Filing a dispute allocates a new ID/increments its count (`daemon/server/dispute.rs:69-82`). This is a fault-path reliability concern, not a claimed ordinary no-fault reproducer. Share bounded locked JSONL storage and add per-request identity/checkpointing where effects are non-idempotent. Test partial sink failure and retry with exactly one effect for the first request.

### A06 — State-transition policy is represented twice per model

**P2 · MAJOR · Duplication without a proven current non-self-edge mismatch.** `loom/src/models/stage/transitions.rs:28-100` and `:118-174` separately enumerate the 13-state policy in `can_transition_to` and `valid_transitions`; session transitions repeat the pattern at `models/session/transitions.rs:23-51,69-87`. No production caller of `valid_transitions()` was found in this sweep; tests can therefore validate a second representation rather than the predicate enforcing real transitions.

Derive the predicate and list from one outgoing-edge table, or remove the unused list API and test the real transitions. Define no-op self transitions explicitly: the predicate accepts them globally, while the list generally omits them and includes `NeedsAdjudication` specially. Exhaustive state-pair tests should exercise the enforcing API. Similar function names alone are not the finding; these bodies independently encode the same adjacency policy.

### A07 — Cleanup reimplements session discovery and parsing

**P2 · MAJOR · Smaller consolidation opportunity.** `loom/src/fs/session_files.rs:219-259` scans/reads/parses session Markdown, selects `stage_id`, and extracts IDs. `fs/worktree_files/sessions.rs:20-98` repeats that work before deletion. Discovery silently skips read failures; cleanup records warnings. Both discard directory-entry errors and malformed records through their own paths.

Provide a common enumeration of validated `(path, identity)` records plus diagnostics. Discovery can project IDs, and cleanup can delete selected paths; keep destructive work out of the shared reader. Test malformed, unreadable, duplicate, and vanished records with consistent interpretation. This deserves lower priority than the assertion, source-analysis, and spool duplication because no additional destructive bug was established here.

### A08 — Lower layers import orchestration and transport policy

**P2 · MAJOR · Dependency direction, not a standalone runtime bug.** `loom/src/fs/stage_request/apply.rs:53-112` calls daemon RPC handlers and constructs daemon responses; `models/stage/methods.rs` imports the orchestrator heartbeat timeout constant; `fs/tmux_tmpdir.rs` imports daemon-level state. These edges make filesystem/model APIs depend on the consumers they should support.

Move request application into a shared service below RPC and spool transports; keep request DTOs in a neutral model module. Put default timeout policy in config/model code. This creates one place to test authorization, replay, and application semantics and reduces the incentive to create another handler for each transport. Avoid treating every cross-module import as a violation: the concern is shared domain behavior owned by one outward-facing adapter.

### A09 — Host-side “Confined” verification does not enforce a filesystem sandbox

**Known architectural trust-boundary gap · MAJOR; P1 where untrusted generated code is executed.** `loom/src/verify/criteria/confine.rs:138-180` launches an ordinary host process. The `Confined` branch applies a restricted environment at `:147-148`, then configures working directory, process group, and pipes; it does not install an OS filesystem/network sandbox. Daemon before-stage execution reaches this launcher (`orchestrator/core/stage_executor.rs:538`, `verify/before_after.rs:27-32`), as can operator completion.

Repository-controlled tests/build hooks can therefore execute with the host user's filesystem access when a host verifier runs them. Sandboxed stage completion inherits its session's OS sandbox, and the daemon's completion RPC validates evidence rather than rerunning all tests; this finding is not an escape from that ordinary path. Daemon before-stage checks also skip detected prior work. The remaining host-execution boundary is documented in existing execution-containment/agent-rule-bending concerns and was not exploited in this audit. Reuse the actual containment capsule for host execution, or define a deliberate reviewed-immutable-input boundary. A disposable sibling-file read/write canary must test the actual launcher; environment-unit tests cannot establish filesystem confinement.

### A10 — Bootstrap installation bypasses self-update's signature policy

**P2 security architecture · MAJOR · New in this review; no substituted executable run.** `loom/src/commands/self_update/mod.rs:254-295` downloads and verifies a `.minisig` against the embedded key through `self_update/signature.rs:9-32`; the release workflow publishes/checks signatures (`.github/workflows/release.yml:175-190,264-267`). `install.sh:244-303,359-366` instead accepts a successful download and installs/executes it without verifying the signature. Platform/asset inventories are also maintained in shell, Rust, and workflow tables.

This loses a signing boundary already enforced by updates. Exploitation requires a substituted release/download response despite a trusted installer; it is not a claim that arbitrary network observers can defeat TLS, or that a signature protects an already compromised installer script. Share a release identity/signature/key contract between bootstrap and update, verify before execution, and test invalid signatures without replacing/running the candidate. Having the new executable verify itself would be too late. B19 separately covers replacement atomicity even for trusted binaries.

### A11 — Hook stdin deadlines are repeatedly reimplemented with an unbounded fallback

**P2 · MAJOR · Known and reconfirmed.** `loom-hooks/read-guard.sh:29-37`, `codex-forward-guard.sh:46-54`, `stage-terminal-guard.sh:41-49`, and `plans-path-guard.sh:28-36`, among others, each select `gtimeout`, then `timeout`, then bare `cat`. Stock macOS may have neither GNU command; a caller holding stdin open can then block the hook indefinitely. `_read_ledger.sh:59-77` already contains a shell-watchdog bounded runner, but the copies do not use it.

Move bounded execution/input collection into a common helper and retain each guard's explicit refusal policy. Test PATH without GNU timeout tools and an intentionally open input pipe, requiring bounded completion. Do not blindly merge strict forwarding-argv parsing with permissive command analysis: `_codex_forward.sh::parse_shell_words` and `_common.sh::loom_tokenize_command` have materially different acceptance contracts.

### A12 — Core token discovery depends on CLI/status internals

**P2 · MAJOR · Known and reconfirmed.** `loom/src/tokens/discover.rs:5-9` imports session inventory from `commands::status::data`, subagent ledger/resolve helpers, and Codex discovery from `commands::usage`. Status in turn consumes token flow (`commands/status/data/mod.rs:87`, `collector.rs:373-375`). The dependency cycle is `tokens -> commands::status -> tokens`; reusable provider/session discovery lives in presentation-facing namespaces.

Move inventory and provider event/discovery contracts below both commands and tracker. Let CLI reports and live tracking retain their own aggregates, but interpret provider bytes and discover sessions in one layer. B20 is an existing semantic consequence of parallel interpretation. This is an incremental extraction, not an argument for a universal accounting object containing every consumer's state.

## Recommended repair sequence

| Order | Work package                                                           | Findings                    | Required proof before considering it closed                                                                                                                                                   |
| ----- | ---------------------------------------------------------------------- | --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1     | Bind verification and landing to an immutable tree and complete policy | B09-B11                     | Unstaged/untracked fixes cannot certify a different commit; concurrent contract amendment invalidates proof; another checkout cannot satisfy the request.                                     |
| 2     | Make verification evidence fail explicitly when incomplete             | B07-B08, B23, A02           | Failed analyzers, missing test execution, truncated negative-assertion output, signals, and timeouts yield consistent non-pass outcomes across all gate surfaces.                             |
| 3     | Repair direct Codex protocol integration                               | B17                         | Version-pinned exec events without turn IDs complete successfully through both Python and shell layers with real ownership checks.                                                            |
| 4     | Repair scheduler/recovery authority                                    | B01-B06, A01                | Holds leave capacity available, blocks survive restart, malformed state prevents false success, worker failure is supervised, partial amendments finish, and empty outputs propagate.         |
| 5     | Restore exact source-graph bytes and bound indexing resources          | B15-B16                     | Dirty/clean cold builds agree byte-for-byte and source windows work; oversized files do not require full-body allocation.                                                                     |
| 6     | Consolidate process and storage transaction mechanics                  | B12, B18-B19, A03, A05, A10 | Deadline/group/pipe fixtures are shared, failed installation preserves the old binary, signatures are checked, and partial spool delivery cannot replay non-idempotent effects.               |
| 7     | Repair guards and consumer consistency                                 | B13-B14, B20-B22, A04, A11  | Nested-package duplicate fixtures work; both changed copies are compared; token interpretation agrees; late UI responses cannot falsify state; hook reads remain bounded without GNU timeout. |
| 8     | Extract shared domain services and eliminate secondary policy tables   | A06-A08, A12                | One transition policy and one session/provider inventory serve the real consumers, with explicit diagnostic behavior.                                                                         |

A09 needs an explicit host-execution trust decision in parallel with the first work package if unreviewed generated code is run by the operator/daemon. Architectural changes should preserve the existing safe ownership/identity checks. A replacement “universal” service is useful only if the current consumers actually route through it and the duplicated implementations are retired.

## Executed checks and evidence limits

| Check                                                                   | Executed result                                                                            | What it establishes                                                                                                               |
| ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| Isolated Git repository plus installed `loom map`                       | Dirty first build, exact restoration, outline at line 1 instead of 3; source window exit 3 | B15 is observable in the installed CLI and explained by the reviewed source. The installed binary was not rebuilt for this audit. |
| Actual Python direct-forward event/parser logic with real-schema events | `unknown`, code 125, null turn ID; isolated clean-retirement case also rejects             | B17's protocol rejection is independent of the fixture's process-inspection restriction. No paid/live Codex run.                  |
| Disposable process group with stdout-inheriting child                   | Parent exited; group lived; no EOF until explicit group cleanup                            | The OS precondition for B18. The Rust quota function and stock Codex descendant behavior were not executed.                       |
| Actual settings write-hook logic with deferred responses                | New project value replaced by old displayed value                                          | B21's response-order effect. UI enablement/server response shape were source-checked; no real settings write.                     |
| Actual WebSocket class with deterministic transport/store doubles       | `reconnecting` changed to `live` after delayed HTTP                                        | B22's transport-phase bug. No browser/server end-to-end session.                                                                  |
| Temporary empty Go module, downloads disabled                           | `go test ./...` exit 0 with `[no test files]`                                              | The runner output missing from B23's parser. Downstream Loom false-pass behavior is source-traced.                                |
| Knowledge integrity check                                               | Exit 0; 0 issues, 328 review notes                                                         | Corrected knowledge remains structurally valid; the review notes are not a certification of semantic freshness.                   |

No full Cargo build, repository test suite, linter, browser suite, live-provider execution, installer run, production daemon mutation, or end-to-end merge/amendment race test was performed. Regression scenarios in the findings are work to add while fixing the bugs, not tests represented as already passing or failing. Concurrency and transaction findings were independently challenged against their guards and callers.

## Coverage and rejected candidates

| Area                 | Depth reached                                                                                                                                                 | Material limits                                                                      |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| Runtime/state        | Startup/ticks, graph readiness/loading, block/orphan recovery, verdict/amendment persistence, output propagation, session/spool ownership, worker supervision | Platform terminal implementations and every recovery interleaving were not executed. |
| Verification/Git     | Acceptance/goal/v2 gates, identity/evidence, review fingerprints, commit/landing boundaries, merge lock and cleanup safeguards, duplicate/wiring scans        | No destructive merge test, host confinement canary, or complete adapter matrix.      |
| Hooks/web/accounting | Direct forwarding, selected guards, quota lifecycle, access/config/terminal admission, settings/socket state, token normalization/discovery, install/update   | Not every hook/component/platform branch; no production provider samples collected.  |
| Retrieval/knowledge  | Snapshot/base/overlay identity, file reads, extraction boundaries, delivery suppression, packing seams, knowledge mutation locking                            | No exhaustive extractor/resolver language corpus or ranking benchmark.               |
| Follow-up challenge  | All six primary runtime findings, all ten original verification candidates, five principal surface findings; sampled owned waits and lifecycle folding        | No additional issue promoted solely to increase coverage counts.                     |

Several plausible candidates were rejected. `plan/graph/mod.rs` readiness/topological-sort methods delegate to `scheduling.rs`; the daemon graph builder delegates to the shared loader. They are wrappers, not duplicate algorithms. Extractor adapters legitimately share boilerplate. Knowledge content mutations already use locked read-modify-write. Delivery suppression also keys on item body hash, so an overlay-epoch concern did not establish stale-content suppression. Terminal lifecycle records and owned waits have evidence checks that rule out several simplistic stale-success theories.

Current merge locking uses a stable-inode OS lock; the old stale-file-reclamation race is not present. Quota cache/history readers reject FIFO/symlink/nonregular input through safe reads. Human-review approval resets the contract budget. Remote dashboard viewing is deliberately public; config mutations and terminal routes retain their authorization checks. Snapshot timestamps already prevent old HTTP contents replacing newer status data. These were not reported as current bugs. Shared graph-rendering glyphs and trivial row-addition snippets were not promoted as consequential duplication.

A normalized 12-line clone scan found 12 distinct file-set candidates in production Rust text, excluding test paths and content after the first `cfg(test)`. It is a deliberately limited search aid: it misses renamed/restructured copies and can flag valid adapters. Every duplication finding above identifies actual independent bodies and either demonstrated drift or a specific maintenance/replay risk; no total percentage of “duplicate code” is claimed.

## Repository changes made for this review

The deliverable is this report. Production code, tests, dependencies, and operational configuration remain unchanged. Repository instructions also required immediate correction of proven stale knowledge: the review updated claims about dashboard read authorization, loop order, amendment recovery, merge locking, human-review contract budgets, embedded hook installation, quota safe reads/teardown, provider ledger fields, owned-wait job lookup, settings response reachability, and confinement type locations. The knowledge CLI regenerated `INDEX.md`. Small reviewer navigation/CLI mistakes were recorded in the existing mistakes topics as required. These documentation corrections are not implementation fixes for the findings above.
