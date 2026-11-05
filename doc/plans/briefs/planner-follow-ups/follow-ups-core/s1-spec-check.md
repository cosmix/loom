# S1: integration-verify spec check, and the run review's Open Findings (R4)

Tier: opus (`loom-senior-software-engineer`, common.md names opus for the integration-verify spec check). Read `common.md` first. Knowledge: `architecture/signal-generation.md` ("Signal Sections: Semi-Stable, Dynamic, Recitation and EmbeddedContext"), `architecture/planning-document-model.md` ("Kinds, Ids and Slugs"). Anchor edits by symbol; line numbers are from the stage base.

## Settled decisions (do not reopen)

- **Where the duty lives.** The plan text puts a "Spec conformance" duty in `cache.rs` `generate_integration_verify_stable_prefix`. It goes in the dynamic `## Product Sections` block instead, emitted only when the plan names sections: the duty has nothing to act on otherwise, and `cache.rs` is pinned at exactly 511 lines in `loom/maintainability-baseline.txt` (a ratchet file; any change raises `TI-ratchet`). Do not edit `cache.rs`. Record this with `loom memory decision`.
- **Plan source and feature.** The plan's repository-relative source path is `crate::fs::get_source_path(work_dir)` (`config.toml` `[plan] source_path`, written relative by `commands/init/plan_setup.rs:199`); its text is read from `crate::fs::resolve_source_path(work_dir)`. The feature is `crate::planning::kinds::classify(rel)` = `DocKind::Plan { feature: Some(f), .. }`. A standalone plan, an absolute or unclassifiable path, a missing or unreadable file, `Implements: none` and no `Implements:` line all emit nothing. A read error is not fatal to signal generation: log it with `tracing::warn!` and emit nothing.
- **Section paths** come from `crate::planning::kinds::section_path(f, n)` (`doc/product/<f>/section_<nn>.md`), in declaration order, duplicates dropped. Bad tokens are ignored (the plan check already reports them).
- **Outcomes are tagged.** The duty tells the agent to prefix its records with `spec-conformance:`, and the run review lists those records in a `## Spec Conformance` section, so the Built state (stage built-and-new) reads spec outcomes with the same parser as the findings.
- **What an open finding is.** The run review is generated after knowledge-distill (`fs/plan_lifecycle.rs` `mark_plan_done_if_all_merged`), which resolves every memory entry, so "pending at plan completion" is always empty. An open finding is therefore:
  - a `Suggestion` entry whose outcome is not `Implemented` or `Discarded` (none yet, `Promoted`, `Merged` or `Deferred`): recorded, never built;
  - a `Note` or `Decision` entry whose outcome is `Deferred`.
  An entry's outcome is the `ReceiptOutcome` of its receipt with the latest timestamp, across every journal (knowledge-distill's journal holds the receipts of other stages' entries). Questions stay in `## Open Questions`. Record this definition with `loom memory decision`.
- **One bullet format** for `## Spec Conformance`, `## Open Questions` and `## Open Findings`: `- **<stage name>** (`<stage-id>`): <text>`. The generator escapes `\` as `\\` and `*` as `\*` in the stage name and collapses every whitespace run in the text (newlines included) to one space, trimmed. The parser takes the name up to the first unescaped `**`, then requires `` (`<id>`): `` with `<id>` matching `crate::validation::validate_id`'s character set (`[A-Za-z0-9_-]+`), unescapes the name, and takes the rest of the line as the text. A line that does not match is skipped. Empty sections say `No spec conformance outcomes.`, `No open questions.` (as today) and `No open findings.`. Section order: Summary, Changes by Stage, Spec Conformance, Open Questions, Open Findings.
- **Layering.** `planning` already depends on `fs`; `fs` never imports `planning`. So the writer and the heading constants live in `fs::plan_review`, and the one parser lives in `planning::run_review`, importing the constants. The round-trip contract proves they agree.

## Build

1. `loom/src/planning/implements.rs`: add
   `pub fn implemented_section_paths(plan_rel: &Path, text: &str) -> Vec<PathBuf>` per the decisions (uses `parse`, `kinds::classify`, `kinds::section_path`). Unit tests in `loom/src/planning/tests/implements_tests.rs`.
2. `loom/src/orchestrator/signals/generate/product_sections.rs` (new): `pub(super) fn append_product_sections(content: &mut String, stage: &Stage, work_dir: &Path)`: only for `StageType::IntegrationVerify`; renders, when the list is non-empty:

   ```text
   ## Product Sections

   This plan implements these product sections. Paths are relative to the worktree root.

   - `doc/product/<f>/section_<nn>.md`

   **Spec conformance.** Read every section above. For each behaviour a section states that the build lacks or contradicts, fix the code. When the build is right and the section is wrong, edit the section to match the build and record `loom memory decision "spec-conformance: <section path>: <what changed and why>"`. A gap you neither fix nor correct becomes `loom memory question "spec-conformance: <section path>: <the gap>"`.
   ```

   Keep a pure core (`fn product_sections_block(paths: &[PathBuf]) -> Option<String>`) so tests need no work dir; plus one test through `append_product_sections` with a work dir (find how `resolve_source_path` resolves the project root for a plain temp layout before building the fixture; `fs/mod.rs:130`). Tests: the block for an IV stage, nothing for a `Standard` stage, nothing without `config.toml`.
3. `loom/src/orchestrator/signals/generate.rs` (395 lines; stay under 400): `mod product_sections;` beside `mod findings;`, and in `assemble_signal` one call `product_sections::append_product_sections(&mut formatted.content, stage, work_dir);` placed after `append_v2_section` and before `append_stage_feedback` (adjudicator feedback stays last). Keep that exact call text: a wiring check greps it.
4. **Sandbox.** Confirm the integration-verify stage can write `doc/product/<f>/` in its worktree: `sandbox::config` `merged` and `apply_knowledge_write_grant`, `sandbox/settings.rs` emission (~256), `loom-hooks/worktree-file-guard.sh`. At the base nothing denies it (only `doc/loom/knowledge` is special-cased, and plans deny only `../../**`). If that holds, change nothing and record `loom memory decision "integration-verify may write doc/product: <evidence file:line>"`; if something denies it, report the exact rule instead of widening the sandbox.
5. `loom/src/fs/plan_review/`: new `sections.rs` holding `pub const SPEC_CONFORMANCE_HEADING`, `OPEN_QUESTIONS_HEADING`, `OPEN_FINDINGS_HEADING` (the `## ...` lines), `pub fn review_item_line(stage_name: &str, stage_id: &str, text: &str) -> String` (one line, no newline), `append_spec_conformance`, `append_questions` (moved from `mod.rs`, now in the shared format), `append_open_findings`. `mod.rs` declares `mod sections;`, re-exports the public items, and `render` calls `append_spec_conformance`, `append_questions`, `append_open_findings(&mut doc, stages, journals)` in that order (keep the call text `append_open_findings(`: a wiring check greps it in `mod.rs`). Spec conformance lists `Decision` and `Question` entries whose content starts with `spec-conformance:`, in stage order then journal order. Unit tests in `loom/src/fs/plan_review/tests.rs` (`#[cfg(test)] mod tests;`): selection by outcome, latest receipt wins, the escape and whitespace rules, empty sections.
6. `loom/src/planning/run_review.rs` (new; `pub mod run_review;` in `loom/src/planning/mod.rs`, alphabetical):

   ```rust
   /// A run review section written in the shared bullet format.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum ReviewSection { SpecConformance, OpenQuestions, OpenFindings }
   /// One bullet of a run review section.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct ReviewItem { pub stage_name: String, pub stage_id: String, pub text: String }
   /// The section's items in document order; empty when the section is absent or says it has none.
   pub fn review_items(review: &str, section: ReviewSection) -> Vec<ReviewItem>
   ```

   A section runs from its heading line to the next line starting `## `. Parser unit tests in `loom/src/planning/run_review/tests.rs`: a name holding `**`, `\` and `: `, a text holding `` `): `` and `**`, a malformed bullet skipped, an absent section.
7. Knowledge is stale after this: `architecture/signal-generation.md` lists the dynamic section contents without `## Product Sections`. Record `loom memory note "stale-knowledge: ..."` for it.

## Contracts (written before you start; make them pass, never edit them)

`loom/tests/spec_check_contracts.rs`: `a_feature_plan_lists_its_implemented_sections`, `an_unmapped_or_standalone_plan_lists_no_sections`, `the_run_review_round_trips_its_open_findings`, `the_run_review_lists_unimplemented_suggestions_and_deferred_entries`. Read them first: they pin the public surface above (`loom::planning::implements::implemented_section_paths`, `loom::fs::plan_review::generate`, `loom::planning::run_review::{review_items, ReviewSection, ReviewItem}`).

## Files

Yours: the files named above, plus `loom/src/planning/tests/implements_tests.rs`. Never edit R1's, R2's, W1's or D1's files (the plan's worker table). D1 writes the duty into `skills/loom-plan-writer/references/`; if your final block text differs from the one above, say so in your report.

## Run once at the end

`cargo test --manifest-path loom/Cargo.toml --lib -- orchestrator::signals::generate:: fs::plan_review:: planning::run_review:: planning::tests::implements_tests::`

R1 and R2 edit the same crate at the same time; a compile error in a file you do not own is theirs: report it.
