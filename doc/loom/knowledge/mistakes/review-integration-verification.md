# Review Integration Verification
>
> Integrated review failures

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
