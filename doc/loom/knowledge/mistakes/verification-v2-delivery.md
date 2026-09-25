# Verification V2 Delivery

> Wave, gate and proof misses in v2

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
