# Verification V2 Gates

> v2 completion gates, order, owners

## Verification v2 Gates: Order, Owners, and What Each Gate Reads

Every v2 gate reads `stage.plan_version == 2` (a `Stage` field, default 1, set through
`PlanIdentity` in `Stage::from_definition`); none re-reads the plan file for the version. A v1 stage runs
none of them.

**Completion order** (`commands/stage/complete_verification.rs::run`): goal-backward checks
(`run_goal_checks`), then `complete_verification_v2::run_v2` for v2 stages, then `after_stage`, the unwired
and duplicate checks, the aggregated check (integration-verify only) and change impact. Inside `run_v2`
(`complete_verification_v2.rs`): contract check (standard stages with contracts), test integrity (standard and
integration-verify), impact-selected tests (standard), reachable re-verification (integration-verify), and
the review gate last. A stage session runs inside a sandbox that cannot reach the daemon's socket
(`checks.control_session.is_some()`), so `run_v2` skips test integrity and the review gate there and returns;
the daemon runs both (`daemon/server/observer.rs::check_completion_gates`) before it applies the
`CompleteStage` transition — see "Review gate and harvest" below. `complete_verification.rs` sits at 399 lines, so new v2 calls go in `run_v2`.
`complete_verification_v2` is a `#[path]` child of `complete_verification.rs`, so its tests run as
`commands::stage::complete::complete_verification::complete_verification_v2::tests::*`; a filter
`commands::stage::complete_verification_v2` selects zero tests.

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
distinctive name. `has_any_goal_checks` counts `reachable` and `regression_test`; a stage whose only goal
check is `reachable` used to skip it at completion. A field added to `has_any_goal_checks` also needs a
subsection in `signals/format/sections/goal_backward_section.rs`, or the signal emits an empty
`## Goal-Backward Verification` header.
The worktree graph (`context/worktree_graph.rs`, `build_for_worktree`) is in-memory and read-only: newest base
layer that is an ancestor of HEAD, plus files changed by `git diff-index --name-only -z <base> --` and
`ls-files --others --exclude-standard` (plumbing: porcelain `git diff` can write the index and reports
renames as the new path only). With no base layer it extracts every supported tracked and untracked file
under `working_dir` and diffs against HEAD.

## Verification v2 Gates, Continued: Integrity, Impact Tests, Review Gate

**Test integrity** (`verify/integrity/`): events `TI-decl-<lang>`, `TI-assert-<lang>`, `TI-edit-<path>`,
`TI-ratchet-<path>`. `TI-edit` matches removed assertion lines against added lines as a multiset, so deleting
one of two identical assertions is caught and a pure move cancels. Counts read unchanged base files from the
worktree and only changed ones through `git cat-file`. `ratchet_files` entries are exact checkout-root-relative
paths (no glob) and any change, tightening included, raises `TI-ratchet`. The gate reads
`reviews/<stage>/integrity.json` and accepts an event no worse than accepted; acceptance is an upsert by
event id because `gate::shortfall` reads the FIRST record per event. The evidence snapshot filed with the
dispute is what an accept records, not a re-scan.

**Impact-selected tests** (`verify/impact_tests.rs`): tests reaching the changed nodes through `impact_with`,
run per adapter through the criteria runner (300 s, certified cache). It also selects the nodes of changed
test files, since `impact_with` never returns its start node, but skips every test inside a contract FILE,
so non-contract tests placed in a contract file run nowhere before integration-verify. Cargo targets are
`Function` nodes named by a libtest path derived from `mod` declarations, `#[path]` included. The Rust
extractor does not model macros (`context/extract/rust.rs:19`), so a test whose only use of the changed code
sits inside `assert_eq!(...)` is not selected; write fixtures with the call outside the macro. No selection,
a timeout or no `select_command` is a note, not a failure.

**One observer for the change fingerprint** (`verify/review/observer.rs`, `verify/review/fingerprint.rs`,
`daemon/server/observer.rs`). Both the review gate (comparing a round's fingerprint with the current one) and
the test-integrity gate (comparing counts derived from it) need one value computed the same way at both ends.
Before this design, a round's fingerprint came from the host `subagent-stop.sh` hook running
`loom hook review-harvest`, while completion computed its own inside the agent's sandbox: a sandbox mounts
`/dev/null` over root dotfiles the host does not have (`verify/tool_artifacts.rs`), masks git's global config,
and runs with its own `HOME`, so the two processes' fingerprints for identical content differed and a round
could never match — see mistakes/sandbox-state-channels.md, "The Same Value Computed in Two Filesystem Views
Never Matches". Now the loom daemon that owns
the worktree is the one observer: `fingerprint::compute` asks it over `Request::ObserveChanges` on
`.loom/work/orchestrator.sock`, naming only the stage id, and the daemon resolves the worktree, target branch
and fingerprint itself (`fingerprint::compute_local`, git pinned to the stage's registered git directory via
`WorktreeGit::pinned` so the worktree's own `.git` file cannot pick the configuration). `compute` computes
locally itself only when `worktree` names no stage worktree, or when nothing answers on the socket AND the
daemon's singleton lock proves no daemon runs (`DaemonServer::proven_stopped`, an unheld `flock`-able regular
lock file); every other outcome is the typed `DaemonUnreachable` error, and the caller fails closed rather
than falling back to its own view. `loom stage review status`/`review integrity` may fall back to a locally
computed value labelled as such, for display only, never for a value recorded or compared.

A stage session's sandbox denies `AF_UNIX`, so a sandboxed `loom stage complete` cannot ask and runs neither
gate locally (`complete_verification_v2.rs`, above); the daemon runs both
(`daemon/server/observer.rs::check_completion_gates`) right before it applies the `CompleteStage` transition
(`control_complete.rs`), outside the session lock since both gates only read.

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
(operator `loom stage amend --field wiring`) or leave the call in place; pin wiring to the entry module or use
`reachable`.
