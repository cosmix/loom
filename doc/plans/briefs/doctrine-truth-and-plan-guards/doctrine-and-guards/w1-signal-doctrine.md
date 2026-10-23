# W1: signal doctrine and note prefixes

Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first.

## Files you own

- `src/orchestrator/signals/format/sections.rs`
- `src/orchestrator/signals/format/codex.rs`
- `src/orchestrator/signals/tests_brief_rendering.rs`
- `src/orchestrator/signals/tests_cache.rs`
- `src/orchestrator/signals/tests_size.rs`
- `src/commands/memory/handlers/prefix.rs`

Read-only context: `src/orchestrator/signals/format/section_boxes.rs`, `src/orchestrator/signals/cache.rs` (`generate_integration_verify_stable_prefix`), `src/orchestrator/signals/tests.rs` (`create_test_session`, `create_test_stage`, `create_test_worktree`).

## 1. Integration-verify stops getting knowledge-curation doctrine

Today an integration-verify (IV) signal prints the "KNOWLEDGE UPDATES REQUIRED" box and the `## Knowledge Management` arm with a `loom knowledge update ...` command table, while the IV stable prefix (`cache.rs`, `generate_integration_verify_stable_prefix`: "Knowledge distillation is handled by a separate knowledge-distill stage") and the plan-writer say IV never curates knowledge. IV joins the standard arm.

In `format_semi_stable_section` (`sections.rs`, starts at the `pub(super) fn format_semi_stable_section(` line, about 23):

1. The box gate (about lines 37-46): change the `matches!` to `StageType::Knowledge | StageType::KnowledgeDistill` and drop `StageType::IntegrationVerify`. Rewrite the comment above it: knowledge and knowledge-distill stages get the box; standard and integration-verify stages get the compact memory reminder under "## Stage Memory" below.
2. The match arm (about line 50): `StageType::Knowledge | StageType::KnowledgeDistill => {`. Rewrite the comment `// Show knowledge commands table for Knowledge and IntegrationVerify stages` (about line 136) to name Knowledge and KnowledgeDistill.
3. The standard arm (about line 199): `StageType::Standard | StageType::IntegrationVerify => {`. Rewrite its comment to say implementation and integration-verify stages. Replace the two-line literal
   `"⚠️ NEVER use 'loom knowledge' in implementation stages — curated into \` / `knowledge by knowledge-distill.\n"`
   with
   `"⚠️ NEVER use 'loom knowledge' in this stage — knowledge-distill curates \` / `memory into knowledge.\n"`
   (same two-line shape).
4. Do not touch `format_dynamic_section` (ledger row 205 stays).

The `matches!` stays four lines after rustfmt: its arguments, `stage_type, StageType::Knowledge | StageType::KnowledgeDistill`, are 62 columns, over rustfmt's 60-column call-argument width. Keep each comment rewrite to the line count of the comment it replaces, so the function stays near 278 and the file near 569. Report both final lengths; the main agent sets the two ledger rows to what `cargo test --test maintainability` reports.

## 2. Recovery text names the stage's own journal

`loom memory list` with no flag lists every journal in the plan (`src/cli/types_memory.rs`, `List`). In `format_recitation_section` (`sections.rs`, the line `content.push_str("- \`loom memory list\` - Review your stage entries\n");`, about 562) write the stage's own id:

```rust
content.push_str(&format!("- `loom memory list --stage {}`\n", stage.id));
```

Write it exactly so, with no trailing description. It must stay one line after `cargo fmt` so the function stays at exactly 58 lines (ledger row). rustfmt wraps a call whose arguments pass 60 columns: with the old `- Review your stage entries` text, or even `- Yours`, this call becomes four lines (checked with rustfmt 1.9.0, edition 2021). The form above stays one line.

## 3. The Codex section names no model

`format/codex.rs`, the literal starting `"Regardless of the list: YOU (opus) keep the work needing architectural judgment,\n\` (about line 90). The section is cache-stable and the stage's model is configurable, so it names the role instead. Rewrap the three-line literal to stay within 100 columns:

```text
Regardless of the list: YOU, the stage's main agent, keep the work needing
architectural judgment, loom-advisor (fable) is available on a second failure,
and verification never moves to codex - see below.
```

Then `rg -n "YOU \(opus\)|architectural judgment" src/orchestrator/signals` and update any test pin that quotes the old wording, keeping `test_signal_codex_implementers_section_gated` at exactly 116 lines (ledger row). A pin that quotes a fragment still present needs no change.

## 4. Tests

`tests_cache.rs` (ledger row 630; the file must end at 629):

- In the standard-signal test that asserts `content.contains("NEVER use 'loom knowledge' in implementation stages")` (about line 197), change the pinned text to `NEVER use 'loom knowledge' in this stage`.
- Replace the two lines `assert!(!content.contains("loom knowledge update entry-points"));` and `assert!(!content.contains("loom knowledge update patterns"));` (about 203-204) with the one line `assert!(!content.contains("loom knowledge update"));`.
- The knowledge-stage test (about lines 210-248) is unchanged: knowledge stages keep the command table.

`tests_brief_rendering.rs` (161 lines, not in the ledger):

- Generalise the helper `integration_verify_signal_without_a_pack(knowledge_tree_empty: bool)` (about line 115) to `signal_without_a_pack(stage_type: StageType, knowledge_tree_empty: bool) -> String`.
- `a_populated_tree_never_claims_the_knowledge_base_is_empty` and `an_empty_tree_still_gets_the_warning_and_the_exploration_ladder` render `StageType::Knowledge`, which still owns the Knowledge Management arm; keep their assertions and doc comments otherwise (the gate on `knowledge_tree_empty`, not on the pack, is what they pin).
- Add `integration_verify_signal_teaches_memory_and_no_knowledge_writes`: for both `knowledge_tree_empty` values, the IV signal contains `NEVER use 'loom knowledge' in this stage` and none of `## Knowledge Management`, `KNOWLEDGE UPDATES REQUIRED`, `loom knowledge update`, `CRITICAL: KNOWLEDGE BASE IS EMPTY`.

`tests_size.rs`, the doc comment of `CLAUDE_MD_TEMPLATE_MAX_BYTES` (about line 36): `Actual size is now about 17,000 bytes.` becomes `Actual size is about 19,900 bytes.` (the template is 19,916 bytes and W3 adds about 20).

## 5. Note prefixes

The stage signal teaches `found:` and `gotcha:` (`sections.rs`, the Surprise and Gotcha rows of the standard arm), the subagent preamble teaches `found/gotcha:` (`loom-hooks/_subagent-preamble.txt:34`), and `NotePrefix::parse` (`src/commands/memory/handlers/prefix.rs`) recognises only `found/gotcha:`. Journals hold 129 `gotcha:`, 128 `found:` and 28 `found/gotcha:` notes. Make `NotePrefix::parse` return `NotePrefix::Found` for a text starting with `found:`, `gotcha:` or `found/gotcha:`. Update the module doc (line 3) and the `Found` variant doc to name the three forms. Replace the test `found_gotcha_prefix_is_recognized` with `found_and_gotcha_prefixes_are_one_class`, which asserts all three forms parse to `Found` and that `foundation: x` parses to `NotePrefix::None`. Nothing reads `NotePrefix::Found` today (`pending_groups.rs` sorts notes as mistake, stale-knowledge or other; `validate_note_shape` checks only the mistake and stale-knowledge forms), so the change makes the parser agree with the signal and the preamble and changes no grouping or validation. Do not add a `found` group.

## Check (run once, at the end)

```bash
cargo test --lib -- orchestrator::signals:: commands::memory::handlers::prefix::
```

Report: the final line counts of `sections.rs`, `format_semi_stable_section`, `format_recitation_section`, `tests_cache.rs`, and `test_signal_codex_implementers_section_gated`; every pin you changed.
