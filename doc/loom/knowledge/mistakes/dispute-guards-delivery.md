# Dispute Guards Delivery

> host-job hangs, parallel worker collisions, scaffold wording, hook quirks

## Host-Job Guards Were Added Where the Failure Was Seen, and Two Full Runs Hung

**What happened:** integration-verify's full-suite host job hung twice: the lib test binary sat at 0% CPU for 36 minutes with six stand-in processes alive, and the first run leaked about 30 `sleep`/busy-loop stand-ins to PID 1. The first fix guarded `direct_worker_live_tests.rs` only; the second run hung in `codex_lifecycle/foreground_tests.rs`, `orchestrator/monitor/foreground_parallel_tests.rs`, `status/data/heartbeat_foreground_tests.rs` and `tests/integration/helpers.rs`. A third pass found 11 tests that still failed because the sandbox hides their own stand-ins from the process table.

**Why:** the guard was written per test file from the failures in front of the worker. `LiveTree::drop` called `killpg` then an unbounded `wait()`, and the host job's profile granted no signal, so the wait never returned. Each stand-in spawner is independent, so one fix never covered the next.

**Prevention:** before any full host-job run, search test code for every stand-in (`while :; do`, `sleep 300`, `killpg`, `process_group(0)`, `Command::new("kill")`), guard each before it spawns with the probe it needs (`process_groups_signalable`, plus `process_tree_visible` for a test that finds the process through the table), and bound every cleanup wait. Never use `--skip` for this. See [Tests in a Host Job](../conventions/host-job-test-guards.md).

**Fix:** guards and macros (`require_group_signals!`, `require_process_control!`) in `process/sandbox_probe.rs`; the operator added `(allow signal (target same-sandbox))` to the host-job profile. `GroupAnchor::retire` still waits unbounded ([concern](../concerns/sandbox-and-confinement-gaps.md#host-job-test-gaps)).

## Parallel Fix Workers Collided Through Formatting, Build Directories and the Size Limits

**What happened:** during integration-verify, (1) a worker told to format only its files ran `rustfmt` over every changed file in the worktree while three siblings were editing them; (2) four workers each reported their files within the 400-line and 50-line limits, then `tests/maintainability.rs` found 11 new violations (3 test files over 400, 8 functions over 50); (3) each worker built into `loom/target/iv-<name>`, and nine such directories (81 GB) made stage completion refuse: `git ls-files --others --ignored ... stdout exceeded the 16777216 byte output limit`; (4) with several workers in one worktree, `cargo test --lib` ran a binary built from another source tree and reported a panic at a line of an older file version.

**Why:** the briefs named the rule, not the command that proves it; completion lists every ignored file in the worktree; siblings sharing `loom/target` rebuild each other's inputs.

**Prevention:** a brief that touches code ends with `cargo test --test maintainability` and requires its result quoted (a 3-line rustfmt-expanded guard pushes a 48-line test over 50; use the guard macros). `rustfmt` takes the worker's own file paths, never a `git diff` list; the orchestrator formats the tree once after all workers return. Give parallel workers `CARGO_TARGET_DIR` outside the worktree (under `$TMPDIR`) or delete theirs before completion. When a panic line does not match the file, or the test count changes between runs, rerun with a private `CARGO_TARGET_DIR`.

## The Contract Signal Said "No Scaffold Edits" for Writer-Chosen Contracts

**What happened:** four contract writers (`confined-test-guards`, `contract-check-and-trial`, `contract-dispute-filing`, `criterion-narrowing-evidence`) read "No scaffold edits: every contract file is already collected" although no contract file had a `mod` declaration, and each had to add the `#[cfg(test)] mod` lines itself.

**Why:** `verify/contracts/collection/mod.rs::StageTree::scaffold` derives edits only for contracts whose `test` is set, and every contract in these plans read `(you choose)`. The freeze derives the scaffold over the merged `--test` choices, so the signal's text was wrong at the time it was printed.

**Prevention:** recurrence across four stages: the signal should say the scaffold is derived at freeze when tests are writer-chosen. A writer declares each new contract file in its parent `mod.rs` itself. Tracked in [concerns](../concerns/contract-autonomy-followups.md#contract-signal-and-prompt-text).

## A Bash Call Quoting the Completion Command Was Blocked by the Control Hook

**What happened:** three times a Bash call was refused with `completion must be one exact pinned command`, because its heredoc or script text merely mentioned the stage-completion command (a knowledge body, a python edit script quoting a doc comment).

**Why:** the `loom-control-complete` PreToolUse hook matches the command text, not the executed command.

**Prevention:** write such bodies with the Write tool and pass them by path (`loom knowledge replace-section <file> <heading> - < body.md`); never put the literal command in a Bash call's text.

## A Temporary Debug Line Was Removed With `git checkout`

**What happened:** a worker ran `git checkout <file>` to drop one debug line and lost every uncommitted edit to the file.

**Why:** checkout restores HEAD, not the state before the debug line.

**Prevention:** remove temporary debug lines with the Edit tool; never `git checkout` a file with uncommitted work.

## Load Generators Started in a Sandboxed Call Outlived It

**What happened:** detached `yes` CPU hogs started in one Bash call could not be killed by `pkill` at its end, because the macOS sandbox denies process listing.

**Prevention:** start load generators as direct children and kill them by `$!` in the same call (`yes & pids+=($!); ...; kill ${pids[@]}`); never through `(cmd &)` subshells; report possible leftovers to the user.

## Eight Confinement Tests Were Merged Failing Because No Gate Ran Them

**What happened:** after integration-verify merged, seven tests in `host_verify::tests_child_contracts` and `tests_child_base::a_criterion_passing_at_the_base_records_a_pass` failed on main when run from a plain shell.

**Why:** a test that starts the real confinement returns early through `confinement_unavailable` in a stage session and in a host job, where a nested `sandbox-exec` is refused. Neither the workers nor the gate executed these tests. The contract fixture's plan had no `working_dir`, which the stage schema requires, and two expectations contradicted the code merged with them: `Restore::finish` (`verify/contracts/trial.rs`) leaves a file written during the run as written, and the pytest adapter records no failed count for a passing run.

**Prevention:** run a test file guarded by `confinement_unavailable` once from an unconfined shell with `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` before accepting the stage that adds or changes it. CI's strict steps are the only gate that executes them.

**Fix:** the fixture gained `working_dir` and the two expectations follow the code.

## A Merge Watch Matched an Earlier Plan's Merge Commit

**What happened:** an operator session waited for integration-verify to merge with `git log --oneline -30 | rg 'Merge loom/integration-verify into main'`. It fired at once on an earlier plan's merge commit with the same title, and work started on a tree the stage had not reached.

**Why:** every plan has a stage named `integration-verify`, so the merge title repeats in history.

**Prevention:** bound the range to commits after the tip seen when the watch starts (`<tip>..main`), or read the stage's `merged` state from `loom status`.

## A Stage Was Reported as Executing While Its Third Dispute Was Being Judged

**What happened:** an operator session reported knowledge-distill as executing from `loom status --compact` and stopped watching. The stage had filed three disputes in eleven minutes and moved to human review seconds after the last check; the operator found out from the user.

**Why:** the one-line status counts a stage in adjudication as executing ([concern](../concerns/sandbox-and-confinement-gaps.md#the-one-line-status-does-not-show-adjudication)), and the session never read the stage events or the dispute directory.

**Prevention:** while a plan runs, read `.loom/work/stage-events.jsonl` for transitions and list `.loom/work/disputes/<stage>/` at every check; never report a stage's state from the compact line alone.

## An Operator Re-Verified a Fix the User Had Confirmed

**What happened:** asked to check whether the runner fixes were on main, an operator session found the commits and then kept tracing the host-job confinement source to prove the fix covered one criterion.

**Why:** it treated a request to confirm commits as a request to re-derive the fix.

**Prevention:** answer the narrow question asked (are the commits on main), report it, and continue the queued work; dig further only when asked or when a gate fails again.

## Sixteen Load Generators Ran for Seven Hours and Nobody Looked

**What happened:** the `yes` CPU hogs a stage agent left behind ([Load Generators Started in a Sandboxed Call Outlived It](#load-generators-started-in-a-sandboxed-call-outlived-it)) kept sixteen cores busy for seven and a half hours. An operator session ran builds, test runs and five workers on the same machine in that time, saw load averages above 100, and blamed slow builds. The user found the processes. The same session had left seven scratch clones and eight experiment directories on disk, about 200 GB by apparent size.

**Why:** the stage agent reported "possible leftovers" in a memory note that nobody acted on, and the operator never listed processes or its own scratch directory between tasks.

**Prevention:** [Leave No Process or Scratch Copy Behind](../conventions/config-plan-and-gate-rules.md#leave-no-process-or-scratch-copy-behind). A leftover an agent cannot kill itself is reported to the user in its final message, with the command that finds it, not only recorded in memory.

## A Check Run in a Copied Clone Reused a Binary That Scans the Original

**What happened:** to keep a reviewer from seeing mutations, a verification script copied a scratch clone with `cp -cR`, planted a violation in the copy and ran the structural guard there. The guard passed. Cargo had reused the copied integration-test binary without rebuilding, and that binary has the original clone's `CARGO_MANIFEST_DIR` compiled in, so it scanned the original tree.

**Why:** a copied `target` keeps fingerprints that match the copy's unchanged sources, and `env!("CARGO_MANIFEST_DIR")` is fixed at compile time.

**Prevention:** in a copied clone, touch or edit a source file of the test target before any check whose test reads files through `CARGO_MANIFEST_DIR`, and confirm `Compiling` appears in the log. A mutation that edits the crate's own source rebuilds by itself; one that only adds a file does not.

## The Last Stage of the Plan Stalled on `loom memory pending --strict`

**What happened:** knowledge-distill filed three criterion disputes in eleven minutes and ended in human review. Its criterion `loom memory pending --strict` exited 0 in the session and failed in the host job with `Operation not permitted (os error 1)`; the operator force-completed the stage.

**Why:** the host job confines a distill stage's criteria to its worktree, and the command lists `<state root>/memory` through the worktree's `.loom/work` link, outside the boundary. The dispute could not remove the criterion either: at the stage's base (a tracked-files checkout with no state root) the command prints `no memory journals` and passes, so the base-evidence gate answered "passes at the base: it fails because of the stage's own change". Every plan ends with this criterion, so every plan would have stalled the same way.

**Prevention:** a criterion that reads plan state needs a grant for exactly what it reads ([A Knowledge-Distill Stage's Criteria Read the Memory Journals](../architecture/memory-spool.md#a-knowledge-distill-stages-criteria-read-the-memory-journals)). A base pass by a command that found no state to read proves nothing about the stage. When a change adds a gate that refuses a way out (here, narrowing only on base evidence), run each standard criterion of each stage type through it once before the plan that adds it completes.

**Fix:** the memory journals grant, on all three routes that run a stage's criteria.

## Closing the Plan Left the Working Tree Dirty

**What happened:** after the last merge the daemon renamed the plan to `DONE-PLAN-*`, logged `Failed to commit post-completion changes: git add -A -- ... failed: The following paths are ignored`, and left the tracked `IN_PROGRESS-PLAN-*` file deleted in the working tree. The next `loom run` would have refused the dirty tree.

**Why:** the closing commit staged the old and the new path in one `git add`, and git refuses the whole command when one path is ignored. This repository ignores `doc/plans/DONE-*`.

**Prevention:** stage paths whose ignore status differs in separate commands, and test a git step against a repository that ignores the path it writes.

**Fix:** `fs/plan_lifecycle/commit.rs` always stages the old path's removal and stages the new path only when `git check-ignore` says it is not ignored.

## Private-Root Tests Were Merged Without a Sandbox Guard

**What happened:** 21 tests that build a native worker's private root (`native_worker::boundary::private_root_tests`, its rollout tests, one each in `runner_recover_tests`, `tokens::discover` and `commands::subagents`) failed on Linux in a host job and in a plain shell with umask 002, and failed for a second reason inside an agent session's sandbox. A plan's dry run reported four test selections failing at the base commit as "expected before the stage's work", with no test name.

**Why:** two environment assumptions. The fixtures created their directories with `tempfile::tempdir()` and `create_dir_all`, which give `0o777 & !umask`: 0775 under umask 002, which `trusted_directory` refuses as writable by other users. CI and macOS run with 022. Separately, in a user namespace that binds the real root, `/` reports uid 65534 and the same check refuses it. The tests came in with the macOS native-worker work and no Linux run with umask 002 happened before the next plan. A first diagnosis made inside a session sandbox found only the second cause, because the walk refuses `/` before it reaches the temp directory; the host job mounts its own root, so the fix for that cause changed nothing there.

**Prevention:** a fixture whose directories are checked for mode or owner creates them with explicit modes (`test_private_dirs::private_tempdir`, `create_dirs`) and never relies on or sets the umask. A failure reproduced in one sandbox is not evidence about another: get the failing output from the environment that reported it before fixing. A test failing at a plan's base commit already exists there; read the dry-run note's failing lines.

**Fix:** explicit modes in the five fixtures; `require_directory_owners!` for the session sandbox; the scratch sweep test accepts the user's own private `/tmp`; the dry run lists the failing lines of a test command that fails at the base commit.
