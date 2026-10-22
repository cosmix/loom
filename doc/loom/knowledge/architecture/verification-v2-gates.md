# Verification V2 Gates

> v2 completion gates, order, owners

## Verification v2 Gates: Order, Owners, and What Each Gate Reads

Every v2 gate reads `stage.plan_version == 2` (a `Stage` field, default 1, set through
`PlanIdentity` in `Stage::from_definition`); none re-reads the plan file for the version. A v1 stage runs
none of them.

**Completion order** (`commands/stage/complete_verification.rs::run`): goal-backward checks
(`run_goal_checks`), then `complete_verification_v2::run_v2` for v2 stages, then `after_stage`, the unwired
and duplicate checks, the aggregated check (integration-verify only). Inside `run_v2`
(`complete_verification_v2.rs`): contract check (standard stages with contracts), test integrity (standard and
integration-verify), impact-selected tests (standard), reachable re-verification (integration-verify), and
the review gate last. A host completion job (`checks.daemon_gates`, the child run in `host_verify/child_complete.rs`)
skips test integrity and the review gate in `run_v2`; the daemon runs both when it applies the job
(`daemon/server/observer.rs::check_completion_gates`, called from `host_verify/apply.rs::check_outside_lock`)
before it completes the stage. See "Review gate and harvest" below. `complete_verification.rs` sits at 399 lines, so new v2 calls go in `run_v2`.
`complete_verification_v2` is a `#[path]` child of `complete_verification.rs`, so its tests run as
`commands::stage::complete::complete_verification::complete_verification_v2::tests::*`; a filter
`commands::stage::complete_verification_v2` selects zero tests.

**No gate has a verifier subagent.** Integration-verify: the orchestrator runs `loom check integration-verify`
once, and that check carries the complete suite; reviewers run only targeted checks (`INTEGRATION_VERIFY_OVERRIDE`,
`orchestrator/signals/cache.rs:115`). Its acceptance criteria are the full gate, and `loom stage complete` runs
every one of them again. Beyond that check, the orchestrator runs only what completion does not: functional
checks, markdownlint, flake-check, Cargo.lock and ledger diffs, and clippy, fmt and maintainability checks first.
Standard stage: the orchestrator runs `loom check <stage-id>` once (`ORCHESTRATOR_GATE`,
`orchestrator/signals/cache/blocks.rs:90`), a fix round runs `loom check <stage-id> --criterion <n>`, and the
loom-code-reviewer round judges the diff against each criterion's intent. Completion runs once per final tree.

**Zero-test guard** (`testrun::registry::recognize`, called from `verify/criteria/runner.rs`). After a v2
criterion runs, an adapter that recognises the command and parses `executed == Some(0)` fails it with
`selected zero tests (<adapter>)`. Recognition runs on the command with variables expanded and `setup`
excluded, in the stage working dir; otherwise `cargo build` or `cd` in setup would be recognised before the
criterion's own runner. The flag lives in `CriterionContract` (part of the cache digest) and the certified
verdict records `tests_executed`, so a v1 pass of a zero-test run never certifies for a v2 stage.

**Wiring v2** (`verify/goal_backward/wiring_v2.rs`, `definition_sites.rs`). A `source` containing `*`, `?` or `[`
is a glob relative to `working_dir`; the check passes when any readable match holds the pattern, skips
unreadable files, and reports the first read error only when none could be read. Glob matches are
canonicalised and read relative to the canonical working dir. `literal: true` escapes the pattern, and
`validate()` skips the regex-compile check for it. Definition-site exclusion: a match counts as a definition,
not a consumer, only when it holds at least one occurrence of a name defined in that file on that name's
definition line and no occurrence of a defined name anywhere else (whole-identifier match through
`context::lexical::whole_term_ranges`, re-exported `pub(crate)`). Coverage `Full` and `Partial` count as
checked; `LexicalOnly`, `Oversized` and `ParseError` are unchecked and produce a stderr warning when a pass
rests only on them.

**Reachable** (`verify/goal_backward/reachable.rs`, `GapType::Unreachable`): resolves `symbol` and `from` by
exact name, preferring symbol-level nodes over `File` nodes (a `File` node takes its stem as its name, so
`from: main` would otherwise make everything in `main.rs` reachable), walks `impact_with`, passes when any
symbol node is reached from any `from` node. That is a false-pass risk for bare names such as `run`; use a
distinctive name. `has_any_goal_checks` counts `reachable` and `regression_test`, so a stage whose only goal
check is `reachable` runs it at completion. A field added to `has_any_goal_checks` also needs a
subsection in `signals/format/sections/goal_backward_section.rs`, or the signal emits an empty
`## Goal-Backward Verification` header.
The worktree graph (`context/worktree_graph.rs`, `build_for_worktree`) is in-memory and read-only: newest base
layer that is an ancestor of HEAD, plus files changed by `git diff-index --name-only -z <base> --` and
`ls-files --others --exclude-standard` (plumbing: porcelain `git diff` can write the index and reports
renames as the new path only). With no base layer it extracts every supported tracked and untracked file
under `working_dir` and diffs against HEAD.

## Verification v2 Gates, Continued: Integrity, Change Fingerprint, Review Gate

**Test integrity** (`verify/integrity/`): events `TI-decl-<lang>`, `TI-assert-<lang>`, `TI-edit-<path>`,
`TI-ratchet-<path>`. `TI-edit` matches removed assertion lines against added lines as a multiset, so deleting
one of two identical assertions is caught and a pure move cancels. Counts read unchanged base files from the
worktree and only changed ones through `git cat-file`. `ratchet_files` entries are exact checkout-root-relative
paths (no glob) and any change, tightening included, raises `TI-ratchet`. The gate reads
`reviews/<stage>/integrity.json` and accepts an event no worse than accepted; acceptance is an upsert by
event id because `gate::shortfall` reads the FIRST record per event. The evidence snapshot filed with the
dispute is what an accept records, not a re-scan.

Impact-selected tests have their own section below.

**One observer for the change fingerprint** (`verify/review/observer.rs`, `verify/review/fingerprint.rs`,
`daemon/server/observer.rs`). Both the review gate (comparing a round's fingerprint with the current one) and
the test-integrity gate (comparing counts derived from it) need one value computed the same way at both ends.
A sandbox's view of the tree differs from the host's: a sandbox mounts `/dev/null` over root dotfiles the host
does not have (`verify/tool_artifacts.rs`), masks git's global config, and runs with its own `HOME`, so two
processes computing the fingerprint for identical content disagree (see mistakes/sandbox-state-channels.md,
"The Same Value Computed in Two Filesystem Views Never Matches"). The loom daemon that owns
the worktree is the one observer: `fingerprint::compute` asks it over `Request::ObserveChanges` on
`.loom/work/orchestrator.sock`, naming only the stage id, and the daemon resolves the worktree, target branch
and fingerprint itself (`fingerprint::compute_local`, git pinned to the stage's registered git directory via
`WorktreeGit::pinned` so the worktree's own `.git` file cannot pick the configuration). `compute` computes
locally itself only when `worktree` names no stage worktree, or when nothing answers on the socket AND the
daemon's singleton lock proves no daemon runs (`DaemonServer::proven_stopped`, an unheld `flock`-able regular
file). Every other outcome is the typed `DaemonUnreachable` error, and the caller fails closed rather
than falling back to its own view. `loom stage review status`/`review integrity` may fall back to a locally
computed value labelled as such, for display only, never for a value recorded or compared.

A stage session cannot reach the daemon socket (its sandbox denies `AF_UNIX`, or, under `allow_all_unix_sockets`, masks the daemon socket, which `daemon/rpc.rs` classifies the same way), so a session's `loom stage complete` relays the completion and computes neither gate itself. The completion job's application runs both gates at the daemon (`daemon/server/observer.rs::check_completion_gates`) before it completes the stage, outside the session lock since both gates only read.

**Review gate and harvest** (`verify/review/`, `commands/hook/review_harvest.rs`,
`commands/hook/review_transcript.rs`). The `loom-code-reviewer` ends its report with a fenced `loom-review`
JSON block, in the `message` of its last `SubagentHandback` tool call or, if it hands back nothing, its final
assistant text; if both parse and disagree, the hand-back wins and the disagreement is logged.
`loom-hooks/subagent-stop.sh` pipes `{stage_id, session_id, agent_id, transcript_path}` to the hidden
`loom hook review-harvest`, which for a v2 stage reads the block from the transcript tail, asks the daemon
for the current fingerprint (sha256 over base sha plus sorted `<path>\t<sha256|deleted>` lines for every path
changed versus the merge base; commits do not change it), and writes
`.loom/work/reviews/<stage>/round-<n>.json`. Each suggestion becomes a `Suggestion` memory entry, written
with `fs::memory::append_entry` against the explicit work dir. An unparseable block records the round as
`malformed`. The gate passes when the latest WELL-FORMED round's fingerprint equals the daemon's current one
and every finding and carried finding is closed (listed `resolved` by a later round, or ruled `dismiss` or
`defer` in `rulings.json`; `uphold` does not close). A malformed round newer than a matching well-formed one
does not fail the gate; it is quoted only when the gate fails for another reason. `loom stage review status`
diffs against the latest well-formed round, not `rounds.last()`; a malformed round has a valid fingerprint and
zero findings and would tell the next reviewer nothing changed. Review order: fix findings, run the full gate,
run the final review, complete; any edit after it, formatting included, needs another round.
`STATE_READ_DIRS` (`sandbox/settings.rs`) gains `contracts` and `reviews`; it is `#[rustfmt::skip]` on one
line to stay at the ledgered 445 lines, and two `sandbox::settings::tests` pin the exact allow list.

**Aggregated wiring in integration-verify** (`run_aggregated_check`) re-reads every completed stage's `wiring`
from the plan file and re-runs it on the merged tree, and re-verifies every v2 stage's `reachable`. It returns
early for any other stage type. A stage that moves a call another stage's wiring pins must amend that wiring
(operator `loom stage amend --field wiring`), leave the call in place, or name it in a comment at its new site
([pinned literals](../mistakes/pinned-literals-ledgers-and-wiring.md)); pin wiring to the entry module or use `reachable`.

## Impact-Selected Tests: Selection, Base Comparison, Observed Record

**Impact-selected tests** (`verify/impact_tests.rs`, `verify/impact_tests/`): tests reaching the changed nodes
through `impact_with`, run per adapter through the criteria runner (300 s, certified cache). A test selects only
when every step of its path is a bound edge (`impact_tests/reach.rs`); one reached only through an ambiguous call's
candidate set is a namesake note, never run. The selection set is every node in a file the
stage changed since its merge base with the target branch (`git merge-base HEAD <target>`, via
`verify/review/fingerprint.rs::changes_since_merge_base`), not since the nearest published graph base; the graph itself
is still layered on that base, and a commit that lands on the target after the stage branched is not a stage change.
A runner's package comes from project detection, which skips `fixtures` directories, so test-data manifests are no
packages. The gate names no language: each adapter declares what it needs and how it selects
(`TestRunnerAdapter::required_environment`, `selection_names`). The JS adapters need `node_modules` when the
package declares dependencies (an unreadable or unparsable manifest counts, per
`skills::project::probe::declares_dependencies`), pytest and unittest need `.venv` beside a `uv.lock`; a package
missing it in its directory and every ancestor up to the root (`impact_tests/runs.rs::missing_environment`) is a
note, as is any runner run that exits 127,
and so is every error the runner returns (E2BIG on an exec of thousands of names, for one; no length bound is configured,
exec reports the OS limit): `run_group` names only its root cause. Notes cut the command at 200 bytes and list 10 names.
Exit 127 cannot tell a missing runner from a suite that itself exits 127. Yarn PnP packages declare dependencies and
have no `node_modules`, so they get the same note. It also selects the nodes of changed
test files, since `impact_with` never returns its start node, but skips every test inside a contract FILE,
so non-contract tests placed in a contract file run nowhere before integration-verify. Cargo targets are
`Function` nodes named by a libtest path derived from `mod` declarations, `#[path]` included
(`impact_tests/libtest.rs`); which files are crate roots is the cargo adapters' Cargo layout rule
(`SelectionNames::LibtestPaths { is_crate_root }`, `testrun/adapters/cargo_test.rs::is_crate_root`). The Rust
extractor does not model macros (`context/extract/rust.rs:19`), so a test whose only use of the changed code
sits inside `assert_eq!(...)` is not selected; write fixtures with the call outside the macro. No selection,
a timeout or no `select_command` is a note, not a failure.

**Judging an impact failure** (`impact_tests/{report,accepted,base,failure}.rs`). A failed selection is
`IT-<runner>:<package>` (`failure::failure_id`; no runner name holds `:`, so the first one splits the id whatever the
package path holds, and `check_failure_id` refuses any other shape, at the CLI and in the daemon). An accepted dispute
(`reviews/<stage>/impact.json`) notes it only while it fails exactly as accepted: the same `FailureKind` (variant and
count or exit code) over the same set of selected tests (`AcceptedImpact::standing`). A smaller count is judged again,
since the accepted record is the stage session's unauthenticated report. Otherwise it is re-run in a shared clone of
the merge base in a scratch directory (`base.rs::BaseCheckout`; it writes nothing into the repository's git directory,
a linked worktree's included). Only `fails` with a known count at both, the stage's no higher, is pre-existing and a
note (`FailureKind::covers`, used only for this comparison of two runs the gate made). The comparison is by count
only: an untracked fixture missing from the base checkout can inflate the base count. A selection that does not build,
or whose output or count the adapter could not read, stays a failure with the base result in its message even when
the base ended the same way: the base checkout holds tracked files only and often cannot build, which would hide a
compile error the stage made. Each remaining failure fails the gate with `ImpactTestsFailed`, naming
`loom stage dispute-criteria <stage> --field impact-tests --test <id>` for the first 32; the message names the ids past
that as not disputable after this run.

**The observed record** (`verify/impact_tests/observed.rs`, `reviews/<stage>/impact-observed.json`) holds the failures the
stage's latest failing completion run reported, each bounded and stripped of control characters
(`ImpactFailure::bounded`: 32 failures, 64 test names, 4 KiB output tail keeping `\n` and `\t`, 1 KiB texts). A
dispute names ids only and the daemon copies its evidence from this record (see
[adjudication-lifecycle](adjudication-lifecycle.md#dispute-kinds-criterion-findings-contract-integrity-impact-tests)),
so the filing agent's request adds nothing to `impact.json` or the judge's evidence. The record is not authenticated.
The host job that runs a completion writes it after a failing run (`host_verify/child_complete.rs::record_failed_impact`),
and an operator's own completion writes it directly. Either run executes the stage's own code, so every field carries
that code's output. The adjudicator is the trust gate: its prompt labels every field as reported by the stage session
and tells it to re-run each disputed selection in the worktree, and the base in a separate checkout, before accepting
(`orchestrator/adjudication/prompt/impact.rs`). An accepted forgery excuses only the exact failure it claims. A failing
run replaces the record; a passing run leaves it, and the adjudicator's re-run judges any stale failure again.

## Host Verification Jobs

> Daemon-run host jobs for completion, checks and freezes: state, child, apply

A stage's completion, its gate check and its contract freeze run as host jobs. The session relays a request, the daemon queues a job, and a child process that the daemon starts on the host does the work under loom's own confinement. The session follows the job through the state directory. The daemon applies the finished job, and that application is the only step that changes stage state.

## Job Records and State

Records live under `<state root>/verification/<stage>/` (`host_verify/store.rs`): `current.json` holds the stage's current record, and each job has a directory with its record copy, `progress.jsonl`, the result, logs and a `groups` file of criterion process groups.

- Kinds (`JobKind`): `check` runs the stage gate and changes no state; `complete` runs the completion and is applied as a completion; `freeze` runs the contracts for [Contract Phase](contract-phase.md).
- States (`JobState`): `queued` and `running` leave; `passed`, `failed`, `cancelled` and `interrupted` are terminal.
- Every record change is a compare-and-set under the stage lock (`submit.rs`, `runner.rs`): the record is reloaded and changed only while it is still this job in the expected state.

## Components

- `commands/stage/complete_relay.rs`: the session side of completion. It relays a `complete` request, follows the job, then follows the daemon's application. A legacy session is refused; an operator's shell completes locally.
- `orchestrator/core/inbox_drain/complete.rs`, `check.rs`, `freeze.rs`: the daemon queues a relayed request as a job and settles the ticket at once with the job id.
- `orchestrator/core/host_jobs.rs`: the runner's tick, the startup recovery and the apply retries. The runner is `host_jobs: Option<HostJobRunner>` on `Orchestrator` (`orchestrator/core/orchestrator.rs`); the `OrchestratorResult` type lives in `orchestrator/core/orchestrator_result.rs`.
- `host_verify/runner.rs`: starts each queued job as a `loom stage verify-host` child, polls it with `try_wait`, and ends it on cancellation, deadline or output cap. The tick blocks on a child only while retiring it, for at most the process cleanup grace.
- `commands/stage/verify_host.rs`, `host_verify/child.rs`, `child_complete.rs`, `freeze.rs`: the child run.
- `host_verify/apply.rs`, `apply_rejection.rs`, `freeze.rs`: the daemon's application of a finished job.
- `host_verify/wait.rs`, `wait_state.rs`, `wait_report.rs`: the session's follow. A call with no matching request state relays a ticket and returns at once; a call that finds one waits inside its budget. Both exit 0 while nothing is decided, because Claude Code runs the relay and heartbeat hooks only after a Bash call that exits 0.
- `host_verify/signals.rs`, `submit.rs`, `digest.rs`, `store_fs.rs`: process signalling, queueing and cancellation, the tree digest, and the no-symlink file access under the state root.

## Child Run

- Paths come from the state root (`JobTree::resolve`). A stage's checkout is its worktree. A knowledge stage runs in the repository root with no worktree root, so a `working_dir` below the root does not grant the whole repository. The child reads the repository through `WorkDir::main_project_root` and `resolve_acceptance_dir`; it does not call `acceptance_runner::resolve_knowledge_execution_paths`.
- Caching follows the record. Unless the record says `no_cache`, the child sets `CachePolicy::Use` and the cache directory under the state root, so the daemon's environment never decides caching.
- A knowledge stage's child creates the completion attestation key first (`handoff::attestation_key`), because its confinement binds over the state root's secret files.
- Acceptance runs first. The completion checks (`complete_verification`) then each run, even after an earlier failure, so one run names every failing check. Test integrity and the review gate are not run by the child; the daemon runs them at apply.

## Cancellation and Restart

- Only `take_down_agents` cancels a job (`orchestrator/core/event_handler/stage_takedown.rs::cancel_host_job`). A takedown, retirement, handoff, stall requeue, park or contract hand-off all reach it through that one function.
- A cancellation is a state write. The runner kills a child whose record is no longer `running` on its next tick.
- A cancelled job that no runner retired is found at restart by its child: the child still runs, and its command line names the job (`signals.rs::stop_children`). Elapsed time does not decide it.

## Escalation Hold

`orchestrator/monitor/hung_latch.rs::host_job_holds_session(work_dir, stage, session_id, now)` holds a session's escalation while the session's relayed job is unfinished and within its deadline, counted from its start or, while it waits for a slot, from its queueing. A record that cannot be read holds nothing. `host_job_holds_escalation(work_dir, stage, now)` keeps its three-argument signature, is test-only, and judges `stage.session`; the hung latch calls `host_job_holds_session` with the session it judges.

## Reading The Tree

- The tree digest (`host_verify/digest.rs`) runs through `WorktreeGit::read_isolated` (`git/worktree/pinned.rs`) on both sides. A session reads its checkout as discovered; the child and the daemon read it pinned to the stage's registered git directory, so the worktree's `.git` file cannot name a git directory that runs a command.
- `checkout_git` and `local_git` (`verify/review/observer.rs`) pin to the OUTERMOST stage worktree among a directory and its ancestors, not the nearest, because a stage can lay out a nested `.git` beside its own worktree. Deliberate limit: a repository nested inside a stage worktree is read through the enclosing stage's git. Revisit if a host process must read a loom project kept inside a stage worktree.

## Application

`host_verify/apply.rs::apply_finished` runs for each finished record, and on startup for each terminal record with no `applied` marker. The `applied` marker is written last, so a crash between the transition and the marker repeats the step, and the session's waiter reads the marker before it reads the stage.

- Passing `complete`: `check_outside_lock` checks ownership and runs `daemon::check_completion_gates` (`daemon/server/observer.rs`: test integrity and the review gate, against the change fingerprint). Then `bind_and_complete`, under the stage lock, requires that the stage is still `Executing` with the same session, that the run passed at the record's head, that the committed content and the check-definition hash still match the run, and then sets `completed_commit` to the run's head. A knowledge stage is also marked merged; any other stage waits for the merge flow. The session is retired and its acceptance receipt is written.
- A binding or gate refusal writes a rejection checkpoint (`apply_rejection.rs`).
- Failing `complete`: a failure checkpoint and attempt evidence come from the failed result.
- Cancelled or interrupted `complete`: nothing is written. A checkpoint would replace the latest attempt a block reads and spend the session's evidence.
- `check`: changes no stage state.
- `freeze`: `freeze::apply_freeze_result` freezes the contracts only while the binding holds, and parks the stage otherwise.
- An application that fails with an I/O or state error is retried at each later tick, up to `MAX_APPLY_ATTEMPTS` (`orchestrator/core/host_jobs.rs`). The last failure writes the marker as `apply failed: <error>`, so it is never retried again.
