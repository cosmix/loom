# D3: rendering, windows, hook floor, view-backed retrieval, eval cases

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside D1 and D2.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 11
  (`read_window`) and section 13 (the Windows, Hook emit floor and Graph source
  bullets, plus how `explanation`, `caveat` and `text_search` render).

## Goal

The brief, `loom knowledge context` and the prompt hook render the new fields.

- Stage briefs and `loom knowledge context` attach up to two anchored source windows.
- The hook floor admits symbol questions.
- Retrieval reads the resolved view, so expansion sees cross-file edges.
- The eval case file gains positive and negative cases for the new intents.

## Files you own (write)

- `loom/src/context/render.rs`: `render_source_entry` and `rendered_item_tokens`
  render `explanation`, `caveat` and windows, and charge them
- `loom/src/orchestrator/signals/format/brief.rs`: the source section and the
  text-search line. The freshness word belongs to stage `map-api-freshness`; leave it.
- `loom/src/orchestrator/signals/format/brief_tests.rs`: source-section assertions
- `loom/src/commands/knowledge/context.rs`: human output for explanation, caveat,
  text-search and windows. It is at 365 lines, so put new rendering in a new
  `commands/knowledge/context_render.rs`, and declare it in
  `loom/src/commands/knowledge/mod.rs` (that declaration is yours)
- `loom/src/commands/hook/user_prompt_compose.rs`: the emit floor
- `loom/src/commands/hook/user_prompt.rs` and `commands/hook/worker_brief.rs`: the
  surface each `StageQuery` asks as (step 3)
- `loom/src/context/retrieve/graph.rs`: `load_resolved_graph` reads the view
  (`GraphStore::view`, stage `resolved-view`) instead of `resolved()`
- new `loom/src/context/retrieve/windows.rs`: attach windows after selection
- `loom/src/context/retrieve.rs`: call `attach_windows` (one call site)
- `loom/eval/retrieval-cases.yaml`
- tests `loom/src/context/tests/retrieve_windows.rs` and `render_source_fields.rs`. D0
  created and declared both; add to them, and never edit `tests/mod.rs`.

Read-only: `rank_source*` (D1, D2), `pack*` (D2), `context/window.rs` (stage
`map-api-freshness`), `context/view/**` (stage `resolved-view`), and
`loom/tests/retrieval_delivery_contracts.rs` (frozen).

## Steps

1. **Render.**
   - `render_source_entry` appends `; <explanation>` and `; <caveat>` inside the
     parentheses that already hold reasons.
   - A window renders as an indented fenced block under its entry, with the language
     from the node's dialect.
   - `rendered_item_tokens` charges all of it. The budget counts rendered text only,
     so everything shown must be charged.
2. **Text-search line.** `brief.rs` and `context_render.rs` print
   `Literal text: the graph does not index bodies; run <command>` when
   `pack.text_search` is set.
3. **Windows** (`retrieve/windows.rs`):
   - `pub(crate) fn attach_windows(pack: &mut ContextPack, graph: &ResolvedGraph, project_root: &Path, surface: Surface)`,
     where `Surface` is `StageBrief`, `Cli` or `Hook`.
   - For `Hook` it does nothing.
   - Otherwise take at most 2 items whose reasons include `ExplicitId`, `ExactSymbol`
     or `ExactPath`, from `Full` files, in a pack whose `semantic_freshness.state()` is
     `Current`.
   - Read `window::read_window(graph, project_root, id, 12)` and store the text in
     `item.window`, a field D0 added.
   - Recompute the token estimate, and drop the window again if the pack then exceeds
     its budget.
   - `retrieve.rs` calls it once, after selection, with `query.surface`. D0 added
     `StageQuery.surface`, defaulting to `StageBrief`.
   - Set the surface at each non-default caller:
     - the stage signal is `Surface::StageBrief`: `orchestrator/signals/retrieval.rs`
       keeps the default;
     - `loom knowledge context` (`commands/knowledge/context.rs`) builds `StageQuery`
       by literal, so set `surface: Surface::Cli` there;
     - the prompt hook (`commands/hook/user_prompt.rs`) and the subagent worker brief
       (`commands/hook/worker_brief.rs`) are `Surface::Hook`
       (`.with_surface(Surface::Hook)`) and attach no windows. Those construction
       lines are yours.
4. **Hook floor** (`user_prompt_compose.rs`; anchor the edit on `admits` and
   `clears_item_floor`, never on line numbers): an item whose reasons contain
   `SymbolQuestion` passes the emit floor even with
   `matched_term_count < min_knowledge_terms`.
5. **View-backed retrieval** (`retrieve/graph.rs::load_resolved_graph`): call
   `graph_store.view(semantic_revision, Some((&plan, &stage)))` and use `view.graph`.
   On `Err`, fall back to today's `resolved()` plus a `degraded` note, never an empty
   graph without a reason.
6. **Eval cases** (`loom/eval/retrieval-cases.yaml`). Add at least these, using ids
   that exist in this repository's graph. Build each id by the id rule
   (`<path>#<kind>:<scope>`) from the definition `rg -n 'fn plan_key\b' loom/src`
   finds, and copy the style of the existing cases in the file:
   - two symbol-question cases (`what does plan_key do`, `where is
     reconcile_source_graph defined`), each expecting the id;
   - two negative cases (prose containing `plan` and `graph` as plain words, `forbid`
     the matching node ids);
   - one relationship case (`who calls reconcile_source_graph`) with `relevant`
     callers;
   - one literal case (`where do we print "source graph never built"`) that must not
     surface a source item: `forbid` the source ids the quoted words would otherwise
     surface. `abstain` applies to the whole hook and cannot express "no source item".
   - Confirm the file parses with the existing case tests
     (`cargo test --manifest-path loom/Cargo.toml --lib commands::knowledge::eval`).
7. **Tests.**
   - `tests/render_source_fields.rs`: explanation, caveat and window rendering, and
     their token charges.
   - `tests/retrieve_windows.rs`: at most 2 windows; none for Hook; none for a
     Partial file; none in a stale pack; a window dropped when over budget.
   - `brief_tests.rs`: the text-search line. `Freshness::default()` reads as
     `NeverBuilt`, so a healthy fixture sets `revision: "test-rev".into()` (D0 did this
     in the existing fixtures).
   - Hook floor: a `SymbolQuestion` item passes. Name the test
     `symbol_question_item_passes_the_emit_floor`; acceptance runs it by name.

## Traps

- Two renderers exist for source items: the brief (`brief.rs` through `render.rs`)
  and the CLI (`commands/knowledge/context.rs`). Update both.
- Hook output is compact: no windows and no explanation beyond the reason word. The
  prompt hook and the subagent worker brief use the brief renderer, so gate windows by
  `Surface` only; the explanation text is short and allowed.
- `retrieval-cases.yaml` has no `deny_unknown_fields`, so use only the existing keys.

## Your one check (run once)

`cargo test --manifest-path loom/Cargo.toml --lib context::tests:: 2>&1 | tail -15`

## Report

Report the files changed, a sample brief source section with a window and an
explanation, and the eval cases added.
