# Completion Runner Blocks

> Runner-env blocks, gate fails

## Criteria the Confined Runner Cannot Run Ended in a Manual Block (2026-10-20)

**What happened:** stage delivery-fixes (plan token-optim/PLAN-02) blocked itself with work committed, review clean and 19 of 20 criteria green. Criterion 14 (`git ls-files '*.md' | ... | xargs bunx markdownlint-cli2`) failed inside `loom stage complete` with `fatal: not a git repository: .git/worktrees/delivery-fixes` and `ConnectionRefused downloading package manifest markdownlint-cli2`. The plan's sandbox allowed `registry.npmjs.org`. The agent had folded criterion 14 into its dispute of criterion 13; the adjudicator judged only 13 ("Criterion 14 is not the disputed criterion and is not judged here"). The next session called `loom stage block` although one criteria dispute remained. Earlier in the same stage, criterion 13 hit the fixed 300 s criterion timeout, and two hook tests failed only under loom's runner.

**Why:** the confined criteria runner denies all network whatever the plan's `network.allowed_domains` says, and it does not expose the worktree's git metadata, so any criterion using git or a package fetch cannot pass at completion. A dispute named one criterion index and the adjudicator judged only that index. Doctrine routed "a network domain or path the plan does not grant" to `loom stage block`, and the agent read an environment-caused criterion failure as that case.

**Prevention:** a criterion that fails only because of where completion runs (network, git metadata, timeout, confinement) is disputed, never blocked. Every failing criterion goes into the dispute. Plan authors check each criterion against the completion runner, not against the session sandbox.
**Fix:** operator deleted criterion 14 after running it on the host (290 files, 0 issues). Loom now closes each gap:

- **The runner** mounts linked-worktree git metadata read-only, gives Linux criteria a private `/tmp` and `/var/tmp`, tolerates a dangling resolver link, and accepts `timeout_secs` on an extended criterion.
- **`loom plan verify`** rejects a registry fetch in a confined criterion whatever `allowed_domains` says.
- **Disputes:** one dispute names several criteria, the adjudicator judges every failing one, and an accepted verdict requeues with its reasoning.
- **`loom stage block`** is refused while the latest completion shows a failing criterion.
- **The verifier** runs criteria through `loom check`.
- **The two hook tests** pass under the runner.

## Post-Merge Cleanup Raced the Closing Session and Blamed the Main Checkout (2026-10-20)

**What happened:** after delivery-fixes merged, `git worktree remove` deleted the tracked files and the `.git/worktrees/delivery-fixes` admin dir, then failed with "Directory not empty": the finished session's sandbox had recreated `.claude/.cc-writes/` three seconds earlier. Every retry, `loom worktree remove` included, then failed with "is not a working tree". The warning said "refused ... force was not set" and listed the main checkout's modified and untracked files as "Blocking paths".

**Why:** cleanup knew only `git worktree remove` and had no path for a worktree git had already dropped. `blocking_paths` (`git/cleanup/worktree.rs`) ran `git status` with the leftover directory as cwd; with no `.git` there, git walked up to the main repository. The headline assumed every failure was the dirty-tree refusal.

**Prevention:** a cleanup step that shells out to git pins repository discovery to the path it inspects. A half-removed worktree holding only scaffold is finished, not reported.

**Fix:** operator removed the empty `.claude/.cc-writes` tree, ran `git worktree prune` and `loom worktree remove delivery-fixes`, which deleted the merged branch and marked the stage merged.

## Trusting the Agent-Writable `.git` File to Pick Host Mounts (2026-10-21)

**What happened:** the first cut of the git-metadata binds followed whatever `gitdir:` the worktree's `.git` file named and bound that directory's `config`, `objects` and `refs` read-only. Review showed a crafted `.git` file or a symlinked entry would expose `~/.ssh` or `~/.kube`, and a FIFO at `commondir` would hang loom.

**Why:** the tree is agent-controlled, and the code treated a file in it as a trust anchor.

**Prevention:** a host path a sandbox exposes is derived from loom's own state (the `.worktrees/<id>` layout), and every file read on the way is bounded, no-follow and non-blocking.

**Fix:** `confine_git.rs::git_dirs` anchors on the repository that holds `.worktrees` and checks `commondir` and `gitdir` both ways. It binds nothing on any mismatch and never binds in Provision mode.

## Boundary Canaries Asserted the Write Syscall, Not the Host File (2026-10-21)

**What happened:** adding a private writable `/tmp` broke three boundary canaries and the provision test. Each expected an in-sandbox write beside a `/tmp`-hosted tree to fail; the write now succeeds into the tmpfs.

**Why:** the in-sandbox expectation stood in for the boundary property. What has to hold is that the host file stays unread and untouched.

**Prevention:** when a boundary's mounts change, re-read every canary's in-sandbox expectations. The host-side assertion is the proof; under `set -e`, a negated command (`! cmd`) asserts nothing.

**Fix:** the in-sandbox writes became `|| true`, the host-side checks stayed, and `CACHE_CANARY`'s negated lines became real checks.

## Acceptance Commands Misread What the Runner Reports (2026-10-20)

**What happened:** four separate misreads cost rounds during PLAN-02-delivery-truth.

- `rg -q -g '!tests/**' read-receipt ../loom-hooks` still matched `loom-hooks/tests/post-tool-use-tool-event.sh`. The glob is applied relative to the search root, so `!tests/**` never excludes `../loom-hooks/tests/`; `'!**/tests/**'` does.
- A wiring pattern spanning a call's first argument (`let text = worker_query_text(&config.stage`) broke when rustfmt split the call across lines (`worker_brief.rs`).
- `loom check` exits 0 when an acceptance criterion fails: `execute` returns `Ok(())` after `print_summary` whatever `acceptance_passed` is (`loom/src/commands/verify.rs:138`). Its summary shows `Acceptance: ✗` instead.
- `loom check` shows only the last 20 lines of a failing criterion's output (`TAIL_LINES`, `loom/src/commands/stage/criterion_output.rs:7`) and persists no full output, so an unfiltered `cargo test` failure cannot be named from it.

**Why:** the runner's report is a rendering of the result, and each one drops something a shell run shows: the exit code, the full output, rustfmt's final layout.

**Prevention:** read the Summary line of `loom check`, never its exit code. Run a long criterion in the shell to name its failing tests. Write `rg` globs as `'!**/dir/**'` and run single-line wiring patterns against rustfmt-formatted output, or anchor them on a token that cannot wrap.

## The Agent Sandbox Fails Part of the Full Gate for Environment Reasons (2026-10-20)

**What happened:** in the agent sandbox the unfiltered `cargo test` gate failed 28 tests. Unix socket binds are denied (`handoff/completion/content_tests.rs:151`, the `completion_replay` daemon socket); `/var/tmp` is read-only for `verify::*` scratch; `TMPDIR=/tmp/claude-1000` trips the `claude`-name precheck in `hooks_subagent_verify_guard` (it passes under `/run/user/1000/loom/scratch/<session>`); and `bounded-stdin.py`'s `codex-forward-guard.sh` case sees stage evidence from an ancestor's `LOOM_*` environment and exits 2. In the completion sandbox `markdownlint` fails on unreadable git metadata and a refused `bunx` download, and the BSD-mode `run-all.sh` suite sits at the 300 s criterion timeout (about 5.5 minutes standalone, unchanged by the stage). `integration-verify` completed with every criterion green except the unfiltered `cargo test`, which needs a Unix socket.

**Why:** the confined runner and the session shell differ in network, sockets, temp directories and ancestor environment, so a criterion can pass in one and fail in the other without a code defect.

**Prevention:** a criterion that fails only because of where completion runs is disputed in one `loom stage dispute-criteria` filing naming each index, never blocked and never weakened. Confirm it by running the same command in the other environment first; a `loom-verifier` run that is green there is the evidence. Plan authors should avoid unfiltered suites that need sockets.

## The Read Tool Cannot Read `$LOOM_SCRATCH_DIR` in a Worktree Session (2026-10-20)

**What happened:** `worktree-file-guard.sh` blocks the Read tool on `/run/user/1000/loom/scratch/session-*` ("resolved path is outside the current worktree"), although Bash may write there.

**Why:** the guard confines file tools to the worktree and the handoff write root; the scratch directory is neither.

**Prevention:** read scratch output with `rg` from Bash, or keep intermediate output inside the worktree where the rules allow it.

## Judges Were Closed While Their Criteria Run Was Still Going (2026-10-21)

**What happened:** integration-verify disputed criterion 1 (`cargo test --all-targets`): tests binding Unix sockets fail only under the session sandbox. Each of three judge sessions started `loom check --no-cache` in the background and ended its turn to wait. The judge-stall watchdog closed each one after 300 s with no tool call ("JUDGE STALLED"), and the stage went to NeedsHumanReview: "Adjudication of dispute 1 produced no verdict after 3 session(s)".

**Why:** the judge's stall budget is the stage's `subagent_timeout_secs` (default 300 s) and activity is heartbeats only (`monitor/hung_latch.rs::adjudicator_stall_event`). A judge must run the criteria, which now legitimately take up to the sum of their timeouts (`timeout_secs` up to 3600), so a judge waiting on its own command reads as hung.

**Prevention:** a watchdog that closes a session must tell waiting on its own running command apart from silence. A budget derived from one setting cannot cover work whose length another setting controls.

**Fix:** operator ruled on the dispute (criterion 1 skips the three sandbox-only tests, all of which pass outside it) and approved the review. `monitor/judge_watchdog.rs` now holds a judge whose own process has a running command child (a shell, `bwrap` or `sandbox-exec` that itself runs something, found by `monitor/running_command.rs` from `/proc` or `ps`) and prints `JUDGE WAITING:` once per wait, up to the disputed stage's summed criterion timeouts plus one budget; a judge with no heartbeat and no command of its own past the budget is still closed. The three tests skip themselves through `process/sandbox_probe.rs` when Unix sockets or nested confinement are unavailable, and `loom check` exits non-zero when a criterion fails.

## Plan Completion Committed the Operator's Work in Progress (2026-10-21)

**What happened:** when PLAN-02 completed, `chore(loom): mark plan complete` (`f353d650`) carried the operator's uncommitted edits to twelve tracked source files: half of a watchdog change, the `loom check` exit-code fix and three test skips. Their new untracked files stayed out, so the commit declared modules it did not contain and did not compile.

**Why:** `fs/plan_lifecycle/commit.rs::commit_post_completion_changes` ran `git add -u` in the operator's checkout. It was there to catch knowledge files, but stage work reaches the target branch as merge commits, so `-u` only ever caught operator work.

**Prevention:** an automated commit in the operator's checkout stages the exact paths it changed, never `-u` or `-A`. Paths the operator already staged still ride along; `git commit --only -- <paths>` would exclude those too.

**Fix:** the completion commit stages only the plan rename, and `unrelated_tracked_edits_stay_out_of_the_plan_done_commit` pins it. The follow-up commits `d4d8fe20` and `ac6c55f2` completed the swept work.

## The Full Suite Fails 26 Tests in a Verifier Shell and Passes Under `loom check`

**What happened:** four stages and the integration-verify gate each saw `cargo test --all-targets`
fail the same 26 tests in a Claude Code sandboxed shell, none touched by the diff: 17 lib tests
(`verify::before_after`, `goal_backward::truths`, criteria `working_dir`, confine `tool_tests`)
with `EROFS` at `/var/tmp/.loom-command-*`, and 9 `hooks_subagent_verify_guard` integration tests.

**Why:** `confine_scratch.rs` `scratch_roots` drops the root equal to the tree under test, leaving
only `/tmp` and `/var/tmp`, both read-only in the sandbox. The 9 guard tests refuse a `TMPDIR`
containing `claude` (`tests/integration/hooks_subagent_verify_guard.rs:44`); `TMPDIR` pointed at
`$LOOM_SCRATCH_DIR` fixes those 9 and not the 17.

**Prevention:** judge these names by `loom check <stage-id>`, the confined runner `loom stage
complete` uses, which passes the full suite. Treat them as verifier-shell environment only when
the diff touches neither `src/verify` nor the verify guard. Do not rerun with a `TMPDIR` override
in a subagent shell to chase the 17.

## A Symlinked Work Dir Made Every Session Record Read as Unreadable

**What happened:** `loom usage` run from a stage worktree reported Claude `partial` with
`unreadable-session-records` for all 8 records and printed no stderr line, while the same window
with `--project` pointing at the main checkout read 0 unreadable.

**Why:** from a stage worktree `.loom/work` is a symlink to the main checkout's work dir.
`completeness.rs::missed_claude_transcripts` enumerated session records and then read each one
through `fs::safe_read::read_to_string_bounded`, which opens its root with `O_NOFOLLOW`
(`safe_fs.rs`, `safe_open_dirfd`), so every read through the symlinked root failed. The
enumeration diagnostics were counted but never printed.

**Prevention:** canonicalize a work dir once before handing it to a bounded no-follow reader as
its root, and print per-record read failures (through `inline_safe`) so a count never stands
alone. Run a new `loom usage` check from inside a stage worktree, where the symlink exists.

**Fix:** `missed_claude_transcripts` resolves the root with `std::fs::canonicalize` and prints
each diagnostic to stderr.

**Recurrence (2026-10-21, host-jobs):** the same no-follow read failed on the completion wait path. `wait::ledger_outcome` read the inbox ledger through the worktree's `.loom/work` symlink, and `safe_open_dirfd` (`O_NOFOLLOW`) refused it, so `loom stage complete` failed on `host_verification_e2e` and `tests_commit_timing` after the verifier rounds had passed. A doctrine rewrite had also dropped the sentinel sentence "Commits happen ONLY as the final step of the stage", which `tests_commit_timing` pins.

**Why:** a bounded no-follow reader rejects a symlinked root, and every session-side read of the work dir goes through the symlink. The verifier rounds ran before the last doctrine edit, so they did not cover the text that broke.

**Prevention:** canonicalize the work dir before every `safe_fs` or inbox read reached from a session, and rerun the full criteria list after the last doctrine edit.

**Fix:** not recorded in the memory.

## A Relay Branch That Returned Ok Ran the Criteria in the Session (2026-10-21)

**What happened:** in the host-jobs stage, `verify_relay::run` returned `Ok` when the wait exited 0 in Relay mode. `execute` then fell through to the local run: it parsed the plan and ran the criteria in the session instead of stopping at the daemon's result.

**Why:** the Relay success path returned normally, so the caller had no signal that the relayed run had finished and continued into the local path.

**Prevention:** a routing function whose branch ends the process must not return `Ok` on the success path. The Relay branch exits with the wait code.

**Fix:** the Relay branch always calls `std::process::exit(code)` (`loom/src/commands/verify_relay.rs:33` and `:43`).

## Claude Code Refused the Nested Criteria Runner on Linux, and the First Relay Design Exited 3 (2026-10-21)

**What happened:** Claude Code 2.1.306 started its Linux Bash sandbox with `bwrap --disable-userns`, so the nested `bwrap` that in-session stage completion used for criteria (`verify/criteria/confine_host.rs::wrap`) failed every criterion with `No permissions to create a new namespace` before its command ran. The 2.1.302 binary has no such flag. Commit `d7ef6a8e` kept loom working in the interim by emitting `sandbox.allowNestedUserNamespaces: true` in every enabled Linux capsule; with the key, a sandboxed `bwrap --ro-bind / / --dev /dev true` exits 0, without it 1. Claude Code documents that key as letting a sandboxed command bypass `denyWrite`, `denyRead` and credential denies in any folder it may write. The host-verification plan then moved `loom check`, stage completion and the contract freeze to host jobs the daemon runs, and its first draft had the requesting and still-running calls exit 3 ("run it again"). The plan's 2026-10-21 pressure test caught that draft before any code shipped.

**Why:** the in-session path ran criteria inside the session, nested in Claude Code's own sandbox, and reported a verdict through the completion broker (the `LOOM_CONTROL_EVIDENCE_V1` record plus the `RecordCompletionEvidence` and `CompleteStage` RPCs). That session-trusted path could not survive the refusal without the opt-out, and the opt-out widened every session's reach. With the daemon running the criteria on the host the session's verdict is not needed, and keeping the broker RPCs would leave a second, weaker completion route, so the broker was removed instead of adapted. The exit-3 design failed because Claude Code runs PostToolUse hooks, `loom-relay.sh` among them, only for a Bash call that exits 0: across session transcripts 1,403 Bash results carrying `LOOM_RELAY_V1` that exited 0 all have a PostToolUse row, and the 15 that exited non-zero have none. A call that printed a ticket and exited 3 would never have been relayed, and its progress heartbeat (also PostToolUse-only) would have gone missing.

**Prevention:** before pinning a relay protocol to exit codes, check which Claude Code hook events fire for a non-zero exit; a Bash call that prints a relay line exits 0, and the doctrine tells agents to read the `Host job:` first line, never the exit code alone. Pin it with a test that the requesting call exits 0 (the plan's `rejects:` text names the exit-non-zero variant). Pressure-test a protocol plan before its first stage starts. Treat a nested-sandbox opt-out as a stopgap with a removal condition ("until criteria run on the host").

**Fix:** in-session `loom check`, stage completion and the contract freeze relay a request; the daemon runs the job as a host child under loom's own confinement and completes or freezes only from its own run. Requested, pending, passed and completed exit 0 under a `Host job: <state> (<kind>, <short head>)` first line; failed, rejected and refused exit 1; stopped exits 4. The completion broker, the macOS daemon-socket grant, inherited confinement and the user-namespace opt-out are gone (plan `IN_PROGRESS-PLAN-host-verification.md`, Protocol decision 1 and deviations 2-3).
