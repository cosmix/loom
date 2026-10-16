# Refactor Stragglers

> What a rename leaves behind

## Source vs Installed: Editing Wrong File

**Mistake:** Edited `~/.claude/hooks/loom/` (installed copy) instead of `loom-hooks/` (source). Lost on reinstall.
**Fix:** Always edit in project's `loom-hooks/` directory.

## Module Refactoring: Duplicate Files

**Mistake:** Splitting `tests.rs` into `tests/mod.rs` without deleting original caused E0761.
**Fix:** When refactoring `foo.rs` to `foo/mod.rs`, DELETE the original file.

## Stale References After Field Removal

**What happened:** After removing truths/truth_checks fields, stale references remained in comments (complete.rs:393), e2e test fixtures (plans.rs), README.md, skill files, and knowledge files.

**How to avoid:** When removing a struct field, grep the ENTIRE project (not just src/) for references. Include: tests/, doc/, skills/, README, knowledge files, comments, YAML fixtures.

## Stale Documentation After Adding Enum Variants (2026-04-16)

**What happened:** After adding KnowledgeDistill as the 4th StageType variant, three stale references remained: entry-points.md said 3 variants (should be 4), SKILL.md said Integration Verify Stage (Last) (now second-to-last), and sections.rs comment said integration-verify only (code had moved to KnowledgeDistill block).

**Why:** The implementation stage focused on Rust code changes and missed docs/comments that reference counts or ordering.

**Prevention:** When adding a new enum variant that changes ordering or counts, search all knowledge files for old counts, search skills for ordering claims, and search source comments for stale stage-type references.

## Parallel Deletion Stages: Straggler Files Outside Assignment Tables

**What happened:** After a parallel subagent deletion stage (`remove-container-keep-scaffolding`), 7 files remained with stale container references because they were not assigned to any subagent: `commands/mod.rs`, `completions/dynamic/tests.rs`, `plan/schema/mod.rs`, `commands/handoff/create.rs`, `commands/stage/tests/session.rs`, and an orchestrator preflight-checks module (since removed from the tree entirely in a later refactor). These caused compile failures discovered only at integration-verify.

**Why:** Parallel subagent deletion scopes by files owned — files that re-export, import, or reference the deleted code but weren't explicitly in the ownership table are silently missed. Test files (`#[cfg(test)]`) are especially prone since `cargo build` doesn't compile them.

**Prevention:**

- After any parallel deletion stage, the MAIN AGENT must run `cargo build && cargo test --no-run` (not just `cargo build`) — test-only files don't appear in a lib build.
- Before assigning subagents, run `rg` for the target symbols across the ENTIRE tree including `tests/`, `mod.rs` re-exports, and completions.

## Struct Field Removal: Straggler Initializers Across Workspace

**What happened:** Removing a struct field (e.g., removing the `execution` field from `LoomConfig`) left ~25 straggler struct literal initializers across test fixtures, core modules, and examples. Each was an explicit `execution: None` / `execution_backend: None` / `backend: Default::default()` that subagents missed because they only searched within their assigned file set.

**Why:** Rust requires all struct fields in literals unless `..Default::default()` spread is used. In a workspace with many test fixtures, explicit literals far outnumber `Default` spreads.

**Prevention:** After removing a struct field, the main agent MUST `rg` the WHOLE tree (including `tests/`) for `<field_name>:` before considering the work done. Do not rely on per-subagent grep scoped to owned files.

**Fix:** Used `..LoomConfig::default()` spread in all new struct literals going forward.

## Stale Code Comments After Large Structural Removals

**What happened:** The container backend removal (`remove-container-keep-scaffolding` + `collapse-backend-scaffolding`) correctly deleted code but left stale references in comments across 7+ files: `monitor/{core,handlers,detection}.rs` referenced `dispatcher`, `daemon/server/client.rs` had admin-token rationale citing containers, `commands/stage/{complete,knowledge_complete}.rs` had isolated-git/container comments. These were caught only at `integration-verify`.

**Why:** The stage that owned doc cleanup (`strip-container-docs`) ran `rg` for identifiers and string literals but did not search comments or table cells. Comments describing removed concepts stay syntactically valid and compile fine.

**Prevention:** A stage that owns cleanup of a removed concept must `rg` the whole tree for:

1. Identifier names (already done)
2. Human-readable name/framing in comments and docstrings (often missed)
3. Table cells in markdown files, knowledge docs, and SKILL.md files

Use `rg -i "container\|docker\|dispatcher" loom/src/ --include="*.rs"` to catch all forms.

## A Subagent That Splits a File Leaves an Untracked Straggler No Local Check Catches (2026-09-04)

**What happened:** a stage nearly shipped a branch whose committed tree could not compile.
The first commit was staged with an EXPLICIT file list taken from the subagent's report, but
the subagent had split `loom/src/commands/status/web/connection.rs` and created
`loom/src/commands/status/web/head.rs`, which its report never named.
`loom/src/commands/status/web/mod.rs` (committed) carried `mod head;` while the split-off
file stayed untracked — HEAD referenced a file not in git. `cargo build`, clippy, the full
test suite, the smoke script and the pre-commit hook all read the WORKING TREE, where the
file exists, so all of them passed; only `git status --short` showed the untracked `??` line.

**Prevention:** after the last commit of a stage, `git status --short` must be EMPTY — an
untracked `??` line under a source directory is a straggler, not noise. Never derive a
staging list from a subagent's report alone; derive it from `git status`. Stronger check
worth the ~70 seconds: `git clone --no-hardlinks --branch <branch> . $TMPDIR/x && cargo build
--all-targets` — the only thing that tests what the COMMIT contains rather than the disk.

**Fix:** `git reset --soft` to the pre-stage commit, then re-commit the same groups with the
missing file included — content was never at risk since a soft reset keeps the working tree.

## Registering a New Global PreToolUse Hook Has a Fourth Site Beyond the Documented Three

**What happened:** adding a global `PreToolUse` hook has a documented three-site checklist
(`fs/permissions/tests/constants_tests.rs:71`), but `fs/permissions/tests/hooks_tests.rs:32` also
hardcodes `assert_eq!(pre_tool.len(), N)` over the whole `pre_tool_hooks` list. Miss the fourth site
and the new hook compiles, installs, and runs correctly — the only symptom is
`test_hooks_config_structure` failing on a bare count mismatch that names no hook.

**Prevention:** when adding or removing a global hook, grep for every hardcoded count of
`pre_tool_hooks`/`post_tool_use`/similar lists, not just the documented checklist — a passing count
assertion elsewhere is a silent fourth site.

## A Codex-Split Extraction Left a Dead Wrapper Only Clippy Caught

**What happened:** codex unit w2b extracted `dispatch_key`/`dispatch_edit_key` as free functions out
of `ConfigTui::handle_edit_key`, but left `handle_edit_key` itself in place as a now-unused wrapper
(`loom/src/commands/config/tui/mod.rs:116`). The scoped `cargo test` the unit ran passed cleanly;
only `cargo clippy --all-targets -D warnings` (`dead_code`) caught the stale method.

**Prevention:** when a brief says "extract X into free functions", grep for the old method
afterward — a passing scoped test suite proves the new code works, not that the old entry point is
gone. `cargo clippy --all-targets -D warnings` over the full tree remains the orchestrator's job,
never a subagent's, and it is the check that actually finds this class of straggler.

## Removing a Now-Unused Import Can Break the `#[path]` Test Module That Relied On It (2026-09-25)

**What happened:** a subagent moved `dispute_kinds.rs`'s integrity lookup to a shared
`observer::integrity_events` call and, since nothing in the module's own code needed `WorkDir`
anymore, dropped `use crate::fs::work_dir::WorkDir;` from its import list. `dispute_kinds_tests.rs`
is that module's `#[path]` test child (`#[path = "dispute_kinds_tests.rs"] mod tests;`) and reaches
every sibling name through `use super::*;` rather than its own explicit imports — including
`WorkDir`, which its own test bodies construct directly. The test build broke.

**Why:** a `#[path]` test module's `use super::*` makes it invisible to a plain grep for
`WorkDir` inside the test file itself, and to a reviewer scanning only the parent module's own
uses; the name is live only through the glob re-export, one file away.

**Prevention:** before removing a `use` from a module that has a `#[path = "..._tests.rs"]` child,
`rg` the test file for the name. If it appears, the test file needs its own explicit `use` for it
before the parent's import can go.

**Fix:** `dispute_kinds_tests.rs` gained its own `use crate::fs::work_dir::WorkDir;`
(`daemon/server/dispute_kinds_tests.rs:11`), independent of whatever `dispute_kinds.rs` imports.

## A Scripted Rewrite Dropped a File's Test Module

**What happened:** a subagent rewrote `loom/src/context/untrusted.rs` with a Python script to factor a shared helper; the script truncated the file and dropped its existing `#[cfg(test)] mod tests`. The subagent noticed and restored the tests from `git show HEAD`.

**Why:** a whole-file scripted rewrite replaced content by position, and nothing checked what followed the edited region.

**Prevention:** prefer the Edit tool for source changes; after any scripted edit, compare the file's `#[test]` count and tail against `git show HEAD:<path>` before moving on.

**Fix:** tests restored in the same change; the count went from 2 at HEAD to 5 (three new).

## A Removed Feature's Doc Comment Steered a Design Question

**What happened:** While extending `DetectedLanguage` with seven languages, the agent asked the operator how each new language should appear in container image tags, because the doc comment of `DetectedLanguage::canonical_name` (`loom/src/language.rs`) said it fed image tags and fingerprint prefixes. The container backend that read it had been deleted in `3aa78105`; nothing outside the method's own test called it. The operator had to ask three times what the question meant.

**Why:** The doc comment was trusted as a description of a live consumer without checking callers. Removal stages delete callers and leave the callee's doc comment describing them.

**Prevention:** Before raising a design question, or extending a method, on the strength of a doc comment that names a consumer, confirm the consumer exists: `loom map --impact <symbol>`, then `rg -n '<symbol>\(' loom/src` when the graph reports no resolved edge. When the operator asks what a question means, answer that first and in full before reporting other progress.

**Fix:** `canonical_name`, its test and its doc comment were deleted. A future backend that needs a short language id can use the source graph's dialect ids (`context/extract/dialect.rs::DIALECTS`).

## A Rate-Formula Change Broke Exact-Value Assertions the Plan Did Not List (2026-10-15)

**What happened:** token-core replaced the fixed 300 s rate window with a span-based one (`flow_build.rs` `rate`). The plan's test-integrity list pinned only `token_flow_contracts.rs` and `flow_build_tests.rs`; `loom/tests/token_flow_bounds_contracts.rs:17` (`unix_now`) and `tracker_tests.rs` `two_stages_sum_into_the_plan_and_decay` also assert exact rates and broke about 1 run in 30, depending on where `now` fell inside a bucket.

**Why:** an exact-rate assertion that reads `unix_now()` depends on the bucket phase, and a search for the old symbol finds none of them.

**Prevention:** when a rate formula changes, `rg` every exact-rate assertion and every fixture that uses `unix_now()`, not only the files the plan lists, and run each such test repeatedly.

## Interrupted extraction leaves module wiring unfinished

**What happened:** Resuming a broad repair after interruption found extracted modules with missing imports and a trailing doc comment unattached to an item.

**Why:** Files persisted before their integration and verification handoffs completed.

**Prevention:** Treat persisted edits as unfinished until the complete tree compiles, and include module declarations, imports, and adjacent documentation in extraction reviews.

**Fix:** Reassign ownership from the actual tree and compiler diagnostics, finish the moves, then run the full gate before committing.

## Shared repair seams need fault-path review

**What happened:** Integration review found a spool receipt written after its disk effect, a source stream loop that did not retry interrupted reads, and a child supervisor that could accept incomplete pipe retirement.

**Why:** Extracting shared mechanisms preserved happy paths while exposing previously implicit error and ownership assumptions.

**Prevention:** Review failure before and after each persistent effect and require EOF, reaping, and stable request identity as separate invariants.

**Fix:** Reserve dispute identity before request-file creation, persist effect receipts with stage updates, retry Interrupted reads, and fail closed on retained pipes. Keep compiler locks in private scratch while sharing dependency data read-only.

## Completion snapshot validation reacquired its own stage lock

**What happened:** Completion update closures called VerificationSnapshot::validate, which reloads the stage with locked_read while update_stage already holds the exclusive directory lock. A standalone content test also briefly imported an unused inaccessible merge helper.

**Why:** Extracted validation hid a lock acquisition; a copied import outlived its use.

**Prevention:** Validate the authoritative object passed into a locked closure directly and keep fixture imports local to actual calls.

**Fix:** B09 adds validate_locked(current, work_dir) for completion and knowledge-completion update closures and removes the unused test import. Integration verification remains separate from this repair record.

## Completion commit identity passed as a close reason

**What happened:** The interrupted daemon completion repair passed the verified commit to Stage::try_complete, whose argument is a close reason. completed_commit remained empty, so the new landing guard rejected broker-completed stages.

**Why:** Two optional string fields were confused during integration.

**Prevention:** Trace mutator argument semantics and assert the persisted completed_commit in end-to-end completion fixtures.

**Fix:** B09 explicitly sets completed_commit to the verified commit and calls try_complete(None); knowledge completion uses the same separation. Targeted verification remains required.

## Stage fixture mutation used a creation API

**What happened:** An inherited-confinement wrong-session fixture initially tried to overwrite a stage with save_stage, which is creation-only.

**Why:** The fixture reused a constructor helper for a later mutation.

**Prevention:** Use update_stage for changes to persisted stage records, including tests.

**Fix:** The final confinement worker changed the fixture before verification; no test result is implied.

## Detector extraction left an unused stderr warning helper

**What happened:** verify/utils.rs retained bounded_stderr_warning and tests after both duplicate and wiring detectors stopped using it.

**Why:** The extraction moved consumers without removing their old shared helper or its ownership comment.

**Prevention:** Search production references after extraction; tests calling only an unused helper do not establish a live consumer.

**Fix:** The verification worker removes the dead helper and its helper-only tests before Clippy verification.

## Compiled repairs still failed strict lint

**What happened:** The final test-compilation gate passed, but strict Clippy found errors across inherited transport, content guards, wiring tests, shared dispute handling and completion replay fixture borrows. Rustdoc also rejected two public links to the extracted private daemon-state helper.

**Why:** Compilation does not enforce unsafe-comment placement, needless lifetime/borrow cleanup, question-mark simplification or the internal argument-count limit or public documentation visibility.

**Prevention:** Keep strict Clippy separate from compilation evidence and rerun only after the reported code changes.

**Fix:** Assigned workers corrected their own diagnostics and refroze the tree; maintainability independently passed all eight checks without growing its allowance ledger.

## Shared parsers need real source and provider forms

**What happened:** Runtime regressions found that external Rust mod declarations did not produce import bindings, and nullable optional provider cache counts were rejected.

**Why:** Extracted shared parsers omitted input forms that real consumers accept; detector-specific workarounds would leave consumers inconsistent.

**Prevention:** Fix the common source/provider interpretation and keep fixtures for external module declarations and explicit null optionals.

**Fix:** The verifier adds real external-module import bindings in the Rust source extractor with extractor revision5, and treats optional null cache counts as absent in the shared provider parser. A confinement fixture executable also moves inside its permitted tree while staying outside fingerprinted inputs.

## Runtime fixtures must preserve authoritative state invariants

**What happened:** Full runtime tests exposed fixtures without required persisted graph stages, assertions expecting malformed-stage omission or blocked-stage requeue, and a dependency-output check using a knowledge brief that intentionally does not render those outputs. Follow-up verification also exposed merge and progressive-completion fixtures invoking landing without recorded completion proof, including control-path enforcement and bypass cases.

**Why:** Fixtures encoded superseded lifecycle assumptions or exercised the wrong consumer surface.

**Prevention:** Construct the persisted state that production requires and assert strict refusal, preserved operator intent and the real output-rendering path. Keep safety side-effect assertions and each test's intended negative guard when adapting fixture prerequisites.

**Fix:** Persist required graph records and verified fixture commits; retain malformed-state refusal, durable blocks, control-path enforce/bypass assertions and no-follow/limit plus untouched-victim/no-partial-write spool checks. Use an ordinary downstream brief for output clearing. Resolver regressions reject a newer ordinary tip and a dirty resolver tree while requiring fresh gates for resolved content. Rerun the full suite after fixture corrections.

## Completion regressions must respect stage-store creation semantics

**What happened:** New resolver-boundary fixtures called save_stage for records already created, causing duplicate-record failures. Final verification repeated that mistake in a multi-status missing-proof table (`Stage already exists: api`) and built a recovery test with an empty graph (`Unexpected persisted stage: s`); both failed before reaching their intended assertions.

**Why:** The fixtures treated a create-only API as an upsert or omitted the authoritative persisted graph definition. Related completion cases omitted committed initialized knowledge inputs. A broad replacement also populated proof in an intentional missing-proof negative fixture.

**Prevention:** Scope fixture edits by test function and preserve negative setup conditions. Use update_stage for existing records or isolated stores for table cases; establish the same committed checkout, graph and initialized inputs required by production.

**Fix:** Use fresh stores for each missing-proof state and seed the recovery graph from the persisted stage definition. Preserve strict proof, unchanged-target and dirty-input assertions; missing original proof stays fail-closed and routes to review.

## Removing a mutating helper can leave another mutable consumer

**What happened:** The sweep follow-up removed proof capture and also removed mut from try_auto_merge stage binding, but apply_auto_merge_outcome still borrows that stage mutably. Compilation caught E0596 before runtime checks. The first memory entry guessed the obsolete import before reading its diagnostic.

**Why:** The edit considered the removed consumer without tracing the remaining one; the coordinator recorded an inferred import name as fact.

**Prevention:** Inspect all uses before changing binding mutability, and read exact diagnostics before naming fixes.

**Fix:** Restore the mutable binding needed by outcome application and remove the obsolete Persistence import in merge_gate.rs.

## Integration must check the size ledger after adding guards

**What happened:** The missing-proof guard grew merge_handler.rs from its ledgered 556 lines to 563 despite the repository no-growth rule.

**Why:** A small semantic fix was added to an already oversized coordinator file.

**Prevention:** Check file and function limits after formatting, and extract cohesive modules before verification.

**Fix:** Move automatic-merge handling and embedded tests into focused modules rather than expanding any size allowance.

## Ordered patch hunks and copied hook dependencies

**What happened:** Final verification encountered an out-of-order patch hunk and 171 failing integration tests whose copied hooks omitted `_bounded.sh`: codex_evidence (21), integration (140), worker_evidence (10). Adding it left an outdated dependency-count comment and pushed two test files past 400 lines. The next complete run exposed 25 skill-trigger cases failing `ModuleNotFoundError: _bounded_input`; the fixture repair had omitted the Python sibling module.

**Why:** Patch context was applied in source order. Fixture copy lists, comments and size limits had drifted from the production dependency graph, and checking only shell dependencies missed Python imports.

**Prevention:** Order hunks by source position, inspect partial application, and audit every copied entry point and its transitive shell/Python dependencies. Update fixture documentation and check file limits before the final run.

**Fix:** Complete only unapplied hunks; install `_bounded.sh` at ten fixture sites and `_bounded_input.py` at both skill-trigger installers. Correct the comment and extract test groups without growing allowances. Audit every HOOK_SKILL_TRIGGER copy site and rerun static checks plus exhaustive tests after the finished source.

## Multi-file prose patches need exact source anchors

**What happened:** a guidance patch matched the worker file but failed on a senior-agent block whose source lines were copied inaccurately.

**Why:** a large deletion used reconstructed context instead of exact boundaries.

**Prevention:** anchor prose rewrites on verified headings and check which earlier files changed after a multi-file patch failure.

**Fix:** inspect the diff, then replace only the remaining exact sections.
