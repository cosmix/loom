# W4: delete the memory summary path; correct the knowledge scaffold; prove section state leaves retrieval

Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first.

## Files you own

- `src/orchestrator/monitor/handlers.rs`
- `src/orchestrator/monitor/detection.rs`
- `src/fs/memory/mod.rs`
- `src/fs/memory/storage.rs`
- `src/fs/memory/query.rs`
- `src/fs/memory/types.rs`
- `src/fs/memory/export.rs`
- `src/fs/memory/export_suggestions.rs`
- `src/fs/knowledge/templates.rs`
- `src/fs/knowledge/dir.rs`
- `src/fs/knowledge/tests_dir_bugfixes.rs`
- `src/commands/knowledge/tests_annotate.rs`

Do not touch `src/fs/memory/archive.rs` or `src/fs/memory/tests/archive.rs` (W6 owns them). `src/fs/memory/parser.rs` needs no edit: `parse_journal` builds its `MemoryJournal` with `..Default::default()`, and its `## Summary` branch stays, because a journal written before this stage can still hold one and the branch only ends the current entry.

## 1. The memory summary path

When a session enters the Yellow context band, `Detection::detect_context_health` (`detection.rs`, about lines 194-199) calls `Handlers::handle_context_warning` (`handlers.rs`, about lines 88-112), which builds a `## Summary` block with `generate_summary` (`fs/memory/query.rs`) and appends it to the journal with `write_summary` (`fs/memory/storage.rs`). The parser ends an entry at `## Summary` and never reads the block (`fs/memory/parser.rs`, about lines 26-29), and `MemoryJournal.summary` (`fs/memory/types.rs`, about line 215) is never read. Delete the path:

1. `detection.rs`: delete the `if let Err(e) = handlers.handle_context_warning(session) { eprintln!(...) }` block in the `ContextHealth::Yellow` arm. Keep `events.push(MonitorEvent::SessionContextWarning { ... })`: the event stays.
2. `handlers.rs`: delete `handle_context_warning` and its doc comment; remove `generate_summary`, `write_summary` and any other name the deletion leaves unused from the `use` at about line 8 (check `read_journal`).
3. `storage.rs`: delete `write_summary` (about lines 123-139) and its doc comment. `read_journal` builds its `MemoryJournal` with `..Default::default()`, so it needs no edit; `std::io::Write` stays (`append_entry` and `create_journal` call `write_all`).
4. `query.rs`: delete `generate_summary`; remove the imports it alone used (`push_suggestions_section`, `truncate_for_display`, and any other `cargo clippy` names); the module doc becomes "Query functions for memory journals." Keep `entries_of_type`: `export.rs` and `export_suggestions.rs` use it.
5. `types.rs`: delete the `summary` field and its doc line from `MemoryJournal`.
6. `mod.rs`: drop `write_summary` from the `pub use storage::{...}` list and `generate_summary` from `pub use query::{...}`; delete the test `test_generate_summary`; remove `summary: None` from the other test's literal (about line 286).
7. `export_suggestions.rs`: its module doc says `### Suggestions` sections appear in memory summaries, signals and handoffs; make it signals and handoffs. In the test `summary_signal_and_handoff_list_suggestions_with_their_ids`, drop `generate_summary(&journal, 5)` from the rendered list and the `journal` literal it needed, drop the unused imports, and rename the test `signal_and_handoff_list_suggestions_with_their_ids`.
8. `export.rs`: `pub(super) use suggestions::push_suggestions_section;` (about line 11): `query.rs` was its only user outside `export.rs`, and `export.rs` calls it itself (about lines 80 and 150). Narrow it to a private `use` so clippy reports nothing.

Before finishing, run `rg -n "write_summary|generate_summary|handle_context_warning|\.summary\b" src tests` from `loom/` and confirm no hit refers to the memory journal.

## 2. The knowledge scaffold sentence

`default_content` (`src/fs/knowledge/templates.rs`, ledger row `default_content 62`) stamps `> This file is append-only - agents add discoveries, never delete.` into all seven tier-1 files. `loom knowledge replace-section` and `delete-section` exist, so the sentence is false. Replace each of the seven lines with this one line:

```text
> Agents edit this file through `loom knowledge update`, `replace-section` and `delete-section`.
```

One line for one line: `default_content` stays at exactly 62 lines.

`KnowledgeDir::has_content` (`src/fs/knowledge/dir.rs`, about lines 52-77) counts a tier-1 file as content when it holds a `##` line without `(Add` or `append-only`. The scaffold has no `##` line, so the `append-only` exclusion matches nothing once the sentence is gone: remove `&& !line.contains("append-only")` and rewrite the comment above the check to say the scaffold carries no `##` heading, so any `##` heading is content. W5's bootstrap warning and `orchestrator/signals/retrieval.rs` (the `knowledge_tree_empty` flag) call `has_content`; its signature stays.

In the same file, the doc comments of `append` (about line 130, `(knowledge files are append-only)`) and `append_target` (about line 188, `(both are append-only)`) become `(\`update\` appends; \`replace-section\` and \`delete-section\` edit in place)`.

Add to `tests_dir_bugfixes.rs` (184 lines) a test `a_fresh_scaffold_reads_as_empty_and_a_section_reads_as_content`: `KnowledgeDir::new(temp)` then `initialize()`; assert `!has_content()`; then write a tier-1 file with `# Architecture\n\n## Module map\n\n- src/main.rs\n` (through the directory's own write API or `fs::write` to `file_path(KnowledgeFile::Architecture)`); assert `has_content()`.

## 3. A section marked non-current leaves retrieval

`loom knowledge annotate <target> --section <heading> --state <value>` writes a `<!-- state: ... -->` marker under the heading through `KnowledgeDir::set_section_state_target` (`src/fs/knowledge/dir.rs`, about line 284; called from `commands/knowledge/annotate.rs`, `annotate_section`). Retrieval's default policy, `LifecyclePolicy::Current` (`src/context/schema/lifecycle.rs`), admits `Active` and `Draft` chunks only. The existing tests prove each half on its own: `section_state_writes_then_replaces_the_marker_under_the_heading` checks the writer, and `a_historical_section_is_retrieved_only_under_the_historical_policy` checks retrieval against a marker written by hand. Add one test to `src/commands/knowledge/tests_annotate.rs` (235 lines) that drives both through the writer:

`a_section_annotated_non_current_leaves_default_retrieval`: for each of `LifecycleState::Deprecated`, `LifecycleState::Superseded` and `LifecycleState::Historical`, in a fresh `TempDir` (so no retrieval cache carries over), create `.loom/work`, write `doc/loom/knowledge/architecture/topic.md` holding two sections (`## Current notes` with unrelated text, `## Old episode` with `quokka-zebra archival wording`), call `KnowledgeDir::new(temp.path()).set_section_state_target(&KnowledgeTarget::parse("architecture/topic").unwrap(), "Old episode", state)`, then `retrieve_for_stage(&StageQuery::new(temp.path(), "quokka zebra archival"), 2_000)`: no returned item carries `state`, and no item `id` names the `Old episode` section. A control on the same fixture with no annotation, in its own `TempDir`, returns an item whose `id` names that section, so the assertion cannot pass on a fixture retrieval never finds. Use the imports the historical test already uses (`crate::context::retrieve::{retrieve_for_stage, StageQuery}`). Keep the function under 50 lines; a small helper for the per-state fixture is fine.

## Check (run once, at the end)

```bash
cargo test --lib -- fs::memory:: fs::knowledge:: orchestrator::monitor:: commands::knowledge::tests_annotate
```

Report: every file you changed, the final length of `default_content` (must be 62), the `id` format the control matched, and anything a sibling worker's file needs because of the removed field.
