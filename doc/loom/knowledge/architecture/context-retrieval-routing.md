# Context Retrieval Routing

> Intent routing, neighbours

How a source-channel query is classified and routed, how neighbours are explained and capped, and
what a pack says about its own weakness. The base ranking and the exact-rung gate are in
[Context Retrieval](context-retrieval.md); the graph behind it is
[Source Graph Resolved View](source-graph-view.md).

## Intent Routing

`rank_source/intent.rs::classify(query) -> QueryIntent` returns one of four intents (first match wins, in
this order):

| Intent | Matches | Effect |
| --- | --- | --- |
| `Literal { text }` | a double-quoted span of 3+ characters containing a space or a non-identifier character | source lexical scores ×0.5; the pack gains a `TextSearchHint` |
| `Relationship { direction, symbol }` | `(who\|what) (calls\|uses\|references\|invokes) X`, `callers of X`, `callees of X`, `what does X call`, `usages of X`, `impact of (changing )?X` | seed the exact node and add its direct neighbours |
| `SymbolQuestion { symbol }` | the whole query is `(what does\|what is\|where is\|explain\|show me) X (do\|does\|defined\|work\|…)?` | admit the exact node |
| `General` | everything else | today's candidacy, unchanged |

`RelationDirection` maps `calls`/`invokes`/`callers of` to `Callers`, `callees of`/`what does X call` to
`Callees`, `impact of (changing) X` to `Impact` (`impact_with`, depth 1, no site line), and
`who/what uses|references X` and `usages of X` to `References` (References edges only). Because uses of a
function are recorded as `Calls`, `who uses fn_name` finds only non-call references
([backlog](../concerns/source-graph-review-backlog.md)). `X` is an identifier or a backticked name;
`captured_name` filters pronouns (`it`, `this`, `that`, …) and only examines the first regex match.

**Routing** (`rank_source/routing.rs`, `routing/relation.rs`) runs before lexical ranking:

- A **symbol question** admits the exact node with reason `SelectionReason::SymbolQuestion`, confidence
  `Low`, `matched_term_count` 1, reaching `fn tokenize` for `what does tokenize do`. It admits NOTHING
  when more than `MAX_SYMBOL_QUESTION_MATCHES = 3` nodes carry the name (`new`, `next`, `main` flooded the
  prompt hook), leaves an already-ranked node untouched, and its reason passes the hook emit floor
  (`user_prompt_compose.rs::clears_item_floor`).
- A **relationship** query seeds on the exact node and adds its direct neighbours in the asked direction as
  `GraphNeighbor` candidates with `via` set. Seeds and neighbours from non-`Full` files are skipped
  (an `ExactSymbol` seed would claim `High`, which `withhold_partial_coverage` forbids), an already-ranked seed
  is left alone, and already-ranked neighbours get `GraphNeighbor` merged and `via` set if absent.
  Routing shares ONE `MAX_EXPANDED_TOKENS` budget with expansion: `route()` returns the tokens spent and
  `expand_from_seeds` starts from that count.
- A **literal** query sets `PackRequest.text_search` (by `build_pack_request`) and `pack()` copies it into
  `ContextPack.text_search`. `TextSearchHint::for_pattern` builds ``rg -n -F -- '<pattern>'`` with every `'`
  written `'\''`; briefs and `loom knowledge context` render `Literal text: the graph does not index bodies; run
  <command>` (written in both `orchestrator/signals/format/brief.rs` and `commands/knowledge/context_render.rs`, and
  pinned by the stage wiring check). The line is charged additively in `recompute_estimate`, and `pack()` holds
  its cost back from the selection budget via a cloned `PackRequest`. `classify` also runs on the multi-line
  composite text of stage and worker briefs, where a quoted phrase is likely.

## Explained Neighbours and the Token Cap

`RankedCandidate.via: Option<NeighborVia { seed, edge_kind, direction, provenance, site_line }>` lives in
`context/rank/candidate.rs` beside `neighbor_explanation(via)`, which renders ``called by `seed` at
path:L<n>`` or ``calls `seed` ``. `ContextItem.explanation` carries it. Explanations and caveats render
through `inline_safe`, so a backtick prints as U+02CB; containment lives at render.
`MAX_EXPANDED_TOKENS = 300` (`rank_source/expand.rs`, re-exported) stops expansion once the estimated rendered
tokens of admitted neighbours, explanation text counted, would pass it; the count caps (`MAX_EXPANDED = 12`,
`MAX_NEIGHBORS_PER_SEED = 4`) stay, and routed neighbours are not counted in `Expansion.expanded`. Expansion
never follows candidate edges (below the 0.5 floor).

## Caveats, Windows, View-Backed Graph

- `ContextItem.caveat`: a source node from a non-`Full` file gets `partial coverage`, and every source item in a
  pack whose semantic state is not `Current` gets `snapshot <state>`; each multiplies the score by 0.6. The factor
  is applied in `pack.rs` only.
- Windows (`retrieve/windows.rs`): stage briefs and `loom knowledge context` attach at most `MAX_WINDOWS = 2`
  windows of at most 12 lines through `read_window`, only for items whose reasons include `ExplicitId`,
  `ExactSymbol` or `ExactPath`, from `Full` files, in a `Current` pack; they are charged in the rendered token
  count. `StageQuery.surface: Surface { StageBrief, Cli, Hook }` selects the surface and the `Hook` surface
  attaches none. The window's `truncated` flag is discarded, so a window cut at 12 lines looks complete.
- Retrieval loads the resolved view, so expansion sees cross-file edges at or above 0.5.
- Eval: `hit_at_5` and `mrr` (`commands/knowledge/eval/metrics.rs`) are any-of over `expect`, so a relationship
  case that lists the seed beside a routed caller passes with routing off. Only a caller-only `expect` is
  routing-sensitive, and it needs a run of the tree's own binary. Every case needs `expect`, `forbid` or
  `abstain` (`load_cases_file` rejects the rest).
