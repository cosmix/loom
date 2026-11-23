# Maintainability Baseline

>

## Cargo stops at the first failing test target and hides later ones

**What happened:** a subagent ran `cargo test`, hit a failure in `tests/integration`, reran only that target with `--no-fail-fast`, and reported one failure. `tests/maintainability.rs` runs after it and was also failing on the change (a re-export pushed `fs/work_dir.rs` past its baseline line count, and a match grew `models_value_of` to 52 lines).
**Why:** cargo skips the remaining test binaries after the first failing one, so a partial rerun never reaches them.
**Prevention:** after any failure, run the whole suite again with `cargo test --no-fail-fast` and read every `test result` line before reporting.
**Fix:** shrink the offending code until it is back under the baseline in `tests/maintainability`, rather than adding a baseline entry.

## Preserve guard exceptions in review summaries

**What happened:** an intermediate review summary said the live file hook rejects all shared-state writes; the hook retains a deliberate handoffs exception. **Why:** the summary generalized before checking every exception. **Prevention:** trace the full guard's exceptions before describing policy divergence. **Fix:** the review uses the confirmed heartbeat/signals and parent-component differences and explicitly preserves the handoffs exception.

## Comprehensive audit scope must not imply exhaustive inspection

**What happened:** a five-agent repository review mapped broad territories and deeply reviewed hotspots, but left substantial areas sampled or unexamined; the user challenged whether the whole codebase had been reviewed. **Why:** territory assignment and many findings do not establish file-by-file coverage. **Prevention:** distinguish a broad audit from exhaustive source inspection and keep a coverage ledger with explicit gaps. **Fix:** state that this pass is not whole-codebase coverage, preserve that limitation prominently in the report, and expand review into uncovered areas rather than claiming completion of exhaustive inspection.

## Planner review must account for active development

**What happened:** the user clarified that the planner feature is still under development during the repository-wide review. **Why:** review priorities must distinguish unfinished planned work from defects in implemented behavior. **Prevention:** label planner findings as development-stage observations; do not treat missing planned features, temporary assembly seams, or evolving interfaces as production regressions without evidence. **Fix:** continue reviewing implemented planner code, retain concrete correctness risks and actionable simplifications, and qualify maturity-sensitive architectural recommendations and priority separately from the established runtime.

## Planner implementation paused by provider quota

**What happened:** the user clarified that the planner implementation plan is stalled because of Claude Code quotas. **Why:** incomplete implementation reflects an interrupted development plan, not necessarily abandoned architecture or omitted requirements. **Prevention:** review current behavior separately from unfinished planned work and avoid recommending competing refactors before checking the active plan. **Fix:** annotate planner findings with this context, identify overlaps with remaining plan work when available, and keep the task limited to review unless implementation or plan resumption is requested.

## Exclude planner from the repository review

**What happened:** the user explicitly excluded the in-development planner from this audit after explaining its plan is stalled by Claude Code quotas. **Why:** reviewing unfinished planner work would distract from the remaining functionality. **Prevention:** scope manifests, findings and completeness claims must exclude planner-owned code while retaining established dashboard and shared-runtime behavior. **Fix:** stop planner-only review, remove planner-only findings from the deliverable, and reassign that reviewer to non-planner dashboard/shared web code and remaining coverage.

## Reconcile curated concerns with the source review

**What happened:** the user requested two additional Astra/high reviewers to inspect the curated concerns and reconcile relevant issues with the code-review report. **Why:** known concerns can reveal coverage gaps and additional supported findings, but may be stale or already fixed. **Prevention:** trace each concern against current code, distinguish confirmed/open from fixed/stale/unproven, and link duplicates to existing finding IDs. **Fix:** add two concern-reconciliation reviewers while retaining the planner exclusion and the original five reviewers' source-coverage work.

## Bound assembled review documents before publication

**What happened:** adding completed review sections briefly expanded the assembled report to 410 physical lines, beyond the repository's 400-line limit. **Why:** the assembler compacted finding headings but did not enforce a final bound after all sections. **Prevention:** enforce the line bound before writing the consolidated artifact. **Fix:** compact additional subsection headings without removing finding detail and reject oversized output before publication.

## Derive review coverage counts from ledgers

**What happened:** an intermediate progress message double-counted three recently logged files as still pending. **Why:** a manual running total overlapped the persisted ledger. **Prevention:** compute unique reviewed-path totals from coverage ledgers and report pending reads separately. **Fix:** the reviewer corrected the pending count from seven to four; final coverage uses deduplicated paths.

## Trace live callers before carrying forward a concern

**What happened:** a preliminary concerns review inferred memory-command misattribution from a legacy helper, but current commands use shared state discovery. **Why:** helper behavior was inspected before confirming live callers. **Prevention:** trace the actual entry point and consumer before marking an old concern current. **Fix:** the claim was rejected as a finding and its knowledge section updated to the shared discovery path.

## Follow callees before denying cache invalidation

**What happened:** an early review incorrectly changed knowledge to say ordinary `publish_base` does not invalidate views. The expanded read found successful publication calls `prune_after_publish`, which calls `remove_views`; only memory fallback skips that path. **Why:** the first trace stopped at the publishing method instead of following its pruning call. **Prevention:** trace side-effecting callees before making a negative behavioral claim. **Fix:** restore the successful-publication invalidation contract and distinguish denied-write fallback and explicit replacement invalidation.

## Exclude superseded scope ledgers from coverage totals

**What happened:** a draft assembly counted the abandoned initial planner coverage ledger in its aggregate after planner exclusion. **Why:** the aggregator globbed every coverage TSV instead of selecting active reviewer scope. **Prevention:** derive counts from active scope manifests and explicit exclusions, and omit superseded planner-only ledgers. **Fix:** the final aggregator excludes the initial planner ledger; planner-only entries in active scopes remain explicitly excluded.

## Validate merge authorization at production callers

**What happened:** review QA narrowed a merge-parent concern: production commit callers always supply the target branch, and plan deny-write paths have an actual merge-result guard. **Why:** the preliminary finding treated optional helper inputs and an ancestor-content shortcut as the whole admission path. **Prevention:** verify caller-supplied arguments and every downstream guard before alleging an authorization gap. **Fix:** remove the missing-target allegation and limit the remaining scenario to built-in protected/gitlink entries outside plan deny-write globs, preserving legitimate target movement.

## Distinguish operator bootstrap from stage confinement

**What happened:** review QA found that an uncapsuled knowledge-bootstrap launch is explicitly operator-owned and refused inside a stage. **Why:** a preliminary concern applied stage confinement requirements to a separate interactive operator workflow. **Prevention:** trace launch mode and admission guards before treating different confinement as a bypass. **Fix:** retain the observation only as an optional launch-policy/notice decision, not an established sandbox escape.

## Conserve quota during review closeout

**What happened:** the user reported only 11% of the weekly quota remains and asked what work is left. **Why:** broad source inspection and concerns reconciliation have substantial cost. **Prevention:** report exact remaining coverage, stop optional expansion and extra QA, and prioritize assembling completed evidence. **Fix:** close finished reviewers, request concise remaining-scope counts, and complete only the already assigned audit and report reconciliation.

## Test-only callers missed during context snapshot changes

**What happened:** A context test build failed because a changed internal function signature missed a test-only caller and a new test lacked an import.

**Why:** The implementation changes were compiled before checking all test-module call sites and imports.

**Prevention:** Search for every caller, including test modules, after changing internal signatures and verify new tests have the required imports.

**Fix:** Updated the test caller and imported the context store before rerunning the focused tests.

## Base identity validation missed a fixture

**What happened:** A graph test failed after base-layer revision validation because its fixture published a layer with an empty embedded revision under a nonempty key.

**Why:** The fixture scan focused on overlay revisions and missed a base fixture using default values.

**Prevention:** When enforcing stored identity invariants, inspect both producer and consumer fixtures for default or contradictory identifiers.

**Fix:** Give the fixture the matching base revision and rerun the context tests.

## Runtime slice coercion in a const table

**What happened:** A shared manifest marker table failed to compile because it used range indexing on an array in a const initializer.

**Why:** A runtime slice-coercion idiom was copied into a constant without checking the stable const rules.

**Prevention:** Use direct array-to-slice coercion in typed const tables and compile before reporting the implementation stable.

**Fix:** Replace the range expression with a direct borrowed array and rerun the focused language and project tests.

## Subagent targeted tests missed the file-size guard

**What happened:** During the gpt-6.2 codex model rename, a subagent split a helper out of `loom/src/pressure/codex_run.rs` and left it at 405 lines. Its targeted `cargo test` filters passed, and the full suite then failed `tests/maintainability.rs` on the 400-line limit.
**Why:** The `maintainability` integration test runs only in the full suite, and the brief did not tell the subagent to check file length on files it grew.
**Prevention:** Briefs that add code to an existing file include `cargo test --test maintainability` among the targeted tests.
**Fix:** Compacted `chain_label` back under 400 lines.

## Growing a Function That Carries a Maintainability Baseline Entry Breaks the Gate (2026-08-10)

**What happened:** A five-line fix inside `generate_index` (`src/fs/knowledge/index.rs`) pushed it from 53 to 58 lines and failed `cargo test --test maintainability`, blocking the push.

**Why:** `maintainability-baseline.txt` records exact line counts for every function already over the 50-line limit. The ledger is a debt list, not a budget — entries may shrink but never grow, and an entry that no longer corresponds to a violation is rejected as stale.

**Also check the FILE entry, not just the function.** The ledger records both, so a change that keeps every function small can still fail on the file total — and four of them can fail at once, as the codex sandbox work did (`repair.rs`, `fs/permissions/settings.rs`, `sandbox/settings.rs`, plus three functions). The move that satisfies the ledger honestly is extraction: lift the new code into a new module (then the `codex_sandbox` module, since deleted, and `commands/repair/settings_checks.rs`), which carries no entry at all while it stays under 400 lines, then lower the now-smaller entries to their measured values. Growth is never recordable; shrinkage must be recorded.

**Prevention:** before adding lines to a function, `rg '<fn name>' loom/maintainability-baseline.txt`. If it is listed, refactor rather than extend — and when the refactor drops it under the limit, DELETE the entry rather than lowering it. Prove behaviour is unchanged by regenerating the artifact and diffing (for INDEX.md: `loom knowledge sync` then `git status --porcelain`, expecting no change).

**Two more shapes, from wiring the viewer reconciler (2026-08-11):** (1) a cross-cutting per-tick
call could not live in `orchestrator.rs` (file ledgered at 564) NOR in `event_handler.rs` (468) —
even a one-line addition to a ledgered FILE fails the gate, so a new hook must find an unledgered
host with headroom (`Monitor::poll` in `monitor/core.rs`, 128 lines, same per-tick semantics).
Check the file ledger BEFORE choosing a call site, not after the test fails. (2) MOVING a
grandfathered over-limit function is fine: the ledger accepts deleting the entry at the old path
and re-adding it, same measured size, at the new path (alphabetical within the `function` group).
Relocated debt is not new debt — but only new-code violations must be refactored instead of
ledgered.

## Ledger Growth Reached Push Time Again — Gates Moved Earlier (2026-08-14)

**What happened:** the maintainability failure class recurred a third time: a guidance commit (`deecb23e`) duplicated a 13-line block into two ledgered `signals/` functions and grew four tests, and the breakage was only discovered when the pre-push hook ran the suite. The truncated-`cargo test` trap above is exactly how it went unnoticed at commit time.

**Fix (mechanical, both hooks in `loom/.githooks/`):**

- `pre-commit` now runs `cargo test --quiet --test maintainability` after formatting — ledger growth blocks the commit itself, not the eventual push. Fast when the build is warm; the first commit in a cold worktree pays a compile.
- `pre-push` now mirrors CI: `cargo clippy --all-targets -- -D warnings`, the rustdoc gate, and `cargo test --all-targets --no-fail-fast`. Since 2026-09-05 it also runs `scripts/flake-check.sh` (the CI flake job) last and warns when `rustup check` reports a newer stable.

**Prevention:** when a failure class recurs at push time, the fix is to move its cheapest sufficient check to commit time, not to write another reminder. The ledger fix itself followed the standard shape: extract the duplicated block into an unledgered module (`signals/helpers.rs::append_settled_completion_rules`), never raise a ledger entry.
