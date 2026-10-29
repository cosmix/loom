# Plan: Doctrine Truth and Plan Guards

## Overview

Agents spend turns, and sometimes whole corrections, on instructions that are false or contradict each other, and plans lose runs and lessons through gaps in verification and cleanup. This plan is plan 3 of the token-optimization programme (`doc/plans/optim-token-metaplan.md`, section 3). One implementation stage corrects the doctrine in the signal, the hooks, the template, the README and two skills; deletes the dead memory-summary path; corrects the knowledge scaffold sentence; proves that a section marked non-current leaves retrieval; and adds four plan guards: the bootstrap warning fires only for a missing or scaffold-only knowledge tree, two version-2 lints require the distill and verify bookends, the stage `model` field is validated against the `[models]` aliases, and an archive failure while journals exist stops cleanup instead of deleting them. Integration-verify runs the full gate and the review; knowledge-distill makes the knowledge check green and rewrites the knowledge sections this plan made false.

The plan targets wasted correction turns and wasted runs, and the resident replay of false doctrine. Every change is a byte or doctrine change that leaves agent behaviour to the agents, so its evidence is deterministic (metaplan evidence class 1): the tests and contracts below, and plan 2's replay for byte counts. It claims no token saving.

**Plan dependencies.** None. Canonical run order: this plan first, then 1, 2, 4, 5, 7 (one checkout runs one plan). The other open plans in `doc/plans/` are ignored by operator decision.

**Accepted warning.** The knowledge base already describes this tree and `loom knowledge sync` runs clean, so the plan has no knowledge-bootstrap stage. Until this plan merges, `loom plan verify --strict` fails on that one warning ("Consider adding a 'knowledge-bootstrap' stage", from `check_knowledge_recommendations`, `loom/src/plan/schema/validation.rs:1109-1137`); it is accepted. This plan's own fix removes it for every later plan, and integration-verify checks that the worktree binary no longer reports it for this file. The future bookend lints this plan adds already hold for it: exactly one knowledge-distill stage with no dependents and every other stage among its ancestors, and the one standard stage is an ancestor of integration-verify.

**Lanes (operator decision 2026-10-19: Claude plus Codex where it fits).**

| Stage | `implementers` | Reason |
| --- | --- | --- |
| `doctrine-and-guards` | `["claude", "codex"]` | Two codex units, each new or single-file code behind interfaces this plan pins verbatim, with mechanical proof (compile, then the orchestrator runs the module's tests): C1 writes the bookend lint module `v2_lints/bookends.rs` and its registration, C2 the stage `model` deserializer, its field attribute and its unit tests. Everything else stays on Claude: hook and doctrine edits (W2, W3), a multi-file deletion (W4), test pins held to exact ledger counts (W1), a signature change that ripples to three callers with `TempDir` tests (W6), the bootstrap-warning move with fixture-backed tests (W5), and the doctrine command test, which converges only by running against the real surfaces (W7). |
| `integration-verify` | omitted | Bookend stage. |
| `knowledge-distill` | omitted | Bookend stage. |

**Environment.** Network is allowed for every domain (`allowed_domains: ["*"]`, the PLAN-token-flow precedent the operator set after repeated sandbox friction); the codex lane needs network for its workspace routing (knowledge `mistakes/codex-worker-briefing.md`, "The Codex Lane Fails Inside a Stage Sandbox With No Network"). The `web/` provision entry exists because the version-2 JS-package lint requires one for the repository's `web/` package; no stage edits `web/`. Host tools: `cargo`, `rg`, `bash`, `bwrap` (confined acceptance), and the codex CLI with its plugin (checked by `loom run` at startup; a missing lane reroutes codex work to sonnet).

**Before `loom run` (operator).** Commit this plan, every file under `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/`, and the five sibling plans `doc/plans/token-optim/PLAN-02-*.md` to `PLAN-06-*.md`. At `4cc085b0` nothing under `doc/plans/` is tracked (`git ls-files doc/plans` is empty). `extract_brief_paths` (`loom/src/plan/schema/structural_checks.rs:78`) and the committed-input bundle (`loom/src/commands/run/plan_inputs/discovery.rs:22`) recognise only `doc/plans/briefs/`, so neither `loom plan verify` nor `loom run` notices an uncommitted brief under `doc/plans/token-optim/`, and a stage worktree holds only committed files (`commands/run/plan_inputs.rs:34-37`). The implementation stage's `before_stage` therefore checks that its briefs exist (the bookend stages name their brief in their description, and an agent that cannot open it stops there), and integration-verify's functional check 2 reads the sibling plans in its worktree. Do not commit the `codex-PLAN-*.md` review reports.

YAML is authoritative where this prose and the YAML differ.

## Goals

- Integration-verify signals stop telling the agent to run `loom knowledge update`; the hooks stop telling implementation stages to curate knowledge; dead and mis-scoped hook checks go.
- The recovery text names the stage's own journal (`loom memory list --stage <stage-id>`) in the template, the compaction hook and the signal.
- `found:`, `gotcha:` and `found/gotcha:` notes are one class.
- The README, `skills/loom-usage` and `skills/loom-plan-writer` state current behaviour: journal names, memory scope, the Fable cases, command confinement, the knowledge write verbs, what `loom review` does, distill never skipped, foundation work delegated, the knowledge write grant, the scaffold headings.
- The memory summary writer and everything that only served it are deleted.
- A section annotated deprecated, superseded or historical is proven to leave default retrieval.
- `loom plan verify` stops failing correct plans on the bootstrap warning, rejects version-2 plans whose bookends are missing or out of order, and rejects a stage `model` outside `haiku`, `sonnet`, `opus`, `fable`.
- An archive failure while journals exist stops plan completion before the `DONE-` rename, and stops `loom clean` and `loom init --clean` before they delete the state directory, so the journals survive and the step can be retried. The foreground `loom run` returns the error; under the daemon, `finish_orchestration` logs it and still writes the completion marker (`daemon/server/orchestrator.rs`), which the README states and knowledge-distill records as a concern. Distilling a failed run is documented and proven by a test.
- A test fails when a doctrine surface teaches a `loom` subcommand the CLI lacks.
- `loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt` is green before any acceptance in this programme depends on it.
- Non-goals: nothing from metaplan section 6; no new command, flag, config key or telemetry event; no change to effort, ceilings or delegation.

## Shared files: what this plan changes, by symbol

Later plans rebase on these edits. Each later brief re-reads the file at run time and anchors by the symbols named here.

| File | Later plans | What this plan changes |
| --- | --- | --- |
| `loom-hooks/post-tool-use.sh` | 1, 2, 5 | `remind_knowledge_update` is renamed `remind_memory_capture`; its heredoc becomes a memory-only box headed `POST-COMMIT REMINDER: Record Memory` (the items "RECORD any mistakes ... loom knowledge update mistakes" and "Memory becomes knowledge via the knowledge-distill stage" are deleted); the section comment becomes `=== POST-COMMIT MEMORY REMINDER ===`; its one call in the `git commit` / `loom stage commit` block is renamed. Nothing between the `LOOM_STAGE_ID` guard and the reminder (the forward-receipt call and the read-receipt branch, lines 248-259 at `6fee4ecd`) changes. |
| `loom-hooks/tests/run-all.sh` | none edit it (plans 1, 2 and 5 run it and expect 103) | The `commit-guard-memory-reminder.sh` line's description changes, and one line is added directly after it: `run_test "learning-validator: warns unless this stage's own journal holds an entry" "$SCRIPT_DIR/learning-validator-stage-journal.sh"`. The suite goes from 102 to 103 tests. No other line changes. |
| `loom-hooks/spawn-guard.sh` | 1 | Header item 5 only: it says the guard prepends the preamble to a typed spawn that lacks it, except `loom-codex-forwarder` and `codex:*` spawns (`spawn-guard.sh:271-290`), in every session, and that inside a stage (`GATE_PASSED`) it also appends an optional scoped worker brief. The file is 403 lines at `4cc085b0` (398 at `6fee4ecd`; `e747e9dc` added five); the rewrite adds at most one line, so it ends at 403 or 404. Plan 1's brief trims it to 400 or fewer. |
| `loom/src/orchestrator/signals/format/sections.rs` | 4 | `format_semi_stable_section`: the knowledge-updates box `matches!` and the Knowledge Management arm cover `Knowledge` and `KnowledgeDistill` only; the standard arm is `StageType::Standard \| StageType::IntegrationVerify`; its prohibition line reads `NEVER use 'loom knowledge' in this stage — knowledge-distill curates memory into knowledge.` `format_recitation_section`: the `loom memory list` line becomes ``- `loom memory list --stage {}`\n`` with `stage.id`, no trailing description (the one form rustfmt keeps on one line). `format_dynamic_section` is untouched. The ledger rows for the file and `format_semi_stable_section` are set to the counts `cargo test --test maintainability` reports (the `matches!` stays four lines, so expect 278 and 569 or a line either way from the comment rewrites); `format_recitation_section` stays at 58. |
| `loom/src/fs/memory/{mod,storage,query,types,export,export_suggestions}.rs` | 4 | `write_summary`, `generate_summary`, `MemoryJournal.summary`, `test_generate_summary` and their re-exports are deleted; `export.rs`'s `push_suggestions_section` re-export becomes a private `use`. `parser.rs` is unchanged: `parse_journal` keeps its `## Summary` branch. |
| `loom/src/commands/memory/handlers/prefix.rs` | 4 | `NotePrefix::parse` returns `Found` for `found:`, `gotcha:` and `found/gotcha:`; the test `found_and_gotcha_prefixes_are_one_class` pins it. |
| `CLAUDE.md.template` | 5 | Rule 3b's body only: `loom memory list` becomes `loom memory list --stage <stage-id>`. |
| `loom-hooks/session-start.sh` | 5 | The compact/resume `additionalContext` says `run 'loom memory list --stage ${LOOM_STAGE_ID}'`. |
| `loom-hooks/tests/session-start-compact.sh` | 5 | One added check that the context carries `loom memory list --stage test-stage`. |
| `skills/loom-plan-writer/SKILL.md` | none (plan 7 reads it) | Section 3's heading-grep sentence (an established tier-1 file carries `##` sections; the scaffold has none), Section 5's FOUNDATION sentence (a delegated FOUNDATION worker), Section 6's plan-version lint list (the two bookend lints added), Section 8's knowledge write-grant sentence (every stage's sandbox, `apply_knowledge_write_grant`). |
| `skills/loom-plan-writer/references/bookend-stages.md` | none | The knowledge-distill paragraph: the `loom review` pruning clause is deleted, "Skip ONLY if" becomes "Never skipped" with the empty-memory path, and the heading-grep sentence is corrected. |
| `loom/src/orchestrator/signals/tests_size.rs` | 5 | The `CLAUDE_MD_TEMPLATE_MAX_BYTES` doc comment says about 19,900 bytes. |
| `loom/src/orchestrator/signals/mod.rs` | 4, 5 | Adds `#[cfg(test)]`, `#[path = "tests_doctrine_commands.rs"]`, `mod tests_doctrine_commands;` directly after `tests_doctrine_waiting`. |
| `README.md` | 2, 4, 5, 7 | The memory-capture paragraph (journal path, recitation scope and the recovery-signal exception, the IV/distill wiring checklist), the state-tree comment, the Fable sentence, the confinement blockquote, the sandbox example and the note under it (the knowledge `deny_write` entry removed, the write grant stated), two paragraphs after the archive paragraph (archive abort; distilling a failed run), the `plan verify` paragraph, a bookend-lint paragraph in "Plan Version 2", and the `model` row. |
| `loom/maintainability-baseline.txt` | 1, 2, 4, 5 | Rows set to what `cargo test --test maintainability` reports: `file src/orchestrator/signals/format/sections.rs` and `function ... format_semi_stable_section` (about the same), `file src/orchestrator/signals/tests_cache.rs` (630 to 629), `file src/plan/schema/validation.rs` (lower). |
| `loom/src/plan/schema/validation/v2_lints/mod.rs` | none (later plans only run `loom plan verify`) | `mod bookends;` before `mod contracts;`, `bookends::check(ctx, &mut out);` after `contracts::check(ctx, &mut out);` in `run`, and `mod loom_subcommands;` becomes `pub(crate) mod loom_subcommands;` (C1); W7 adds `#[cfg(test)] pub(crate) fn loom_command_paths` and `#[cfg(test)] pub(crate) struct LoomCommandPaths` to `loom_subcommands.rs`. |
| `doc/loom/knowledge/architecture/signal-generation.md`, `entry-points/hooks.md` | 4, 5 | knowledge-distill rewrites the sections that describe integration-verify's signal and the learning validator to current truth with `replace-section`; headings are kept. |

## Deviations from the metaplan (the tree wins)

- **Stage `model` is shell-escaped.** The metaplan says the field is interpolated raw into the `claude` command line (`orchestrator/terminal/native/mod.rs:174`). `build_claude_command` passes it through `escape` (`loom/src/orchestrator/terminal/native/mod.rs:141`), so there is no injection; the defect is a typo that fails only at spawn. The allowlist still applies. A survey of the main checkout at `4cc085b0` (`rg -n '^\s+model: ' doc/plans loom/tests/fixtures/plans`) found `opus[1m]` in seven stage entries of three archived `DONE-` plans (`DONE-PLAN-anti-slop-thoroughness.md`, `DONE-PLAN-accept-edits-and-remote-control.md`, `DONE-PLAN-remove-container-backend.md`) and no other value outside the four aliases; no sibling plan in `doc/plans/token-optim/` sets a stage `model`. `doc/plans/` is untracked, so a stage worktree cannot repeat the survey, and the stage does not. Loom never re-parses archived plans, and persisted stage files parse as `Stage`, not `StageDefinition` (`fs/stage_loading.rs`, `extract_stage_definition`), so they stay as they are.
- **The bookend lints break two unit tests.** The metaplan says every v2 plan and fixture conforms. Two tests assert an empty full lint run on plans without bookends (`ordinary_wiring_pattern_is_clean`, `version_one_plans_and_missing_repositories_have_no_provision_findings`); W5 narrows each to its own lint.
- **The end-to-end lint case lives in the contract file.** The metaplan names `loom/tests/plan_environment_contracts.rs`, which holds another plan's frozen contracts. This stage's contracts already drive `loom plan verify --json` end to end, so the case lives in `loom/tests/doctrine_guard_contracts.rs`; acceptance still runs `plan_environment_contracts`, whose v2 fixture conforms.
- **`v2-valid.md` fails `--strict` for unrelated reasons.** In a scratch repository the installed binary reports six warnings (four missing summaries, two `Cargo.toml` paths) and exits 0 without `--strict` (rechecked 2026-10-19). Integration-verify runs `loom plan verify` without `--strict`, which still fails on any bookend error, since those are errors in a v2 plan.
- **`check_knowledge_recommendations` has six tests**, not seven (`loom/src/plan/schema/tests/knowledge_recommendations_tests.rs`).
- **`hook-system.md` lacks `sources`.** The metaplan says five high-impact pages carry `sources`/`verified`; `architecture/hook-system.md` carries `verified` only. Distill adds its sources with the `owned-waits.md` annotation.
- **The archive abort does not make the daemon report failure.** The metaplan says the archive failure aborts plan completion. `mark_plan_done_if_all_merged` returns the error, the foreground `loom run` propagates it (`commands/run/foreground.rs:100-104`), and the plan stays `IN_PROGRESS` with its state in place. The daemon's `finish_orchestration` (`daemon/server/orchestrator.rs:169-180`) only logs `Warning: Failed to mark plan as done: ...` and still writes the completion marker, which the status broadcaster reports as `OrchestrationComplete` (`daemon/server/broadcast.rs:192-197`). Changing what the daemon reports is out of scope. The README states the behaviour, and knowledge-distill records the gap in `concerns`.
- **Recovery signals carry no recitation.** The metaplan's README fix says the last ten journal entries reach every later signal for the stage. `format_recovery_signal` (`orchestrator/signals/recovery_format.rs`) renders no memory, so the README names the exception.
- **Note prefixes have no consumer yet.** `NotePrefix::Found` is read by nothing: `pending --group` sorts notes as mistake, stale-knowledge or other (`commands/memory/handlers/pending_groups.rs:62-75`), and `validate_note_shape` checks only the mistake and stale-knowledge forms (`prefix.rs:86-105`). W1's change makes the parser agree with the signal and the preamble; it changes no grouping or validation.
- **The failed-run procedure runs as a unit test.** The metaplan asks the stage to run it once. A stage session's `loom memory` commands resolve the plan's own state directory, and pointing them at a second one from inside the sandbox is unproven, so W6 runs the procedure as `archived_journals_copied_into_a_new_state_dir_are_pending`, over the functions the procedure exercises (`archive_run_state`'s layout, `pending_report`, `strict_should_fail`). An earlier authoring run reported one manual run outside a sandbox with the same result.
- **The doctrine test reads shell fences only.** The metaplan says markdown surfaces are read through fenced code lines. Prose sits in `text` fences: `skills/loom-orchestration/SKILL.md:177` carries `(loom caps the tree at 2 levels)`, which the lexer reads as a subshell running `loom caps`. Inline spans need the same care: `skills/loom-plan-writer/SKILL.md:121` carries `` `<!-- loom METADATA -->` ``, which lexes as `loom METADATA`. W7 reads every line of a fence labelled `bash`, `sh`, `shell`, `zsh` or `console`, plus each inline code span whose trimmed text starts with `loom`; the per-group minimums still hold (a port of the lexer counted 35 templates, 9 commands and 340 skills invocations under that rule at `4cc085b0`). Hooks print their guidance from heredoc bodies, which the lexer drops (`shell_lex.rs`, `skip_heredoc_bodies`), so W7 also reads every hook line whose text after leading whitespace and box characters starts with `loom`. Under these rules one line fails at `4cc085b0`, `skills/loom-usage/SKILL.md:713` (`loom stage hold/release <id>`); W3 splits it.
- **The knowledge check has a third issue.** Besides `conventions.md` (251 lines) and `mistakes/ci-toolchain-and-cargo.md` (414 at `4cc085b0`), `conventions/git-and-build-workflow.md` cites `doc/astra-bug-sweep-2026-10-15.md`, which `4cc085b0` deleted. Distill re-runs the check at its start and fixes whatever it reports.
- **The full test suite is not in the standard stage's acceptance.** The metaplan asks every code stage to run the pre-push gate. `loom plan verify` warns on an unfiltered test run outside integration-verify (`plan/schema/validation_suite.rs`, `warn_full_suite_outside_integration_verify`), and `--strict` fails on the warning. The standard stage runs the whole pre-push gate (`loom/.githooks/pre-push`) and CI's four hook steps in its `loom-verifier` round, and carries module-filtered and per-test filters plus the other gate steps in acceptance; integration-verify's acceptance runs `cargo test --all-targets --no-fail-fast`.
- **Four gate steps run only in a verifier round.** The pre-push gate also runs markdownlint over tracked markdown, `cargo audit` and `scripts/flake-check.sh`, and CI also runs `scripts/test-pre-commit-partial-staging.sh`. Acceptance commands run confined with no network and an empty scratch `CARGO_HOME`, bun cache and `TMPDIR` (`verify/criteria/confine.rs:214`, `confine_host.rs:204-215`), so `bunx` and `cargo audit` cannot run there. `verifier.md` and integration-verify's `functional-checks.md` run all four in the session sandbox, with caches under the ignored `loom/target/`.
- **Line drift.** The plan-writer's three Fable cases are at `SKILL.md:180-182`, not `174-175`. Briefs anchor line numbers to `6fee4ecd`; between it and `4cc085b0` the plan's files changed only in `README.md` (4 lines), `loom-hooks/spawn-guard.sh` (+5 in `resolve_defined_tier`), `format/codex.rs` (the `YOU (opus)` literal is unchanged) and ledger rows this plan does not name. Every ledger row the plan names, and the hook suite's 102 tests, are unchanged at `4cc085b0`.

## Baselines

Checked on 2026-10-19 at `6fee4ecd` (main checkout, outside a sandbox): `loom-hooks/tests/run-all.sh` registers 102 tests; `loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt` fails on the three issues above; `v2-valid.md` verifies as stated above; every two-word `loom` path in the markdown doctrine surfaces resolves with `loom <path> --help`, apart from the `text`-fence prose line above and `loom stage dispute-*` (a placeholder). Recorded by the earlier authoring run at the same revision and not re-run (no measurement builds in this authoring step): `cargo build` exit 0; `cargo test --all-targets --no-fail-fast` exit 0, 8,576 passed, 0 failed, 7 ignored, about 2.5 minutes; 6 doctests; clippy, fmt and rustdoc exit 0; `./scripts/check-hook-syntax.sh` exit 0; both hook-suite modes 102 passed. Rechecked on 2026-10-20 at `4cc085b0` (main checkout): `loom plan verify` on this plan reports 0 errors and the one accepted knowledge warning; `loom knowledge check --strict --baseline ...` fails on the same three issues; `loom-hooks/spawn-guard.sh` is 403 lines; every `cargo test --lib` filter in the stage's acceptance that names an existing module selects tests (`orchestrator::signals::` 250, `orchestrator::monitor::` 114, `plan::schema::` 289, `fs::memory::` 43, `fs::knowledge::` 140, `fs::plan_lifecycle::` 30, `commands::memory::` 45, `commands::knowledge::tests_annotate` 9, `commands::init::` 48, `commands::clean::` 10, `commands::plan::` 5, `assets::tests::` 35; `--test integration plan_verify::` 26).

## Execution Diagram

```mermaid
graph LR
    doctrine-and-guards --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. Doctrine truth and plan guards (`doctrine-and-guards`)

One standard stage carries every code and doctrine change. Stage Necessity Test: no piece needs another merged first (Q1: every ordering is compile-order, handled by waves inside the stage); the territories that share files (`README.md`, the plan-writer skill, `v2_lints/mod.rs`, the ledger) would conflict as separate parallel stages and gain nothing as sequential ones (Q2 resolved by ownership); no later work needs a checkpoint between them (Q3); the combined work fits one session, since each worker reads only its own ranges and the two codex units return compact evidence (Q4).

Workers (briefs under `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/`, each read after `common.md`):

| Worker | Lane | Role | Files owned (relative to `loom/`) | Brief |
| --- | --- | --- | --- | --- |
| W1 | sonnet | Signal doctrine, note prefixes | `src/orchestrator/signals/format/{sections,codex}.rs`, `src/orchestrator/signals/{tests_brief_rendering,tests_cache,tests_size}.rs`, `src/commands/memory/handlers/prefix.rs` | `w1-signal-doctrine.md` |
| W2 | sonnet | Hook doctrine and checks | `../loom-hooks/{post-tool-use,commit-guard,learning-validator,session-start,spawn-guard}.sh`, four hook tests, `../loom-hooks/tests/run-all.sh` | `w2-hooks.md` |
| W3 | sonnet | README, template, skills | `../README.md`, `../CLAUDE.md.template`, `../skills/loom-usage/SKILL.md`, `../skills/loom-plan-writer/SKILL.md` and four references | `w3-docs.md` |
| W4 | sonnet | Memory summary path, knowledge scaffold, section-state retrieval test | `src/orchestrator/monitor/{handlers,detection}.rs`, six `src/fs/memory/*.rs` files, `src/fs/knowledge/{templates,dir,tests_dir_bugfixes}.rs`, `src/commands/knowledge/tests_annotate.rs` | `w4-dead-path-and-scaffold.md` |
| W5 | sonnet | Bootstrap warning, bookend lint tests | `src/plan/schema/validation.rs`, `src/plan/schema/validation/knowledge_recommendations.rs`, `src/commands/plan/verify.rs`, `src/commands/init/plan_setup.rs`, five schema test files, `tests/integration/plan_verify.rs` | `w5-plan-verify-lints.md` |
| W6 | sonnet | Archive abort, failed-run distill test | `src/fs/memory/archive.rs` and its test, `src/fs/plan_lifecycle.rs` and its test, `src/commands/init/cleanup.rs`, `src/commands/clean/mod.rs`, `src/commands/memory/handlers/tests_resolve_pending.rs` | `w6-archive.md` |
| C1 | codex terra | Bookend lint module | `src/plan/schema/validation/v2_lints/{bookends,mod}.rs` | `c1-bookends.md` |
| C2 | codex terra | Stage `model` allowlist | `src/plan/schema/{types,types_v2}.rs`, `src/plan/schema/tests/{stage_model_tests,mod}.rs` | `c2-stage-model.md` |
| W7 (wave 2) | sonnet | Doctrine command resolution test | `src/plan/schema/validation/v2_lints/loom_subcommands.rs`, `src/orchestrator/signals/tests_doctrine_commands.rs`, `src/orchestrator/signals/mod.rs` | `w7-doctrine-commands.md` |

The main agent owns `loom/maintainability-baseline.txt` (exact rows, after `cargo test --test maintainability`), the post-unit checks of C1 and C2, the verifier round (`verifier.md`), the review rounds, the integrity dispute and the commit. The stage YAML's table is the parsed one.

Risk-checklist walk (v2):

- Untrusted input: the stage `model` value comes from a plan file. Contract `stage-model-outside-the-allowlist-is-rejected`.
- Filesystem paths: archiving copies the state directory before deletion. Contract `archive-failure-with-journals-is-an-error`.
- Process I/O volume: none of the changed code reads a pipe or walks a large tree, apart from the doctrine test, which reads files directly; no contract.
- Configuration propagation: the bootstrap warning reads the repository's knowledge tree. Contract `bootstrap-warning-silent-for-a-populated-tree` (a non-default, populated tree).
- Lifecycle: an archive failure must leave a retryable state. The contract above plus W6's caller tests (cleanup, clean, plan completion retry).
- Reachability: the lints are reached only through `loom plan verify`. Contracts `distill-lint-rejects-a-stage-it-never-follows` and `iv-lint-rejects-an-unverified-standard-stage` drive the CLI. No `reachable` check: the source graph does not bind the `v2_lints::run` call in `commands/plan/verify.rs`, so a literal `wiring` check on the registration line stands in.
- External data: none.
- The behaviour the stage exists to add (IV doctrine): contract `iv-signal-never-teaches-knowledge-writes`.

### 2. Integration verification (`integration-verify`)

Full gate from `loom/` and the hook suites, the four verifier-only gate steps, the review, and the functional checks in `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/integration-verify/functional-checks.md`: the worktree binary no longer prints the bootstrap warning for this plan (judged on the `knowledge` bucket of `--json`; a criterion reading a path under `doc/plans/` draws a `plan verify` warning, so this stays a functional check behind the contract and the two caller wirings); every `PLAN-0[1-6]-*.md` plan under `doc/plans/token-optim/` passes the bookend lints (a failure is recorded for that plan's author); `v2-valid.md` still verifies.

### 3. Knowledge distillation (`knowledge-distill`)

Single agent. Follows `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/knowledge-distill/curation.md`: corrections first, the structural issues, freshness metadata for `owned-waits.md` and `hook-system.md`, the knowledge sections this plan made false, receipts for every entry. `--strict-evidence` stays out of its acceptance.

## Coverage of the metaplan

Section 3 of the metaplan:

| Metaplan item | Where |
| --- | --- |
| IV curates no knowledge; pins rewritten; ledger rows lowered | W1, main agent (ledger); contract `iv-signal-never-teaches-knowledge-writes` |
| Post-commit reminder | W2 |
| Dead `remind_knowledge_capture`; stage-scoped `remind_memory_usage`; test writes the stage's journal | W2 |
| `learning-validator.sh` counts `###` in the stage journal; new hook test | W2 |
| Note prefixes one class, pinned by a test (parser agrees with signal and preamble; no consumer reads the class yet) | W1 |
| Recovery text in template, hook, signal | W3, W2, W1 |
| `codex.rs:90` names no model | W1 |
| `spawn-guard.sh:19-20` header | W2 |
| `tests_size.rs:36` comment | W1 |
| README `:941`, `:948`, `:1054`, `:1115`, `:1310` | W3 |
| `loom-usage` `:401`, `:464`; `plan-template.md:157`; `bookend-stages.md:36` | W3 |
| Plan-writer append-only, distill always, FOUNDATION, write grant, scaffold headings | W3 |
| Scaffold sentence and `has_content`, fresh-scaffold test | W4 |
| Memory summary deletion | W4 |
| `owned-waits.md` frontmatter | distill |
| Knowledge check green | distill |
| `--strict-evidence` not in distill acceptance | distill YAML |
| Section state leaves retrieval: one fixture test | W4 (`a_section_annotated_non_current_leaves_default_retrieval`) |
| Bootstrap warning; both callers; tests | W5; contract `bootstrap-warning-silent-for-a-populated-tree` |
| Bookend lints | C1 (module), W5 (unit tests); contracts `distill-lint-rejects-a-stage-it-never-follows`, `iv-lint-rejects-an-unverified-standard-stage` |
| Stage `model` allowlist; survey of plans and fixtures | C2; the planning-time survey in "Deviations from the metaplan"; contract `stage-model-outside-the-allowlist-is-rejected` |
| Archive failure aborts | W6; contract `archive-failure-with-journals-is-an-error` |
| Failed-run distill paragraph; procedure run once | W3; W6 (unit test, deviation) |
| `SKILL.md:294` and README list the new lints | W3 |
| Subcommand resolution test | W7 |
| No knowledge writes in standard and IV signals (test assertions) | W1 |
| Lint unit tests, end-to-end case, `v2-valid.md` check | W5; contracts; integration-verify |
| Archive-abort test, hook tests, the gate with the hook steps | W6, W2, verifier round, acceptance |
| No guidance lost | Each removed sentence is false or has its replacement in the briefs |
| State table: worker brief is stage-only; header wrong | W2 (`spawn-guard.sh` header) |

Disposition rows that name plan 3:

| Source and item | Where |
| --- | --- |
| Codex ledger #6 knowledge semantic freshness | distill (`owned-waits.md`, `hook-system.md` annotations; `--strict-evidence` stays out) |
| Audit 8c section state leaves retrieval; 8e duplicate headings | W4 (8c test); 8e already shipped (`fs/knowledge/catalog/headings.rs:68`) |
| Proposal §3.1, 7.1 knowledge-readiness, narrowed | W5 |
| Proposal §3.2, 6.1, 6.3 exactly one distill finalizer | C1, W5 |
| Proposal §6.4 archive failure blocks cleanup | W6 |
| Proposal §6.6 "`loom review` prunes knowledge" claims | W3 |
| Proposal §3.3, 9.5 closed allowlist for stage `model` | C2 |
| Proposal §3.3 signal names the resolved orchestrator: names no model | W1 (`codex.rs`) |
| Proposal §9.2 Fable doctrine drift, README fix | W3 |
| Proposal §10 ancestry lint kept | C1, W5 |
| Proposal §10.1 foundation as a worker assignment | W3 |
| Proposal §3.5 confined commands, README stale | W3 |
| Graft Gap 4 memory summary round-trip: writer deleted | W4 |
| Graft Gap 7 lifecycle state and evidence (freshness) | distill; W4 (section-state test) |
| Graft Gap 8 stop calling knowledge append-only (scaffold and plan-writer residue) | W4, W3; distill (`stack.md:4` note) |
| Graft Gap 8 IntegrationVerify must not curate knowledge | W1 |
| Graft Gap 8 compact recovery, `--stage` scoping | W1, W2, W3 |
| Graft Phase 0 mandatory terminal distill: a v2 lint | C1, W5 |
| Graft single writer: narrowed to doctrine and the file-tool guard | W1, W2, W3 (doctrine); the guard is unchanged |
| Graft commit-guard knowledge path and stage scoping; post-commit reminder | W2 |
| Graft `/loom-knowledge` skill dismissed; `loom-usage` stale lines | W3 |
| Graft doctrine tests that named commands exist | W7 |
| Graft README memory location and inheritance claims (plans 3 and 4) | W3 (current behaviour; plan 4 rewrites it) |
| Graft host finalizer and bootstrap deliverables: dropped, plan 3 fixes the bootstrap warning only | W5 (warning only) |

Dropped by the metaplan, with its reasons, and not built: removing `dirty-paths.json` and `loom context record-edit`; a `command_confinement: inherit` warning; a discard flag and telemetry event for skipping distill; a catalog check on every knowledge-completion branch; a bootstrap receipt gated on the check; confirming audit item 1g.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
    network:
      allowed_domains: ["*"]
      allow_local_binding: true
      allow_unix_sockets: []
  stages:
    - id: doctrine-and-guards
      name: "Doctrine truth and plan guards"
      summary: "Removes false and contradictory instructions from signals, hooks, the template, the README and two skills, deletes the dead memory-summary path, and adds four plan guards: a bootstrap warning that respects a documented tree, version-2 bookend lints, a stage model allowlist and an archive abort."
      stage_type: standard
      skills: ["loom-rust"]
      implementers: ["claude", "codex"]
      subagent_timeout_secs: 900
      working_dir: "loom"
      dependencies: []
      description: |
        Build stage 1 of doc/plans/token-optim/PLAN-01-doctrine-truth-and-plan-guards.md (loom run renames it IN_PROGRESS-PLAN-01-doctrine-truth-and-plan-guards.md). The plan overview lists exactly what later plans expect this stage to change in the shared files; make those edits and no others there.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/. YAML paths and the worker table are relative to loom/ (../ for files outside the crate); brief paths are relative to the repository root. Brief line numbers come from 6fee4ecd: every worker anchors by symbol and quoted text.
        Every worker and codex unit reads doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md first, then its own brief. Territories are DISJOINT across both lanes. Workers NEVER spawn subagents.
        LANES: this stage licenses claude and codex. C1 and C2 are codex units; every W row is Claude.
        WAVE 1, CLAUDE: spawn W1-W6 in ONE message, in the background, BY AGENT TYPE loom-software-engineer (sonnet), each with a short fixed prompt plus "Your brief: <path>. Read it in full before anything else."
        WAVE 1, CODEX: in your next message spawn C1 and C2 as loom-codex-forwarder subagents in the FOREGROUND, both in ONE message, each with --model gpt-5.6-terra --effort xhigh, its unit id (c1-bookends, c2-stage-model), an explicit Bash timeout of 600000 ms, and this task text with its brief path filled in: "Read the brief at <brief path> with cat and carry out every step in it. Write only the files it lists. Run no git command." The task text carries no apostrophe. After each returns, run git status --short and confirm no changed path lies outside the union of the territories in the worker table below (W1-W6 write the same tree meanwhile, so a path inside a territory is attributed by each unit's own report), then cargo build. A unit that timed out (exit 124) or failed is never re-forwarded as is: hand its brief and the failure to a fresh loom-software-engineer.
        Then wait for W1-W6 with one background loom subagents watch. Once W5 and C1 have both reported, run cargo test --lib plan::schema:: yourself (W5's bookend unit tests exercise C1's lints); after C2, cargo test --lib plan::schema::tests::stage_model_tests. Route each failure to a fresh worker of the owning territory.
        WAVE 2: once W2, W3 and C1 have reported, spawn W7 the same way as wave 1. If W7 reports an unresolved loom command in a surface, spawn one fresh loom-software-engineer for the owning territory (W2 for loom-hooks, W3 for its docs; a surface no worker owns goes to a fresh worker with that one file) with the file, line and path.
        LEDGER (you, after both waves): cargo fmt --all, then cargo test --test maintainability, then set each row it names in maintainability-baseline.txt to the exact value it reports. Expected changes: file src/orchestrator/signals/format/sections.rs and function format_semi_stable_section move by at most a line or two from the comment rewrites (the matches! stays four lines after rustfmt), so set whatever it reports; file src/orchestrator/signals/tests_cache.rs 630 to 629; file src/plan/schema/validation.rs lower. Every other recorded row must stay as it is (format_dynamic_section 205, format_recitation_section 58, initialize_with_plan 192, verify.rs 555 and execute 133, default_content 62, tests/integration/plan_verify.rs 1028, tests/plan_environment_contracts.rs plan 64, test_signal_codex_implementers_section_gated 116): route a mismatch there back to a fresh worker of the owning territory. maintainability-baseline.txt is a ratchet_files entry, so this edit raises TI-ratchet-loom/maintainability-baseline.txt.
        GATE: one loom-verifier round briefed with doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/verifier.md, which carries every step of the repository's pre-push gate (loom/.githooks/pre-push, markdownlint, cargo audit and scripts/flake-check.sh included), the doctest run, CI's four hook steps and then this stage's acceptance. A markdownlint failure goes to a fresh W3 worker, which fixes it by hand. The acceptance below runs module-filtered tests because loom plan verify warns on an unfiltered cargo test outside integration-verify; the verifier round and integration-verify run the whole suite. Route each failure to a fresh worker of the owning territory, then run a fresh verifier round.
        INTEGRITY: before the final review round run loom stage review integrity <stage-id>. Only files matching the Rust test globs (src/testrun/languages.rs: **/tests/**/*.rs, **/tests.rs, **/*_tests.rs, **/tests_*.rs, **/*_test.rs) raise events, and a TI-edit needs a removed or changed assert line (src/verify/integrity/edits.rs). Expected events, ids with repository-relative paths: TI-ratchet-loom/maintainability-baseline.txt (rows set to exact counts); TI-edit-loom/src/orchestrator/signals/tests_cache.rs (the standard-signal pins rewritten); TI-edit-loom/src/plan/schema/tests/v2_lint_command_tests.rs and TI-edit-loom/src/plan/schema/tests/v2_lint_environment_repo_tests.rs (narrowed to their own lints); and, only if W6 rewrites its assert lines, TI-edit-loom/src/fs/memory/tests/archive.rs (Result return). prefix.rs, fs/memory/mod.rs and export_suggestions.rs match no test glob, and tests_brief_rendering.rs and knowledge_recommendations_tests.rs change no assert line, so none of those raises an event. File ONE loom stage dispute-integrity naming every listed event that is in this set, each with its reason; an event outside the set is a defect to fix.
        Then the review round the signal names, then commit.

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | W1 | Signal doctrine and note prefixes | sonnet | src/orchestrator/signals/format/sections.rs; src/orchestrator/signals/format/codex.rs; src/orchestrator/signals/tests_brief_rendering.rs; src/orchestrator/signals/tests_cache.rs; src/orchestrator/signals/tests_size.rs; src/commands/memory/handlers/prefix.rs | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/w1-signal-doctrine.md |
        | W2 | Hook doctrine and checks | sonnet | ../loom-hooks/post-tool-use.sh; ../loom-hooks/commit-guard.sh; ../loom-hooks/learning-validator.sh; ../loom-hooks/session-start.sh; ../loom-hooks/spawn-guard.sh; ../loom-hooks/tests/commit-guard-memory-reminder.sh; ../loom-hooks/tests/post-tool-use-commit-reminder-tokenized.sh; ../loom-hooks/tests/learning-validator-stage-journal.sh; ../loom-hooks/tests/session-start-compact.sh; ../loom-hooks/tests/run-all.sh | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/w2-hooks.md |
        | W3 | README, template and skill doctrine | sonnet | ../README.md; ../CLAUDE.md.template; ../skills/loom-usage/SKILL.md; ../skills/loom-plan-writer/SKILL.md; ../skills/loom-plan-writer/references/authoring-detail.md; ../skills/loom-plan-writer/references/bookend-stages.md; ../skills/loom-plan-writer/references/plan-template.md; ../skills/loom-plan-writer/references/sandbox.md | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/w3-docs.md |
        | W4 | Memory summary path, knowledge scaffold, section-state retrieval test | sonnet | src/orchestrator/monitor/handlers.rs; src/orchestrator/monitor/detection.rs; src/fs/memory/mod.rs; src/fs/memory/storage.rs; src/fs/memory/query.rs; src/fs/memory/types.rs; src/fs/memory/export.rs; src/fs/memory/export_suggestions.rs; src/fs/knowledge/templates.rs; src/fs/knowledge/dir.rs; src/fs/knowledge/tests_dir_bugfixes.rs; src/commands/knowledge/tests_annotate.rs | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/w4-dead-path-and-scaffold.md |
        | W5 | Bootstrap warning and bookend lint tests | sonnet | src/plan/schema/validation.rs; src/plan/schema/validation/knowledge_recommendations.rs; src/commands/plan/verify.rs; src/commands/init/plan_setup.rs; src/plan/schema/tests/knowledge_recommendations_tests.rs; src/plan/schema/tests/v2_lint_tests.rs; src/plan/schema/tests/v2_lint_bookend_tests.rs; src/plan/schema/tests/v2_lint_command_tests.rs; src/plan/schema/tests/v2_lint_environment_repo_tests.rs; tests/integration/plan_verify.rs | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/w5-plan-verify-lints.md |
        | W6 | Archive abort and failed-run distill test | sonnet | src/fs/memory/archive.rs; src/fs/memory/tests/archive.rs; src/fs/plan_lifecycle.rs; src/fs/plan_lifecycle_tests.rs; src/commands/init/cleanup.rs; src/commands/clean/mod.rs; src/commands/memory/handlers/tests_resolve_pending.rs | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/w6-archive.md |
        | C1 | Bookend lint module | codex terra | src/plan/schema/validation/v2_lints/bookends.rs; src/plan/schema/validation/v2_lints/mod.rs | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/c1-bookends.md |
        | C2 | Stage model allowlist | codex terra | src/plan/schema/types.rs; src/plan/schema/types_v2.rs; src/plan/schema/tests/stage_model_tests.rs; src/plan/schema/tests/mod.rs | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/c2-stage-model.md |
        | W7 | Doctrine command resolution test (wave 2) | sonnet | src/plan/schema/validation/v2_lints/loom_subcommands.rs; src/orchestrator/signals/tests_doctrine_commands.rs; src/orchestrator/signals/mod.rs | common.md | doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/w7-doctrine-commands.md |

        CONTRACT SURFACE. The contract session writes tests/doctrine_guard_contracts.rs from this text before any code. Every contract is a top-level #[test] fn named after its id with underscores, so cargo test <name> -- --exact selects it. The file declares #[path = "integration/helpers.rs"], #[allow(dead_code)], mod helpers; for loom_cmd(), as tests/plan_environment_contracts.rs does, and copies that file's git, write_file and repo helpers. Its own verify helper runs loom plan verify --json PLAN.md from the repository, parses stdout as JSON and returns (stage_id, message) pairs from errors[] (plan_environment_contracts.rs's verify_errors reads messages only). Every fn stays under 50 lines and the file under 400 lines, rustfmt-clean (cargo test --test maintainability scans tests/). The plan fixtures are version-2 plans built from a small per-stage YAML helper (id, stage_type, dependencies, whether it carries a contract), so no fixture fn passes 50 lines; plan() in tests/plan_environment_contracts.rs is the field reference. Every stage has a summary. Each standard stage has one contract entry AND one acceptance command, shaped like add-greeting's in plan() (contract file loom/tests/<name>.rs with runner cargo-test, acceptance cargo test --manifest-path loom/Cargo.toml --test <name>, one <name> per stage): validate() rejects a standard stage with no acceptance and no goal checks (plan/schema/validation.rs, the requires_goal_backward branch; contracts do not count), and loom plan verify runs the v2 lints only when validate() passes (commands/plan/verify.rs, execute), so without it the two lint contracts can never see their findings. The integration-verify stage's acceptance is cargo test --manifest-path loom/Cargo.toml --all-targets; knowledge stages accept loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt. All six are red at freeze: two of them call signatures this stage introduces, so the file does not build until then.
        - iv-signal-never-teaches-knowledge-writes: for knowledge_tree_empty false and true, build loom::models::session::Session::new() assigned to stage "stage-1" (assign_to_stage), loom::models::stage::Stage::new("Verify".into(), None) with id "stage-1" and stage_type StageType::IntegrationVerify, loom::models::worktree::Worktree::new("stage-1".into(), "/repo/.worktrees/stage-1".into(), "loom/stage-1".into()) and loom::orchestrator::signals::EmbeddedContext { knowledge_tree_empty, ..Default::default() }; render loom::orchestrator::signals::format_signal_with_metrics(&session, &stage, &worktree, &[], None, None, &ctx).content; assert it contains "NEVER use 'loom knowledge' in this stage" and contains none of "loom knowledge update", "KNOWLEDGE UPDATES REQUIRED", "## Knowledge Management".
        - distill-lint-rejects-a-stage-it-never-follows: a git repository holding PLAN.md with knowledge-bootstrap (knowledge) -> add-greeting (standard) -> integration-verify -> knowledge-distill, plus a root knowledge stage knowledge-notes that no stage depends on; loom plan verify --json exits 1 and its errors hold an entry with stage_id "knowledge-notes" whose message contains "not an ancestor of knowledge-distill stage", and no such entry for add-greeting or knowledge-bootstrap.
        - iv-lint-rejects-an-unverified-standard-stage: knowledge-bootstrap -> add-parser (standard) -> add-greeting (standard) -> integration-verify -> knowledge-distill, plus add-farewell (standard, depends on knowledge-bootstrap) which knowledge-distill also depends on; errors hold an entry with stage_id "add-farewell" whose message contains "not an ancestor of any integration-verify stage", and no such entry for add-parser or add-greeting.
        - bootstrap-warning-silent-for-a-populated-tree: a TempDir holding doc/loom/knowledge/architecture.md with "# Architecture\n\n## Module map\n\n- src/main.rs - entry point\n"; loom::plan::schema::check_knowledge_recommendations(&[stage], Some(temp.path())) with stage = loom::plan::schema::StageDefinition { id: "feature".into(), name: "Feature".into(), working_dir: ".".into(), ..Default::default() } returns an empty Vec.
        - stage-model-outside-the-allowlist-is-rejected: loom::yaml::from_str::<loom::plan::LoomMetadata> on a version-1 plan of one stage (id s1, name First Stage, working_dir ".", artifacts ["README.md"]) with model "claude-opus-5" returns Err whose text contains "invalid model"; the same plan with model "sonnet" parses and stages[0].model is Some("sonnet").
        - archive-failure-with-journals-is-an-error: a TempDir root with root/.loom/work/memory/stage-a.md holding "journal", root/.loom/memory created and root/.loom/memory/archive written as a regular file; loom::fs::memory::archive_run_state(&root.join(".loom/work"), root, Some("plan-a")) returns Err, format!("{err:#}") contains root.join(".loom/memory/archive").display().to_string(), and root/.loom/work/memory/stage-a.md still exists.
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/orchestrator/signals/format/sections.rs"
        - "src/orchestrator/signals/format/codex.rs"
        - "src/orchestrator/signals/tests_brief_rendering.rs"
        - "src/orchestrator/signals/tests_cache.rs"
        - "src/orchestrator/signals/tests_size.rs"
        - "src/orchestrator/signals/tests_doctrine_commands.rs"
        - "src/orchestrator/signals/mod.rs"
        - "src/commands/memory/handlers/prefix.rs"
        - "src/commands/memory/handlers/tests_resolve_pending.rs"
        - "src/commands/knowledge/tests_annotate.rs"
        - "src/orchestrator/monitor/handlers.rs"
        - "src/orchestrator/monitor/detection.rs"
        - "src/fs/memory/mod.rs"
        - "src/fs/memory/storage.rs"
        - "src/fs/memory/query.rs"
        - "src/fs/memory/types.rs"
        - "src/fs/memory/export.rs"
        - "src/fs/memory/export_suggestions.rs"
        - "src/fs/memory/archive.rs"
        - "src/fs/memory/tests/archive.rs"
        - "src/fs/knowledge/templates.rs"
        - "src/fs/knowledge/dir.rs"
        - "src/fs/knowledge/tests_dir_bugfixes.rs"
        - "src/fs/plan_lifecycle.rs"
        - "src/fs/plan_lifecycle_tests.rs"
        - "src/plan/schema/validation.rs"
        - "src/plan/schema/validation/knowledge_recommendations.rs"
        - "src/plan/schema/validation/v2_lints/bookends.rs"
        - "src/plan/schema/validation/v2_lints/mod.rs"
        - "src/plan/schema/validation/v2_lints/loom_subcommands.rs"
        - "src/plan/schema/types.rs"
        - "src/plan/schema/types_v2.rs"
        - "src/plan/schema/tests/mod.rs"
        - "src/plan/schema/tests/stage_model_tests.rs"
        - "src/plan/schema/tests/knowledge_recommendations_tests.rs"
        - "src/plan/schema/tests/v2_lint_tests.rs"
        - "src/plan/schema/tests/v2_lint_bookend_tests.rs"
        - "src/plan/schema/tests/v2_lint_command_tests.rs"
        - "src/plan/schema/tests/v2_lint_environment_repo_tests.rs"
        - "src/commands/plan/verify.rs"
        - "src/commands/init/plan_setup.rs"
        - "src/commands/init/cleanup.rs"
        - "src/commands/clean/mod.rs"
        - "tests/integration/plan_verify.rs"
        - "tests/doctrine_guard_contracts.rs"
        - "maintainability-baseline.txt"
        - "../loom-hooks/post-tool-use.sh"
        - "../loom-hooks/commit-guard.sh"
        - "../loom-hooks/learning-validator.sh"
        - "../loom-hooks/session-start.sh"
        - "../loom-hooks/spawn-guard.sh"
        - "../loom-hooks/tests/commit-guard-memory-reminder.sh"
        - "../loom-hooks/tests/post-tool-use-commit-reminder-tokenized.sh"
        - "../loom-hooks/tests/learning-validator-stage-journal.sh"
        - "../loom-hooks/tests/session-start-compact.sh"
        - "../loom-hooks/tests/run-all.sh"
        - "../README.md"
        - "../CLAUDE.md.template"
        - "../skills/loom-usage/SKILL.md"
        - "../skills/loom-plan-writer/SKILL.md"
        - "../skills/loom-plan-writer/references/authoring-detail.md"
        - "../skills/loom-plan-writer/references/bookend-stages.md"
        - "../skills/loom-plan-writer/references/plan-template.md"
        - "../skills/loom-plan-writer/references/sandbox.md"
      contracts:
        - id: iv-signal-never-teaches-knowledge-writes
          file: tests/doctrine_guard_contracts.rs
          test: iv_signal_never_teaches_knowledge_writes
          scenario: "renders an integration-verify signal through format_signal_with_metrics with knowledge_tree_empty false, then true, and an empty context pack"
          rejects: "a signal that drops the knowledge-updates box for integration-verify but keeps the Knowledge Management arm, whose command table tells the stage to run loom knowledge update"
        - id: distill-lint-rejects-a-stage-it-never-follows
          file: tests/doctrine_guard_contracts.rs
          test: distill_lint_rejects_a_stage_it_never_follows
          scenario: "loom plan verify --json on a version-2 plan whose root knowledge stage knowledge-notes has no dependent, while add-greeting reaches knowledge-distill only transitively"
          rejects: "a lint that only counts knowledge-distill stages, or that checks only the distill stage's direct dependencies and so flags add-greeting"
        - id: iv-lint-rejects-an-unverified-standard-stage
          file: tests/doctrine_guard_contracts.rs
          test: iv_lint_rejects_an_unverified_standard_stage
          scenario: "loom plan verify --json on a version-2 plan where standard stage add-farewell feeds knowledge-distill but no integration-verify stage, and add-parser reaches integration-verify only through add-greeting"
          rejects: "a lint that accepts any standard stage the distill stage follows, or that checks only the integration-verify stage's direct dependencies and so flags add-parser"
        - id: bootstrap-warning-silent-for-a-populated-tree
          file: tests/doctrine_guard_contracts.rs
          test: bootstrap_warning_silent_for_a_populated_tree
          scenario: "check_knowledge_recommendations on one root standard stage with Some(root) of a TempDir whose doc/loom/knowledge/architecture.md holds a real ## section"
          rejects: "a check that ignores the repository's knowledge tree and keeps warning on every plan whose stage ids and names lack the word knowledge"
        - id: stage-model-outside-the-allowlist-is-rejected
          file: tests/doctrine_guard_contracts.rs
          test: stage_model_outside_the_allowlist_is_rejected
          scenario: "parses one plan YAML with stage model claude-opus-5 and the same YAML with model sonnet through loom::yaml::from_str::<LoomMetadata>"
          rejects: "a model field that still accepts any string, or a deserializer that validates reasoning_effort only"
        - id: archive-failure-with-journals-is-an-error
          file: tests/doctrine_guard_contracts.rs
          test: archive_failure_with_journals_is_an_error
          scenario: "archive_run_state on a TempDir whose .loom/work/memory holds stage-a.md while .loom/memory/archive is a regular file, so the archive directory cannot be created"
          rejects: "an archive_run_state that prints the failure and returns Ok(None), so its caller deletes the state directory holding the only copy of the journals"
      acceptance:
        - "cargo build"
        - "cargo test --lib orchestrator::signals::"
        - "cargo test --lib orchestrator::signals::tests_doctrine_commands"
        - "cargo test --lib orchestrator::monitor::"
        - "cargo test --lib plan::schema::"
        - "cargo test --lib plan::schema::tests::stage_model_tests"
        - "cargo test --lib bookend_lints"
        - "cargo test --lib fs::memory::"
        - "cargo test --lib fs::knowledge::"
        - "cargo test --lib fs::plan_lifecycle::"
        - "cargo test --lib commands::memory::"
        - "cargo test --lib commands::knowledge::tests_annotate"
        - "cargo test --lib commands::init::"
        - "cargo test --lib commands::clean::"
        - "cargo test --lib commands::plan::"
        - "cargo test --lib assets::tests::"
        - "cargo test --lib fs::plan_lifecycle::tests::archive_failure_prevents_done_and_retry_finishes"
        - "cargo test --lib commands::init::cleanup::archive_tests::cleanup_work_directory_keeps_state_when_archiving_fails"
        - "cargo test --lib commands::clean::tests::clean_state_directory_keeps_state_when_archiving_fails"
        - "cargo test --lib fs::memory::archive_tests::archive_failure_without_journals_only_warns"
        - "cargo test --lib commands::memory::handlers::tests_resolve_pending::archived_journals_copied_into_a_new_state_dir_are_pending"
        - "cargo test --lib commands::knowledge::tests_annotate::a_section_annotated_non_current_leaves_default_retrieval"
        - "cargo test --lib fs::knowledge::dir::tests_bugfixes::a_fresh_scaffold_reads_as_empty_and_a_section_reads_as_content"
        - "cargo test --lib plan::schema::tests::knowledge_recommendations_tests::a_populated_knowledge_tree_needs_no_bootstrap"
        - "cargo test --lib plan::schema::tests::knowledge_recommendations_tests::a_scaffold_only_tree_still_warns"
        - "cargo test --lib orchestrator::signals::tests::tests_brief::tests_brief_rendering::integration_verify_signal_teaches_memory_and_no_knowledge_writes"
        - "cargo test --lib commands::memory::handlers::prefix::tests::found_and_gotcha_prefixes_are_one_class"
        - "cargo test --test doctrine_guard_contracts"
        - "cargo test --test integration plan_verify::"
        - "cargo test --test plan_environment_contracts"
        - "cargo test --test maintainability"
        - "cargo test --doc --workspace --all-features -- --test-threads=1"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - "../scripts/check-hook-syntax.sh"
        - "bash ../loom-hooks/tests/run-all.sh"
        - "LOOM_HOOK_TEST_BSD=1 bash ../loom-hooks/tests/run-all.sh"
        - 'rg -q -F "learning-validator-stage-journal.sh" ../loom-hooks/tests/run-all.sh'
        - 'rg -q -F "other-stage.md" ../loom-hooks/tests/commit-guard-memory-reminder.sh'
        - 'rg -q -F "loom memory list --stage test-stage" ../loom-hooks/tests/session-start-compact.sh'
        - 'rg -q -F "loom memory list --stage" ../loom-hooks/session-start.sh'
        - 'rg -q -F "loom memory list --stage" ../CLAUDE.md.template'
        - 'rg -q -F "loom memory list --stage {}" src/orchestrator/signals/format/sections.rs'
        - 'rg -q -F "any other value fails when the plan is parsed" ../README.md'
        - 'rg -q -F "**Never skipped.**" ../skills/loom-plan-writer/references/bookend-stages.md'
        - '! rg -q -e "fn write_summary" -e "fn generate_summary" -e "fn handle_context_warning" -e "pub summary:" src/fs/memory src/orchestrator/monitor'
        - '! rg -q -F "remind_knowledge_capture" ../loom-hooks/commit-guard.sh'
        - '! rg -q -F "loom knowledge update mistakes" ../loom-hooks/post-tool-use.sh'
        - '! rg -q -F -e "prune stale/duplicate knowledge entries" -e "only write path" -e "only knowledge write path" -e "stage hold/release" ../skills/loom-usage/SKILL.md'
        - '! rg -q -F -e "memory/<session>.md" -e "per-session journals" -e "applies **no** namespace" -e "major bugs, visual/UI design" -e "intentionally protected by sandbox defaults" ../README.md'
        - '! rg -q -F "YOU (opus)" src/orchestrator/signals/format/codex.rs'
        - '! rg -q -F "**Skip ONLY if** the plan produces no new knowledge" ../skills/loom-plan-writer/references/bookend-stages.md'
        - '! rg -q -F -e "main-agent FOUNDATION step" -e "auto-get write access" -e "loom review prunes" -e "to prune stale entries" -e "append-only files rot" -e "headings from the scaffold on" -e "scaffold already has" ../skills/loom-plan-writer'
        - '! rg -q -F "\"doc/loom/knowledge/**\"]" ../skills/loom-plan-writer/references/sandbox.md'
        - '! rg -q -F "append-only" src/fs/knowledge/templates.rs src/fs/knowledge/dir.rs'
      before_stage:
        - command: "cargo --version"
          description: "cargo on PATH"
        - command: "rg --version"
          description: "rg on PATH for the acceptance checks and the hook tests"
        - command: "jq --version"
          description: "jq on PATH for the hook tests"
        - command: "test -f ../doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md && test -f ../doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/verifier.md"
          description: "the worker briefs are committed (loom run does not bundle briefs outside doc/plans/briefs/)"
      after_stage:
        - command: 'rg -q -F "pub fn check_knowledge_recommendations" src/plan/schema/validation/knowledge_recommendations.rs'
          description: "the bootstrap warning lives in its own module"
        - command: '! rg -q -F "fn check_knowledge_recommendations" src/plan/schema/validation.rs'
          description: "validation.rs no longer defines the bootstrap warning, so its ledger row can only fall"
        - command: 'rg -q -F "remind_memory_capture" ../loom-hooks/post-tool-use.sh'
          description: "the post-commit reminder is the memory reminder later plans anchor on"
      artifacts:
        - "src/plan/schema/validation/v2_lints/bookends.rs"
        - "src/plan/schema/validation/knowledge_recommendations.rs"
        - "src/orchestrator/signals/tests_doctrine_commands.rs"
        - "src/plan/schema/tests/stage_model_tests.rs"
        - "src/plan/schema/tests/v2_lint_bookend_tests.rs"
        - "tests/doctrine_guard_contracts.rs"
      wiring:
        - source: "src/orchestrator/signals/format/sections.rs"
          pattern: "StageType::Standard | StageType::IntegrationVerify =>"
          literal: true
          description: "integration-verify signals take the standard memory arm"
        - source: "src/plan/schema/validation/v2_lints/mod.rs"
          pattern: "bookends::check(ctx, &mut out);"
          literal: true
          description: "the bookend lints run inside v2_lints::run"
        - source: "src/plan/schema/tests/v2_lint_tests.rs"
          pattern: "#[path = \"v2_lint_bookend_tests.rs\"]"
          literal: true
          description: "the bookend lint unit tests are compiled into the schema test tree"
        - source: "src/commands/plan/verify.rs"
          pattern: "check_knowledge_recommendations(stages, repo_root.as_deref())"
          literal: true
          description: "plan verify passes the repository root to the bootstrap warning"
        - source: "src/commands/init/plan_setup.rs"
          pattern: "check_knowledge_recommendations(&parsed_plan.stages, Some(repo_root))"
          literal: true
          description: "loom init passes the repository root to the bootstrap warning"
        - source: "src/plan/schema/types.rs"
          pattern: "deserialize_with = \"deserialize_stage_model\""
          literal: true
          description: "the stage model field is validated when the plan is parsed"
        - source: "src/commands/init/cleanup.rs"
          pattern: "archive_run_state(&work_dir, &main_root, plan_id.as_deref())?;"
          literal: true
          description: "loom init --clean stops when archiving fails"
        - source: "src/commands/clean/mod.rs"
          pattern: "archive_run_state(&work_dir, &main_root, plan_id.as_deref())?;"
          literal: true
          description: "loom clean stops when archiving fails"
        - source: "src/fs/plan_lifecycle.rs"
          pattern: "archive_run_state_before_done(work_dir, &root)?;"
          literal: true
          description: "plan completion stops before the DONE rename when archiving fails"

    - id: integration-verify
      name: "Integration verification"
      summary: "Runs the full gate and the hook suites on the merged tree, reviews the change, and proves the bootstrap fix and the new lints with the worktree binary."
      stage_type: integration-verify
      skills: ["loom-rust"]
      subagent_timeout_secs: 900
      working_dir: "loom"
      dependencies: ["doctrine-and-guards"]
      description: |
        Final verification after doctrine-and-guards. Verify functional integration, not only passing tests. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: read the plan (doc/plans/token-optim/, IN_PROGRESS- prefix once loom run starts), loom memory show --all, doc/loom/knowledge/INDEX.md and only the sections it points to for the changed areas, and doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/integration-verify/functional-checks.md.
        BUILD AND TEST (zero tolerance; nothing is pre-existing): every acceptance command below (the repository's pre-push gate steps that run confined, the doctest run that --all-targets leaves out, and CI's hook steps), plus the four steps functional-checks.md lists under "Gate steps outside acceptance" (markdownlint, cargo audit, scripts/flake-check.sh, scripts/test-pre-commit-partial-staging.sh), which need network or writable caches that confined acceptance lacks: the canonical verifier runs them in the session sandbox.
        CODE REVIEW: the review round the signal names, with the focus areas in functional-checks.md; an engineer agent fixes every finding, or it is disputed; never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each one implemented with loom memory resolve <id> --outcome implemented --reason <what changed>; leave the rest pending for knowledge-distill.
        FUNCTIONAL: the checks in functional-checks.md, with the worktree binary (loom/target/debug/loom, the sanctioned exception to loom from PATH: the installed binary has none of this plan's changes). Record each result with loom memory note.
        Record discoveries to loom memory for knowledge-distill, including any knowledge file the tree contradicts: loom memory note "stale-knowledge: ...".
      acceptance:
        - "cargo build"
        - "cargo test --all-targets --no-fail-fast"
        - "cargo test --doc --workspace --all-features -- --test-threads=1"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - "../scripts/check-hook-syntax.sh"
        - "bash ../loom-hooks/tests/run-all.sh"
        - "LOOM_HOOK_TEST_BSD=1 bash ../loom-hooks/tests/run-all.sh"
        - 'L="$PWD/target/debug/loom" && H=$(mktemp -d "${TMPDIR:-/tmp}/v2valid.XXXXXX") && [ -n "$H" ] && cp tests/fixtures/plans/v2-valid.md "$H/PLAN.md" && (cd "$H" && git init -q && "$L" plan verify PLAN.md >/dev/null); s=$?; rm -rf "$H"; exit $s'
      wiring:
        - source: "src/plan/schema/validation/v2_lints/mod.rs"
          pattern: "bookends::check(ctx, &mut out);"
          literal: true
          description: "the bookend lints stay registered on the merged tree"

    - id: knowledge-distill
      name: "Knowledge distillation"
      summary: "Makes the knowledge check green, rewrites the knowledge sections this plan made false, and curates every stage memory with a receipt."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries; lean on them and keep code spot-reads narrow.
        Follow doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/knowledge-distill/curation.md: it lists the structural issues to fix, the freshness metadata to add, and the knowledge sections this plan made false.
        START with loom memory pending --group; read the plan and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge: memory in place with loom knowledge replace-section <file> "<heading>" "<body>"; never loom knowledge update, which appends the fix below the stale text.
        Then curate mistakes (prevention rules), patterns, decisions and conventions via loom knowledge update. TIER ROUTING: findings of about 40 lines or fewer go inline in the tier-1 file; larger findings go to loom knowledge update <category>/<slug> with a 2-4 line tier-1 summary and link. INDEX.md regenerates on every knowledge write.
        README.md was updated by doctrine-and-guards; change it here only to correct a claim the merged tree contradicts.
        SUGGESTIONS: record every unimplemented reviewer suggestion (listed under suggestions by loom memory pending --group) in concerns or the topic it belongs to, then resolve it promoted, merged or discarded.
        RECEIPTS: every Note/Decision/Question taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it; finish with loom memory pending --strict and resolve whatever it lists.
        LAST, if this stage removed structural issues, ratchet the baseline: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
        - 'rg -q "^sources:" doc/loom/knowledge/architecture/owned-waits.md'
        - 'rg -q "^verified:" doc/loom/knowledge/architecture/owned-waits.md'
        - 'rg -q "^sources:" doc/loom/knowledge/architecture/hook-system.md'
        - '! rg -q -F -e "write_summary" -e "generate_summary" -e "remind_knowledge_capture" -e "check_knowledge_recommendations(&stages)" -e "in implementation stages" doc/loom/knowledge --glob "!mistakes.md" --glob "!mistakes/**"'
      files:
        - "doc/loom/knowledge/**"
        - "README.md"
```

<!-- END loom METADATA -->
