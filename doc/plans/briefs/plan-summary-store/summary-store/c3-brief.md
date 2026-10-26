# C3: the rebuild brief

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/plan-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`loom/src/planning/summary/brief.rs`, `loom/src/planning/summary/brief/tests.rs` (seeded).

## Files to read

`doc/plans/briefs/plan-summary-store/summary-store/w3-commands.md` section "brief.rs" (the behaviour), `loom/src/planning/summary/claims.rs` and `evaluate.rs` (W2 wrote them in the previous wave: `create_claim`, `release`, `evaluate`; the base-layer graph does not show them), `loom/src/context/untrusted.rs` line 123 (`inline_safe`).

## Entry points

`pub fn brief(root: &Path, paths: &RuntimePaths, ids: &[String], note: Option<&str>) -> anyhow::Result<String>`.

## Steps

1. Write `brief.rs`: validate ids, create every claim first and undo the ones made on a refusal, then build the text (ids, kinds, source paths, current text with a stale or keep prefix per sentence, the note beside its sentence, the body rules, the absolute draft path, the exact seal command).
2. Write `brief/tests.rs`: a refused second brief, the note placement, the escape byte stripped from a source title.
3. cargo check --tests.

## Done when

The orchestrator runs `cargo test --lib planning::summary::brief::` and `cargo test --test planning_summary_cli_contracts brief_prints_note_and_marks`.

## Traps

Every line of document text passes through inline_safe. The seal command printed is `loom plan summary seal <id>` without --force.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Every read of doc/, drafts/ or claims/ goes through crate::fs::safe_read; never std::fs::read on them.
