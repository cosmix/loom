# Verification V2 Delivery

> Wave, gate, proof misses in v2

## A Wave Brief Left Shared Files Unowned, and Mid-Run Approval Never Arrived (2026-09-24)

**What happened:** across schema-v2, contract-phase, dispute-kinds, review-harvest-gate and runner-adapters, workers
kept hitting files no brief owned: `plan/schema/mod.rs` (explicit re-export list), `models/stage/mod.rs` and
`defaults.rs`, `daemon/mod.rs`, the exhaustive `match request` in `daemon/server/client.rs:274`,
`inbox_drain/test_support.rs`, `cli/types_memory.rs` (the `--outcome` `value_parser`), `commands/subagents/mod.rs`,
and about fifteen test files that construct a struct whose field moved (`criterion_index` into `DisputeKind`). A
worker that asked for approval by message got none: `SendMessage` to a busy worker never reached it, and it ended
with four edits undone.

**Why:** briefs listed the files a feature is written in, not the files a signature, enum arm or re-export forces.

**Prevention:** before fan-out, run `loom map --impact <symbol>` for every pinned type and list what breaks;
assign each shared declaration site (`mod.rs`, re-export lists, exhaustive matches, CLI value parsers, test payload
helpers) to exactly one worker; a worker may make a visibility-only edit to an unowned file and must report it.
Two workers never write one file; when a brief hands one worker a type another owns (`DisputeVerdict` in
`models/dispute.rs`), move the task to the owner. Do not plan on messaging a running worker.

## A Truncated Caller Search Was Read as Complete (2026-09-24)

**What happened:** a worker changed `verdict::parse_and_validate`'s signature after `rg ... | head -30` cut off
`commands/stage/adjudicate.rs:109`; the proof build broke in another worker's file.

**Prevention:** before changing a `pub` signature, run the caller search with no limit, or `loom map --impact`.
The same rule applies to a brief that names a private function: test-guards W2 cited `run_with_cache`, which is
private, and the public entry was `run_acceptance_with_config` with a probe stage.

**Fix:** kept `parse_and_validate(raw)` and added `parse_and_validate_for(raw, kind)`.

## One Worker Was Given 33 Files and Hit the Turn Limit (2026-09-24)

**What happened:** a dispute-kinds worker with a 24-file territory plus nine test files hit the 150-turn subagent
limit with no report. Its code compiled and its named tests existed, so the harvest was possible but blind.

**Prevention:** split a territory of 30+ files into two workers or require an interim report file. Spawn stage
workers WITHOUT the `name` parameter: a named worker runs as an in-process teammate that idles after reporting,
so `loom subagents watch` never sees terminal evidence and waits to its deadline. `watch` also rejects the
`name@session-<uuid>` id (exit 5, "worker set does not resolve to one Claude parent UUID"); it needs the
transcript agent id from `loom subagents list`. Codex-only watches need `--session <claude-session-uuid>`.
Never pipe `watch` through `tail`: the pipe masks the exit code. Under the Linux sandbox each forwarder runs in
its own PID namespace, so a codex "process gone" from the watch is unverified; check the job log mtime and the
forwarder's completion notice. A knowledge stage exits with "no worktree identity"; wait on notifications there.

## An Explore Subagent Told to Return Commands Ran Them (2026-09-25)

**What happened:** an Explore subagent, told to be read-only and return `replace-section` commands, ran them, and one
replacement contradicted itself.

**Why:** Explore has Bash, so "read-only" is an instruction, not a limit.

**Prevention:** diff the knowledge files after every harvest before trusting a "verified" or "applied" report.
A body that starts with `-` needs `replace-section --`. Distillation runs single-agent for this reason.

## Workers and Codex Units Measured Sizes Before rustfmt (2026-09-24)

**What happened:** `adjudication/prompt/contract.rs` went from 321 to 411 lines and three functions passed 50 lines
once `cargo fmt` ran; `run_goal_backward_verification` went 49 to 53 lines when rustfmt re-wrapped a 61-character
call (`fn_call_width` 60); a one-line gate call in the stage-verification entry file became five lines (399 to
404).

**Prevention:** workers skip the formatter, so their line counts are pre-fmt. The main agent runs `cargo fmt`
BEFORE the maintainability test, tells codex units to format their file before measuring, and puts new v2 calls in
`run_v2` (the parent file sits at 399 of 400 lines). Workers hand-format to rustfmt defaults (chain width 60, call
width 60, max width 100). rustfmt orders new `mod` lines alphabetically.

## The Stage Gate Was Green and the Commit Still Failed (2026-09-24)

**What happened:** four separate stages passed build, clippy, fmt and their own tests, then failed at commit or in the
full suite: (1) the pre-commit hook's `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps` rejected an intra-doc link to
a private module (`verdict.rs`, `verify/integrity/mod.rs`) and an ambiguous ``[`shell_words`]`` that matched the crate of
that name; (2) two `sandbox::settings::tests` pinning the exact `STATE_READ_DIRS` list (13 entries) broke when
`reviews` was added, and only the full `cargo test` caught it because each worker ran one narrow proof; (3) a
`#[path]` child-module declaration inserted next to a sibling by hand failed `cargo fmt --check`; (4) a fixture plan
was committed unlinted (see the next entry).

**Prevention:** the gate before commit is build, `cargo clippy --all-targets -D warnings`, `cargo fmt --check`,
`RUSTDOCFLAGS='-D warnings' cargo doc --no-deps` and the full `cargo test`; a brief that adds to a pinned list names
the tests pinning it; write links to private items as plain code spans.

## The Markdown Lint Step Skips Silently in a No-Network Stage (2026-09-24)

**What happened:** `loom/.githooks/pre-commit` runs `bunx markdownlint-cli2 --fix 2>/dev/null || true`. In a stage
sandbox `bunx` needs `registry.npmjs.org` for transitive packages even when the tool itself is cached, the proxy
denies it, and the commit succeeds with no lint. Fixture plans and skill files were committed unlinted; a later
unsandboxed commit can reformat them.

**Prevention:** a stage that edits markdown says markdownlint did not run, does a manual fence and table review, or
the plan grants the registry host. Rerun `plan verify` on fixture plans after any later autofix. The hook should
fail loudly or detect a cached binary instead of `|| true` (open item in `concerns/verification-v2-followups.md`).

## A Test Script Existed and the Canonical Gate Never Ran It (2026-09-25)

**What happened:** `loom-hooks/tests/loom-relay-kinds.sh` and `loom-relay-gates.sh` were never in `run-all.sh`, a static
`run_test` list. The regression test for the dispute-kind relay fix never ran, so the missing `relay_kind_at` case shipped.
Separately, `run-all.sh` took 334 s against a 300 s criterion timeout; `_path_without.sh` forked a `ln` per binary, and
one `ln` per PATH directory cut it to 162 s. Timing a hook test by hand needs `env -u LOOM_HOOK_PATH`.

**Prevention:** a new `loom-hooks/tests/*.sh` gets its `run_test` line in the same change; compare `fd -e sh
loom-hooks/tests` with the list. Test modules attached by `#[path]` (`review_status_tests.rs`, `skip_retry_tests.rs`,
`contract_budget_tests.rs`, `impact_tests_tests.rs`, `complete_verification_v2_tests.rs`) are not followed by the
unwired-file detector; name the `cargo test --lib` paths that run them.

## Commit Mechanics That Cost a Stage Time (2026-09-24)

- **Splitting one file's hunks across commits** (`git update-index --cacheinfo`): the pre-commit hook rejects staged
  paths with unstaged changes and compiles the WORKING TREE, so the first commit failed, its staged files rode into
  the next, and a mixed commit went in under the wrong message. For concerns sharing a file, write each intermediate
  file state into the working tree (reverting later-concern files that would not compile), stage whole files,
  commit, then restore the final files.
- **A clippy ICE** ("the compiler unexpectedly panicked", analysis passes, clippy 1.97.1) hit `loom stage complete`
  after files were swapped between intermediate states. An immediate manual re-run exited 0 on the same tree; retry.
- **A merge in the main checkout** fails with `unable to unlink old loom/maintainability-baseline.txt: Device or resource
  busy` because the sandbox bind-mounts that file writable. Merge in a detached worktree, commit there, `git reset`
  (mixed) in main, write the baseline in place with `git show`, `git checkout --` the rest.
- **zsh** applies `:l` (lowercase) to an unbraced `$M:path`; `git show $M:loom/...` failed and the redirect truncated the
  baseline. Write `${M}:path`.
- **A Bash command whose text names `loom` and the stage-completion path** (a python heredoc editing
  `commands/stage/complete_verification.rs`) is read by `loom-control-complete.sh` as a completion attempt and blocked
  as untokenizable. Edit that file with the Edit tool, or put long text in a file and feed it on stdin.
- **`loom memory note` with stdout sent to `/dev/null`** hides the `LOOM_RELAY_V1` line the relay hook reads; entries
  reached the daemon only through the leftover-ticket sweep on a later Bash call. Pipe through `tail -1`.

## A Gate Counted the Wrong Round, or Skipped a Check Entirely (2026-09-24)

**What happened:** four guards had a hole their own tests did not reach.
(1) `has_any_goal_checks` did not count `reachable`, so a v2 stage whose only goal check was `reachable` never had it
run at completion (integration-verify's re-check still caught it on the merged tree); counting `regression_test`
then made a regression-test-only stage's signal emit an empty `## Goal-Backward Verification` header.
(2) `loom stage review status` diffed against `rounds.last()`, which may be malformed; a malformed round carries a valid
fingerprint and zero findings, so the next reviewer was told nothing changed and the gate could pass an unreviewed diff.
(3) `recover_orphaned_sessions` requeued a dead Contract session generically, so crash and restart cycles spawned
writers without spending `MAX_CONTRACT_RESPAWNS`.
(4) The v2 `loom_subcommands` lint downgraded an unknown subcommand when ANY stage touched `loom/src/cli`, so an
independent stage's real typo lost its error.

**Why:** each check read one shared input (a field list, the last round, "any dead session", "any stage in the plan")
where the rule is about a narrower one.

**Prevention:** for a gate, name the exact record it anchors on (latest WELL-FORMED round) and test the malformed
neighbour; a field added to `has_any_goal_checks` needs its subsection in `goal_backward_section.rs`; a budget is
spent where the work is handed out, on every path that hands it out; a plan-wide predicate that changes a stage's
verdict must be scoped to that stage and its transitive dependencies.

## Documentation and Reviews Asserted More Than the Tree Showed (2026-09-24)

**What happened:** the orchestration skill listed "aggregated wiring" among the checks every stage gets; the check returns
early unless the stage is integration-verify. A reviewer claimed `git ls-files` run from a subdirectory lists the whole
repo; it does not (`--full-name` only changes path display). Reviewers also flagged Scala `IO.parTraverseN` (real in
cats-effect 3), Ruby `Data.define` positional args (raise `missing keyword`) and a five-item catalog row (within the
table's range). A language-skills note claimed flutter detection is a substring read of the Dart manifest; the tree matches an `sdk: flutter`
line in the Dart manifest.

**Prevention:** when documenting a check sequence, open each called function and note its stage-type guard; verify a
reviewer's claim against the tool or library before acting, and record the ruling with its evidence; treat a memory as a
claim until it is checked against the tree (stale memories in this run were caught only that way).

## Fixtures and Proof Commands That Could Not Fail (2026-09-24)

**What happened:** (1) impact-selection fixtures called the changed function only inside `assert_eq!(add(1, 2), 3)`, so no
test was reached: the Rust extractor does not model macros. (2) A fix-worker's proof `rg` for the defect text also
matched legitimate example commands in the Tooling tables of three skills. (3) A byte-for-byte golden for a text-builder
refactor was captured without compiling the crate by running the old and new functions in a scratch `rustc` program with
stub types and `cmp`ing the output. (4) A worker removed `StageSandboxConfig` from a tests `mod.rs` import list as unused after collapsing
literals to `..Default::default()`; the child module `confinement.rs` reached it through `use super::*`.

**Prevention:** call the symbol outside any macro in graph fixtures (`let sum = add(1, 2); assert_eq!(sum, 3)`); scope a
proof grep to the section being fixed or match the full template line; before deleting an import from a `mod.rs` or a
tests parent, `rg` the name in sibling files that glob-import `super::*`.

## A Rust Plan With Contracts Ran From the Repository Root (2026-09-30)

**What happened:** PLAN-source-graph-mechanism gave all seven contract stages `working_dir: "."` and passed `--manifest-path loom/Cargo.toml` to every acceptance command. Contracts do not take that flag. The cargo adapter builds `cargo test <name> -- --exact` (`testrun/adapters/cargo_test.rs::single_test_command`), and freeze and completion run it from the stage's working dir (`verify/contracts/site.rs`). The repository root has no `Cargo.toml`, so every contract would exit 101. Freeze accepts that exit as red, and completion fails the stage on it. `loom plan verify` reported 0 errors. A pressure test caught it before execution.

**Why:** the author checked the acceptance commands, which carry their own manifest path. The contract command is generated by loom, and it never appears in the plan.

**Prevention:** a stage with `contracts` sets `working_dir` to the directory holding the test runner's manifest (`loom` for this crate). Every YAML path is then package-relative. `artifacts`, `wiring.source` and contract `file` reject `..`, so the existence checks for files outside the package move to a stage whose working dir is `.`.

**Fix:** the plan was converted to `working_dir: \"loom\"` for its seven standard stages before any run.

## A Contract File That Cannot Build Freezes as Red (2026-09-30)

**What happened:** each contract file in PLAN-source-graph-mechanism names symbols the stage creates. A Rust contract file compiles as one test binary, so each file freezes as `BuildFailed`, which freeze accepts as red. That proves nothing about any single assertion: a contract whose assertions already hold, or could never fail, still freezes.

**Why:** red-at-freeze is evidence of a failing assertion only when the file compiles against the base tree.

**Prevention:** when a contract file cannot compile at freeze, the stage proves each contract by mutation before its final review round. It applies the implementation named in `rejects:`, confirms the contract fails, restores the tree, and records `mutation: <id> red`. A negative guard is a legal contract under the same condition.

**Fix:** the plan's gate conventions require the mutation check in every code stage.

## A Frozen Contract File Failed the Stage's Own Formatter

**What happened:** `loom/tests/stage_exits_contracts.rs` was not rustfmt-clean (two `assert_eq!` lines wrapped under rustfmt). `cargo fmt --all` and the pre-commit formatter rewrote it, and `loom stage complete` then failed the frozen-hash check; restoring the frozen text failed the stage criterion `cargo fmt --all -- --check`.

**Why:** the contract session ran on the installed binary, which predates the freeze-time formatter gate ([plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md#freeze-time-formatter-gate)). The same hazard applies to any plan that runs on an older installed loom.

**Prevention:** in a stage with contracts, run `rustfmt --check` on the frozen files BEFORE `cargo fmt --all`. A frozen file that fails the stage's formatter is a `dispute-contract` at once, not an edit and not a revert.

**Fix:** dispute the contract; an accepted dispute re-freezes the file at its current content.

## A Frozen Contract Function Over a Size Limit Is Ledgered, Not Edited

**What happened:** two contract files carried functions over the 50-line limit (`tests/completion_gates_contracts.rs::wiring_dispute_indexes_the_wiring_list` at 51 lines, the 64-line `plan()` helper in `tests/plan_environment_contracts.rs`), so `cargo test --test maintainability` could not pass, while the signal said never to edit the ledger.

**Why:** frozen files cannot change, so the only way to keep the maintainability test green is a ledger line in `loom/maintainability-baseline.txt`. Precedent: `language_packs_contracts.rs`.

**Prevention:** add the ledger line, list the file in the plan's `ratchet_files`, and file ONE `dispute-integrity` for the resulting `TI-ratchet` event after the final review round, naming every ledger change in it. Record the choice with `loom memory decision`.

## Function Line Counts Taken Before `cargo fmt` Are Wrong

**What happened:** workers reported function sizes measured before formatting; after `cargo fmt` `handle_dispute_criteria` was 59 lines and `handle_file_dispute` 52. Separately, adding one `close_reason` line to `build_stage_summary` took it to 51 lines over a 50-line ledger entry in a stage that could not edit the ledger.

**Why:** workers skip `cargo fmt` (it is the main agent's check), and rustfmt wraps lines. A brief that adds a field to a ledgered function never named its line budget.

**Prevention:** brief workers to count lines as rustfmt will wrap them, and run `cargo test --test maintainability` after `cargo fmt`. A brief that adds a `StageSummary` field names `build_stage_summary`'s remaining budget; the fix that fit was folding the call into an existing tuple (`let (facts, now) = (...)`), which reads oddly and wants the ledger moved or a helper extracted.

## The Signal's Acceptance List Drops `exit_code`

**What happened:** three stages (plan-environment, stage-exits, knowledge-distill, which then wrote a phrase its own criterion required to be absent) read negative `rg` criteria (`exit_code: 1`, absence expected) as presence checks, because the signal's Acceptance Criteria list prints only the command.

**Why:** the signal renders the command without the criterion's `exit_code`; the plan YAML and `.loom/work/stages/<id>.md` are authoritative.

**Prevention:** before treating a signal criterion as failing, read the stage file under `.loom/work/stages/` for its `exit_code` BEFORE writing any text a `rg` criterion matches. A hook or `loom plan verify` check that prints `exit_code` in the signal would end the recurrence.

## Hook Guards That Refuse a Stage's Legitimate Command

**What happened:** the subagent-verify-guard hook blocked `cargo build --all-targets` for a subagent even when the brief prescribed it as the one check; a `cargo test --lib <module>::` filter passed. The `loom-control-complete` PreToolUse hook refused a Bash heredoc append whose text contained `loom stage retry` (it cannot tokenize the text safely); appending the same text with the Edit tool worked. It refused this distillation's own heredoc the same way.

**Prevention:** brief a subagent's single check as a module-filtered `cargo test --lib <module>::`. Put prose that quotes a control command into a file with the Edit or Write tool, never a Bash heredoc. Copy the 32-hex ids for `loom memory resolve` with `rg -o`, never retype them; `loom memory resolve` takes no `--stage` for another stage's suggestion (it refuses: a stage records only to its own journal), and without `--stage` it settles the suggestion with a receipt in the current journal.

## Tests That Flaked Under Load or Race

- `commands/hook/reconcile_graph/tests_wants_rebuild.rs::cancel_signals_a_holder_that_is_the_reconciler` failed about half its runs: `fake_reconciler` returned before the child exec'd `sh`, so `/proc/<pid>/cmdline` still held the test binary's argv and `is_reconciler` said no. The helper now polls `cancel::process_argv` until the argv holds `hook`. Prevention: a test that reads a child's `/proc` identity waits for the exec, it does not assume it.
- `commands::status::web::terminal::tests_pty::pty_child_shutdown_reaps_the_child` asserts a sub-second reap and failed once under the full `commands::status::` run beside other cargo jobs; it passes alone. Rerun it in isolation before blaming the change.

## A Single Fact Mirrored in Three Places

**What happened:** the status legend text is mirrored in `web/src/api/fixtures/statuses.json` (checked by `commands::status::web::model::tests::statuses_fixture_matches_stage_status`) and inlined into the tracked `web/dist/assets/index.js`; a legend change edited all three.

**Prevention:** after changing any status legend wording, run the `commands::status::web::model::` tests and rebuild or hand-edit the `web/dist` literal in the same change.

## Test Modules Wired by `#[path]` Read as Unwired

`orchestrator/provision_tests.rs` and `orchestrator/core/provision_gate_tests.rs` are wired as `#[cfg(test)] #[path = "..."] mod tests;` in their parent modules. The unwired-file check does not follow `#[path]` declarations and reported them as unwired. Confirm the declaration with `rg -n '#\[path' <parent>` before wiring a second copy.

## The Impact Gate Failed a Stage on Its Own Spawn Error

**What happened:** guard-core (PLAN-target-ref-guard) changed `src/git/runner.rs`, which reaches thousands of tests. The cargo adapter built `cargo test -- <names>` as one shell line, and `verify/criteria/confine.rs` spawns a shell line as a single `sh -c` argument, which Linux refuses above 32 pages with E2BIG. `impact_tests/runs.rs::Selection::run_group` passed the spawn error up with `?`, so `loom stage complete` failed while every stage gate passed. The agent could not fix `src/verify` (outside its `files`, and the installed binary runs the gate) and had no dispute for the impact gate, so it blocked; a retry blocked again two minutes later.

**Why:** D14's rule "a test that cannot be selected or run is a note" was implemented case by case (timeout, exit 127, no selector, missing `node_modules`), and a run error was never one of the cases. The fixtures were all small, so no test built a command near any OS limit.

**Prevention:** A gate that degrades when a run cannot happen degrades on every error the run returns, not on a list of known outcomes. Never answer an OS limit with a constant tuned to the host: the first proposed fix was a 100 KiB bound under Linux's 128 KiB per-argument limit, which scales with page size, does not exist on macOS (about 1 MiB for all arguments plus the environment), and would ship to every project loom runs. Let exec report the limit and degrade. Bound what a note prints: the criteria runner's error context carries the full command text.

**Fix:** `run_group` turns a run error into a full-suite note naming only the error's root cause, and every note, failure and `ran` entry shows the command cut at 200 bytes and at most 10 selected test names (`impact_tests/runs.rs`, tests in `runs_tests.rs`, one of them a real 4 MiB exec). The recovery gap this exposed is in [[concerns/merge-and-recovery-edge-cases]], "A Defect in a Loom Gate Parks the Stage Until an Operator Acts".

## Contracts in a Sub-Workspace Ran Under the Root's Runner Config (2026-10-14)

**What happened:** in a monorepo whose `apps/web/` has its own bunfig.toml (a happy-dom preload), and whose root bunfig.toml ignores `apps/web/**`, every contract in `apps/web` failed `loom stage complete` with `bun-test output not parsed`. Loom ran `bun test apps/web/src/x.test.ts -t ...` from the stage working dir. Bun read the root config, treated the non-`./` argument as a filter, and matched no file. The freeze had accepted that same no-match exit as `exit_nonzero_unverified`, so it never saw the contracts fail. A dispute could not help: it re-freezes content and cannot change where a contract runs.

**Why:** contracts assumed the stage working dir is every runner's project root. Runners that read config or find their project only in their cwd (bun, vitest, jest, mocha, dart, go, dotnet, phpunit) break when a contract file belongs to a nested project. The 2026-09-30 Rust case above is the same bug, worked around by the plan.

**Prevention:** a contract command is built only through `run_dir::resolve_run`, which resolves the run dir from the recorded cwd, plan `cwd`, adapter run-root markers, then working dir. An adapter whose output can say "no test file matched" must classify that as `NotSelected`, never leave it unparsed.

**Fix:** `cwd` on the plan contract and in `freeze.json`, `run_root_markers` per adapter, bun `./` paths, and bun no-match parsing (see architecture/contract-phase.md, "Where a Contract Runs").

## A `reachable` Check Fails When the Call Is a Value or Goes Through a Re-export

**What happened:** tui-tokens' `reachable` checks failed although the code worked: `left.and_then(quota_left_text)` passed the function as a value, and `pane_lines`/`budget` were called by a caller of `render_dashboard` rather than inside its chain. token-core's brief had to pin the same rule for `crate::tokens::` re-exports.

**Why:** the source graph records a call edge only for a function named in a call expression, resolved through its defining module; a function used as a value, a call made upstream of the named root, and a qualified call through a `pub use` re-export leave no edge.

**Prevention:** a `reachable` target must be called by name in an expression inside the named root's call chain, through its defining module path (`crate::tokens::model::read_flow`, never `crate::tokens::read_flow`). Write the plan's `reachable` entries and the call shape together.

**Fix:** a direct call in `left_spans` (`ledger/quota.rs`), and `pane_lines`/`budget` moved into `render_dashboard` (`ledger/layout.rs`).

## A Required Field on a Shared TypeScript Type Needs Every Constructing Test Updated, and Vitest Does Not Check It

**What happened:** a web worker made `StageNodeData.tokens` required and did not touch `stage-node.test.tsx`; the mid-wave `bun run typecheck` ran before that worker finished, and `tsc -b` inside `bun run build` raised TS2741 at the end.

**Why:** the worker's one allowed check was vitest, which strips types and never type-checks.

**Prevention:** brief every worker that a required field added to a shared type must be threaded through each existing test and fixture that constructs it, and run `bun run typecheck` once after the last worker returns, not between waves.

## A Source Edit After the Dist Build Fails the Dist-Diff Criterion

**What happened:** a round-1 lint fix touched `web/src/components/token-flow-panel.tsx` two minutes after `web/dist` was built, so the committed bundle still held the unfixed expression and the `web/dist` diff criterion failed. The legend-text mirror in [A Single Fact Mirrored in Three Places](verification-v2-delivery.md) is the earlier occurrence of the same coupling.

**Why:** `web/dist` is a committed build product of `web/src`; nothing ties the two together except the criterion at the end.

**Prevention:** any `web/src` change, lint fixes included, is followed by the vite build into `web/dist` as the last step before the gate. A completion check that rebuilds `web/dist` and compares is proposed in [Token Flow Gaps](../concerns/token-accounting-and-proof-defects.md#proposals).

## Filtering `loom stage commit` Output Loses the Commit Ticket

**What happened:** token-core piped `loom stage commit` through `rg` and tui-tokens through `tail -1`; both cut the `LOOM_RELAY_V1` line, so `loom request status` answered "not relayed yet" and the commit never reached the daemon. integration-verify chained several `loom memory resolve` calls with `&&` and a trailing command, and the relay hook refused both requests until the next standalone loom call.

**Why:** commit tickets are the control kind, which the hook's sweep never drains ([Memory Relay Drain Gap](memory-relay-drain-gap.md)); the relay reads the line from the Bash call's own unfiltered stdout and admits one memory-writing loom command per call.

**Prevention:** run `loom stage commit` and each `loom memory` write alone in a Bash call with stdout unfiltered. When the line was cut, issue the command once more: the first ticket never reached the daemon.

## A Build-State Claim Was Recorded Without Re-checking the Tree

**What happened:** integration-verify noted that a `nix::unistd::geteuid` compile error in `loom/src/tokens/tracker_tests.rs:354` was pre-existing at HEAD, from a `git status` run once at the start. The file had an uncommitted 127-line change in the working tree that added the line.

**Prevention:** run `git status` and `git diff <file>` immediately before recording a claim about build state, and attribute a compile error to a file only after the diff. The call is now `libc::geteuid()` in `unsafe` with a SAFETY comment, as `locator.rs` does.

## A Label Taken From a Brief Read as a Contradiction

**What happened:** an operator read the Token flow panel's mix row "fresh input 42/m" beside the headline "89.2k fresh / min" as contradictory. Both were right: the row is raw `input_tokens` (uncached, tiny under prompt caching) and "fresh" is input plus cache writes plus output.

**Prevention:** a label must not reuse a defined term for a different quantity. The panel row is "uncached input" and "cache hit (5 min)" (the header rail's cache hit is the plan total), and the TUI and web legends use the same wording.

## `rg -q` in a `pipefail` Smoke Script Fails With 141 or 23

**What happened:** `printf "$big" | rg -q X` and `curl url | rg -q X` failed once the payload passed the pipe buffer.

**Why:** `rg -q` exits on the first match, so the writer dies of SIGPIPE (141, curl 23) and `pipefail` reports it.

**Prevention:** match a captured variable with `rg -q X <<<"$var"`. `scripts/smoke-web-dashboard.sh` only pipes small pages and has not hit it.

## Recurrences Recorded Elsewhere

- A private `thread_local` linked as an intra-doc link from a `pub fn` doc failed `RUSTDOCFLAGS='-D warnings' cargo doc` after a restructure that ran only `cargo test`: [A `[`link`]` to a Private Item Fails the Docs Build](testing-and-lint.md).
- `loom-code-reviewer` rounds recorded twice for two of four parallel reviewers (identical findings at one fingerprint): a re-review lists every duplicate id under `resolved` ([subagent-liveness-and-watch.md](subagent-liveness-and-watch.md)).
- `loom subagents watch --worker codex:<unit>` exiting 5 or 3 beside Claude forwarder ids ("companion request cwd mismatch", a low pid from a PID namespace mismatch): wait on the Agent completion notifications ([subagent-liveness-and-watch.md](subagent-liveness-and-watch.md)).
- Codex output that was not rustfmt-clean: `cargo fmt --all` after the wave ([codex-worker-briefing.md](codex-worker-briefing.md)).

## Resolver content needs fresh completion evidence

**What happened:** The sweep left resolver-produced content eligible to land on the original completed commit proof. A first follow-up blocked unverified resolution without connecting the automatic resolver workflow; a scripted edit also assumed obsolete rustfmt layout.

**Why:** Ancestry and clean-worktree checks establish ownership, not verification. Resolver sessions have different ownership from ordinary completion broker sessions.

**Prevention:** Require fresh completion gates for resolver content through the existing trusted MergeResolved handler; preserve broker ownership checks. Inspect files after a partial edit instead of assuming every replacement applied.

**Fix:** A shared resolver verifier brackets acceptance and all completion gates with input/policy snapshots, validates under the stage lock, and records completed_commit. CLI resolved merge, daemon relay and resolver exit use it; landing compares the branch tip under the merge lock. Ordinary operator completion also accepts clean resolved worktrees.

## Auto merge must not manufacture completion proof

**What happened:** The follow-up audit found try_auto_merge calling capture_completed_commit after prechecks that do not require verification proof, allowing a Completed stage with missing completed_commit to acquire the current branch tip as proof.

**Why:** A legacy merge convenience confused discovering a branch tip with certifying that its content passed completion gates.

**Prevention:** Missing proof must route to fresh completion verification or review before merge; only verified content may create completed_commit for landing.

**Fix:** Remove proof manufacture from the automatic landing path and add a missing-proof regression alongside resolver re-verification.

## Every landing entry point must reject missing proof

**What happened:** Final review found automatic HEAD-derived proof in blocked retry, restart recovery, merged-state validation, resolver-exit ancestry checks, progressive completion and locked landing finalization, beyond the first automatic-merge guard.

**Why:** Closing one caller leaves other producers able to manufacture the proof consumed by the shared merge boundary.

**Prevention:** Trace every automatic completed_commit producer and require persisted proof to survive locked validation. Keep explicit authorized operator overrides distinct from successful verification.

**Fix:** The final verifier removes automatic fallback derivations, guards the shared landing boundary, and adds regression coverage. Existing authenticated operator no-verify and force-unsafe/assume-merged paths retain their intentional skip semantics at the explicit command boundary; a recorded commit in such a path does not mean tests ran. Full final-tree gates follow the complete fix.

## The Impact Gate Hid a Monorepo Package's Siblings

**What happened:** in a monorepo stage (collaterai shared-contracts) every acceptance criterion passed from the worktree root, but `loom stage complete` failed on impact-selected tests. The impact runner starts each test group in its package directory, and the confinement bound only the command's working directory, so sibling packages (a uv editable path dependency), shared fixtures and the root Bun store were invisible. The stage had no dispute for the impact gate and blocked.

**Why:** the confinement treated the working directory as the writable tree. That held while every caller ran from the worktree root; the impact gate was the first to start below it.

**Prevention:** a confined command's start directory and its visible tree are separate inputs. A caller that starts below the worktree root passes the root explicitly; never infer it from the nearest `.git`, which a vendored submodule also has.

**Fix:** see [execution containment](../architecture/execution-containment.md#read-this-first-what-containment-means-in-loom).
