# Review Integration Verification
>
>

Integration failures and verification discipline for the Astra review repairs.

## Integrated Rust compile exposed missed lease field

**What happened:** The first integrated all-target compile failed because a graph-reconciler LockState initializer omitted the new claim field; it also reported an unused monitor import.
**Why:** Independent worker edits were handed back without compiling the integrated tree.
**Prevention:** Run the central all-target compile after worker handoffs and repair all diagnostics before the full suite.
**Fix:** Update the initializer and remove unused imports while preserving lease semantics, then rerun the compile.

## Tally changes lacked a finding flow reconciliation

**What happened:** Consecutive review tallies listed different status totals without explaining how new assignments and returned work moved between categories.
**Why:** The aggregate snapshots were correct but the intervening finding flow was not recorded.
**Prevention:** State newly assigned, newly verified and moved-to-partial counts beside each tally, and reconcile the transition arithmetically.
**Fix:** Explain the 14:28 to 14:41 transition in the tracker and preserve its timestamped snapshots.

## Asset test helper became unused after backup changes

**What happened:** Integrated all-target compilation warned that the_backup in assets/tests.rs is unused.
**Why:** The backup tests changed ownership without removing the now-orphaned private helper.
**Prevention:** Check shared test helpers after moving or replacing their last consumers and run the integrated warnings gate.
**Fix:** Remove the orphaned helper without removing test coverage, then rerun compilation and Clippy.

## Full-suite run started before final formatting

**What happened:** The integrated formatting check found many worker edits unformatted after the final all-target run had already started.
**Why:** I launched the suite and formatting check concurrently rather than completing the formatting gate first.
**Prevention:** Format and pass the check after all workers return, then start full verification on the settled tree.
**Fix:** Stop the premature run, format the integrated tree and restart verification.

## Full suite reported widespread failures without immediate diagnostics

**What happened:** The formatted-tree all-target run reported thousands of failures; a representative redirected run fails creating `/tmp` tempdirs while the direct Cargo command passes.

**Why:** Shell redirection prevented the command from matching the approved Cargo prefix, leaving it in a sandbox with read-only `/tmp`.

**Prevention:** Run full Cargo checks directly through the approved prefix; do not add redirection that changes sandbox treatment.

**Fix:** Restart the suite directly and retain its summarized results without redirecting its output.

## Integrated tests exposed graph lease and status regressions

**What happened:** The direct all-target suite failed two graph-reconciler tests and one status collector activity test.
**Why:** The new lease and status behavior had not yet been reconciled with the full existing test suite.
**Prevention:** Delegate integrated failures with the exact failing tests, preserve their meaningful assertions, and verify the underlying behavior before changing expectations.
**Fix:** Investigate the lease and status failures independently and rerun focused and full verification.

## Hook asset changes broke install checks

**What happened:** The integrated Rust suite failed embedded hook validation and hook installation tests after the shared Git command helper was added.
**Why:** The integration of the hook helper with asset installation or existing hook contracts was incomplete.
**Prevention:** Run installed-asset and hook policy tests whenever adding a sourced hook library; preserve existing guard behavior.
**Fix:** Investigate the exact install failures, repair the shared asset boundary and rerun the focused and full suites.

## Integrated capsule and amendment tests failed

**What happened:** The all-target suite reported a native capsule deny test and an adjudication amendment retry test failing.
**Why:** The respective integrated behavior and existing fixture expectations need reconciliation; exact panic details are still pending from libtest.
**Prevention:** Trace the failing scenarios and preserve behavioral assertions rather than changing expectations without evidence.
**Fix:** Assign separate fresh workers to investigate and rerun focused and full verification after repair.

## Plan-session drain failed during integrated suite

**What happened:** The all-target suite reported the summary-session drain regression failing during the integration run.
**Why:** The cause is not yet known; the full test output will provide the assertion after the library target finishes.
**Prevention:** Investigate the exact failure and distinguish a behavior regression from a timing-sensitive fixture before changing the test.
**Fix:** Assign a fresh integration worker and rerun focused plus full verification after diagnosis.

## Qualified lifecycle regression failed in integration

**What happened:** The all-target suite failed the new plugin-qualified agent stop regression.
**Why:** The exact lifecycle mismatch has not yet been identified; the library target has not emitted its panic summary.
**Prevention:** Verify start/stop identity and evidence end to end before marking qualified-agent handling complete.
**Fix:** Assign a fresh integration worker to diagnose the scenario and preserve the terminal-evidence assertions.

## Session drain returned before its ended event was published

**What happened:** A plan-session drain observed an inactive session before its queued Ended event reached listeners, producing an intermittent missing terminal status.
**Why:** Session removal and event publication occurred across an unlocked flush boundary.
**Prevention:** Define drain completion to include queued and in-flight event publication, subject to the existing bounded deadline.
**Fix:** Recheck publication state before completing the drain and cover the interleaving deterministically.

## Shared hook helper broke isolated integration fixtures

**What happened:** The integration target reported widespread commit-filter failures after the shared Git command helper was introduced.
**Why:** The installed and isolated fixture dependency boundaries need reconciliation; the precise panic summaries are pending.
**Prevention:** Verify both installed hook bundles and standalone integration fixtures whenever a sourced helper is introduced.
**Fix:** Assign a fresh integration worker to repair the dependency boundary without weakening hook assertions.

## New terminal fixture also bypassed the spawn guard

**What happened:** The full integration suite again reported a sanctioned-spawn guard failure after additional test files were added.
**Why:** The earlier fix covered one CLI fixture, but the rest of the worker wave had not been audited for direct Loom spawns.
**Prevention:** Scan every new integration fixture for binary spawns before running the full guard, and use the shared helper.
**Fix:** Assign a fresh worker to reconcile all implicated new fixtures and rerun the guard.

## Final wave exceeded maintainability limits

**What happened:** The full integration run reported numerous new file and function size violations across the worker wave.
**Why:** Several workers returned without verifying the repository limits after the combined edits and formatting.
**Prevention:** Check file and function sizes during each handoff and run the maintainability target before the full suite.
**Fix:** Assign a fresh worker to split cohesive helpers without changing the baseline or weakening tests, then rerun the gate.

## Pressure pipeline contract target failed

**What happened:** The all-target run reported a failure in the pressure_pipeline_contracts target after the pressure placement changes.
**Why:** The detailed assertion was truncated from the central output and requires a focused rerun.
**Prevention:** Inspect exact contract failures and preserve their behavioral coverage before changing implementation or fixtures.
**Fix:** Assign a fresh integration worker to rerun and repair the pressure contract, then repeat the full suite.

## Generic patch context hit the wrong clean helper

**What happened:** A maintainability refactor hunk matched run_bare_clean instead of confirm_branch_deletion_listing.
**Why:** The patch used shared nearby text rather than the function signature as its anchor.
**Prevention:** Anchor repetitive-file edits on the exact function and inspect the resulting diff immediately.
**Fix:** Corrected the hunk before running verification.

## Clippy exposed multiple combined-wave warnings

**What happened:** The all-target Clippy gate failed with 22 errors after the worker wave, including needless borrows, oversized argument lists, iterator ordering and documentation formatting.
**Why:** Individually edited files were integrated before running the shared warnings gate.
**Prevention:** Run Clippy on the combined tree before the final all-target suite and fix warnings without broad allows.
**Fix:** Assign a fresh integration worker to resolve the reported warnings, then rerun formatting, maintainability and the full gate.

## Lint repair exceeded a file limit and left formatting

**What happened:** After Clippy passed, the maintainability gate found recover_hung.rs at 401 lines and formatting found an import-order difference.
**Why:** The lint fix did not rerun both structural and formatting gates after its final edits.
**Prevention:** Run formatting and maintainability after any lint-driven structural change, before the full suite.
**Fix:** Reduce the file below the limit without changing behavior and format the final tree.

## New shell helper had trailing whitespace

**What happened:** The staged diff check found an extra blank line at the end of a new hook helper.
**Why:** The final shell handoff was not checked with the staged diff before commit.
**Prevention:** Run a staged whitespace check for every commit group, including new shell files.
**Fix:** Removed the extra line and restaged the helper.

## Web build rewrote tracked distribution files

**What happened:** The web production build changed tracked distribution assets even though this review did not edit web source.
**Why:** The verification command writes its output into the tracked dist directory.
**Prevention:** Check tracked generated outputs after builds and retain only intentional product changes.
**Fix:** Restore the previously clean distribution assets to their recorded contents; keep the build result as verification evidence.

## Patch matched the wrong similarly shaped function

**What happened:** An ownership refactor inserted a wrapper into the wrong adjacent function, leaving unbound variables.

**Why:** The patch context matched a generic function ending shared by multiple signatures.

**Prevention:** Anchor edits on the complete unique function signature and inspect adjacent wrappers immediately.

**Fix:** Correct the wrappers before compilation.

## Project detection tests require external writable temp

**What happened:** Running the compiled project-detection tests in a sandbox failed because /tmp was read-only; placing TMPDIR inside the repository made fixtures discover the enclosing checkout and fail unrelated assertions.

**Why:** These tests inspect ancestor Git markers, so their temporary root is part of the test semantics.

**Prevention:** Run project-discovery tests with writable temporary storage outside the checkout, using the approved Cargo test environment.

**Fix:** Rerun the focused tests in that environment and report the initial fixture failures separately.

## An Integration-Verify Gate Started Before the Fix Round Is Voided by It

**What happened:** an integration-verify host job runs in the live stage worktree and is bound to its tree digest (`host_verify/child.rs`). The gate was started before the review fix round; the fix worker's edits voided it ("the tree does not match the request"), and the job's `cargo test` held the build-directory lock the worker needed. A `loom check` on a changed tree supersedes (cancels) the running job, the only way to stop it.

**Why:** the gate and the review round were treated as parallel work; the gate reads the tree the fix round is about to change.

**Prevention:** run the review round first and request the full gate once the fix round is done. `loom check --criterion <n>` takes the 0-based criterion index.

## A Failure Seen Only in the Session Sandbox Is Settled by the Host Job

**What happened:** six `inbox_drain` tests (`tests_merge::merge_resolved_*`, `tests_merge_check::*`) failed in the session sandbox with and without the round's edits ("resolver content failed completion verification"). The host job's `cargo test --all-targets` passed all 10 criteria on the same tree. The criterion logs (`acceptance-<n>.log`) are not readable from the stage sandbox; only the `result.json` summaries under `.loom/work/verification/<stage>/<job>/` are.

**Prevention:** do not chase or edit around a failure that only a shell in the session sandbox shows; settle it with the gate's host job (`loom check`), and dispute a criterion only when the host job itself fails it. `cargo audit` cannot fetch its advisory database from the sandbox (network denied, `~/.cargo/advisory-db` read-only): record it as unverified, never as passed.

## The Gate Ran on a Tree the Next Edit Would Change (2026-11-14)

**What happened:** three stages ran `loom check` or `loom check integration-verify` at the wrong time.
(1) Beside the first review round, before the reviewer's findings were fixed, so the result described a
tree the fixes would change. (2) Calling the integration-verify gate while a fix worker was still
editing queued a new host job per changed tree, stacking full-suite runs behind the final one.
(3) A full integration-verify run on the final tree, seconds before the stage's completion command,
which runs every acceptance criterion itself (a user correction). (4) `loom check` through
`run_in_background`: the relay hook never saw its `LOOM_RELAY_V1` line and refused it; the next
foreground call recovered it.

**Why:** "run `loom check` beside the final review" was read as "beside any review" and as a step
separate from completion. The background-relay failure recurs from
[plan-server-delivery](plan-server-delivery.md).

**Prevention:** run the gate once, in the foreground, beside the re-review that follows the fix
round, after the fix worker has returned. After the last fix, re-check only the repaired criterion
(`loom check <stage-id> --criterion <n>`), or nothing when the fixer verified it, then commit and
complete the stage. A proposal for a check: have `loom check` refuse a submission while a queued
job for an older tree of the same stage is still pending, so a stacked run cannot start.

## A Wiring Entry Was Read From the Plan File, Not the Live Stage File (2026-11-15)

**What happened:** a fix swapped the `signals` re-export to `format_model_budget_section` to satisfy
`allocation-runtime` wiring[3] as the model-allocation plan words it. During human
review the operator had amended that entry in `.loom/work/stages/01-allocation-runtime.md:119-124` to
`pub(crate) use format::budget::current_model_budget;`, so the swap broke aggregated-wiring. A
dispute by wiring index could not name it either: `dispute-criteria --field wiring` indexes only the
filing stage's own list, so an aggregated failure of another stage's entry has no direct index.

**Why:** `amend` writes to the stage file, never to the plan file, and the daemon checks the stage
file.

**Prevention:** before changing code for a goal-backward or aggregated check, read the entry in
`.loom/work/stages/<NN>-<stage>.md`. The tree exports `current_model_budget`
(`signals/mod.rs:86`).

## Integration-verify disputed its own wiring for another stage's stale entry

**What happened:** an integration-verify fix moved a re-export, so allocation-runtime's wiring[3] failed the aggregated re-verification. The agent filed `dispute-criteria --field wiring --criterion-index 0`, which indexes integration-verify's OWN wiring list. The adjudicator judged that entry, found it passing (and trusted a green `loom check`, which never runs aggregated wiring), and rejected; the stage went to human review.

**Why:** dispute-criteria had no way to name another stage's list, and the adjudicator briefing presented `loom check` as authoritative for wiring.

**Prevention:** an aggregated wiring gap is disputed with `--owner-stage <stage>`, which the aggregated output prints with the `[wiring n]` index; the owner briefing tells the adjudicator `loom check` does not run these entries.

**Fix:** `CriterionTarget.owner_stage` through CLI, RPC, spool and admission; accepted verdicts amend the owner (`DisputeKind::amended_stage`). The operator repaired the live incident with `loom stage amend allocation-runtime --field wiring --op replace --index 3`.

## A cross-stage verdict reused the owner's amendment audit key

**What happened:** the first owner-stage dispute implementation keyed its amendment audit rows `<dispute>.<k>` against the OWNER stage. Dispute numbers are per filing stage, so integration-verify's dispute 1 collided with the owner's own dispute 1; `apply_amendment` treats a matching `(stage_id, dispute_id)` row as already applied and skipped the amendment.

**Why:** `plan/amendment/apply.rs` matches idempotency rows by the amended stage, while dispute ids are allocated per filing stage.

**Prevention:** a verdict that amends another stage keys its rows `<filing stage>:<dispute>.<k>` (`adjudication/plan_patch.rs::audit_key`). Owner-stage disputes also require the owner merged: amending an unmerged stage clears its `completed_commit` (`plan/amendment_fields.rs::sync_stage_from_definition`).

**Fix:** review round before commit; regression test `tests_owner_wiring.rs::an_owner_verdict_is_not_mistaken_for_the_owners_own_dispute_of_the_same_id`.

## Hook-Test Fixtures Inside the Verified Worktree Resolved the Real Checkout (2026-11-14)

**What happened:** integration-verify's hook criterion sets `TMPDIR=$PWD/target/tmp`, inside
`.worktrees/<id>`, so a fixture under TMPDIR sat in the live worktree. `loom_cwd_worktree` (a greedy
`.worktrees` regex) treated its cwd as that worktree, `git rev-parse` climbed from a fake `.git`
directory into the real checkout, and the `loom` binary resolved a nested fixture project to the
enclosing project's `.loom/work`. Separately, `_loom_git_no_repo` (`loom-hooks/_git-command.sh:217`)
set `GIT_CEILING_DIRECTORIES` to the scratch root, but git never applies a ceiling to its starting
directory, so a search starting at the scratch root climbed into any repository that held
`LOOM_SCRATCH_DIR` and refused a legitimate `mkdir $S/x && cd $S/x && git init && git commit`. A
fixture that creates a transient path inside the worktree can also fail the completion run with
"untracked completion input": `reject_untracked` (`handoff/completion/content.rs:57-71`) pairs
`git ls-files --others` with a separate ignored-output listing, so a path that appears or vanishes
between the two reads counts as untracked
(`worktree-isolation-traversal-resolve.sh:136` creates and removes a symlink under `target/tmp`).

**Why:** a fixture's location decided the project, repository and worktree the code under test saw.

**Prevention:** build a neutral fixture under `fixture_base` (`loom-hooks/tests/_fixture_base.sh`),
which moves a worktree-located base to `/tmp/claude-<uid>/loom-fixtures/<closed uuid>`; a fixture
repository is a real `git init`, never `mkdir .git`. The ceiling is the scratch root's parent when
the search starts at the root (regression case `commit-filter-scratch-repo.sh`, "scratch directory
inside another repository"). Run `loom-hooks/tests/run-all.sh` from a neutral copy when judging a
hook change: inside a stage worktree it fails six cases from ambient `LOOM_*` env and the commit
filter, and the same three cases fail at HEAD. Judge a host-only failure, such as
`cache_input_runner_tests::unreadable_and_over_budget_inputs_bypass` (a `chmod 000` input the agent
sandbox cannot reproduce), from the daemon's host run.

## Tool and Template Slips of One Line Each (2026-11-14)

- **`grep` instead of `rg`**, twice, once inside a `bash -c` block, where the prefer-modern-tools
  hook warned after the call ran. Use `rg -n` for every content search, including JSON manifests.
- **`git rm` staged a deletion.** Subagents delete with `rm` and leave the index alone; staging is
  the main agent's. `git restore --staged <file>` undoes it.
- **A no-op `Edit`** (`old_string` equal to `new_string`) written as a verification step. Verify
  with `rg`.
- **`AGENTS.md.template` failed MD046** at HEAD: three `loom memory note` lines sat in an indented
  code block under "Inside a loom stage". Fenced as `text`, wording unchanged. Run
  `markdownlint-cli2` on a template after editing it.
- **The contract signal said "No scaffold edits: every contract file is already collected"**, but
  the freeze refused until `signals/mod.rs` declared the `#[cfg(test)] mod`
  (`allocation_contract_tests`). Declare a contract test file in its parent `mod.rs` first
  ([code-quality-and-hook-debt](../concerns/code-quality-and-hook-debt.md)).

## A Clean Textual Merge Left a Re-export Imported Twice on macOS

**What happened:** merging `loom/native-macos-boundary` left `NetworkAccess` imported twice in `loom/src/verify/criteria/mod.rs` on macOS: one side had added an unconditional `pub(crate) use confine::host::NetworkAccess`, the other had added it to the `cfg(target_os = "macos")` block. The merged lib failed with E0252 and clippy and the tests failed at integration-verify.

**Why:** each side compiled alone and the textual merge was clean.

**Prevention:** after a stage merge that touches a re-export or `use` block, run `cargo check` on the merge commit before the next stage starts.

**Fix:** the macOS duplicate was dropped.

## A Fix Round Compiled on macOS Broke Both Linux Builds

**What happened:** the integration-verify fix round's opus worker added `private_root_rollouts.rs` importing a `cfg(any(macos, test))` constant, and a `StandIn::session_leader` used only from a macOS test module. Both compiled on macOS and macOS test builds and broke Linux non-test and Linux test clippy. Neither the worker nor the macOS gate could see it; the re-review caught it by reading the `cfg`s (F-1-1, F-1-2).

**Why:** no usable Linux target on the host (the rustup Linux target lacks `core` for the active toolchain, so `cargo clippy --target x86_64-unknown-linux-gnu` fails with E0463).

**Prevention:** a brief for cross-platform Rust requires a reachability table per `cfg` (macOS, macOS test, Linux, Linux test) for every new or moved item whenever Linux cannot be compiled locally; the hosted Linux jobs are the first compile of those paths.

**Fix:** the re-review round named both items and the worker gated them to the builds that use them.
