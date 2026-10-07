---
---
# Context And Source Graph

> Context retrieval, source graph

## Context Retrieval Subsystem

Read `loom/src/context/mod.rs` FIRST — its docstring carries an accurate pipeline diagram
and states plainly which channel is wired. Then the file for what you touch:

| Path                                                    | What it owns                                                                                                             |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `context/mod.rs`                                        | pipeline overview, public re-exports, the one-entry-point rule                                                           |
| `context/retrieve.rs`                                   | `retrieve_for_stage`, `StageQuery`, `context_epoch` — the ONLY way into the pipeline                                     |
| `context/schema.rs`                                     | `ContextPack`, `ContextItem`, `Channel`, `Freshness`, token constants; re-exports source-graph names                     |
| `context/ingest.rs`, `rank.rs`, `fuse.rs`, `pack.rs`    | chunk ingest, per-channel scoring, two-tier fusion (exact rungs, then reciprocal-rank fusion), budget-bounded packing    |
| `context/store.rs`                                      | derived-artifact store under `.loom/cache/context-v1/`; **`open` follows the `.loom/work` symlink to the MAIN project root**  |
| `context/delivery.rs`, `delivery/session.rs`             | delivery records, `plan_key`/`plan_key_from`, epoch-scoped suppression; `session.rs` adds the prompt hook's per-session dedupe (`hook_recipient_id`, `delivered_to_session`, `discard_session_delivery`, A.16/A.21) |
| `context/untrusted.rs`                                  | `inline_safe` — the ONE flattener for untrusted values, now on three surfaces (two agent-facing, one operator-facing) — see patterns.md |
| `context/freshness.rs`, `fingerprint.rs`, `coverage/mod.rs` | staleness tracking, content fingerprints, `CoverageReport`                                                               |
| `context/refresh/source_graph.rs`                       | `reconcile_source_graph`, `SourceGraphScope`, `SourceGraphOutcome`                                                       |
| `context/graph_store/`                                  | base/overlay layering, `GraphLayer`, canonical serialization                                                             |
| `context/source_graph/`                                 | `SourceNode`, `SourceEdge`, `EdgeProvenance` and its seven per-class confidence constants, `ImportBinding`, `node_id` |
| `context/extract/`                                      | `SourceGraphExtractor` trait, `extractor_for`/`Lookup`, `dialect.rs` (`DIALECTS`), `registry()`, `ExtractorIdentity`, `context/extract/treesitter/` shared harness, one module per dialect |
| `context/resolve/`                                      | cross-file binding rules, `impact_with`, `SymbolIndex`, path conventions                                                 |
| `telemetry/mod.rs`                                      | `TelemetryEvent`, `emit`, `read_events` over `.loom/work/telemetry/events.jsonl`                                              |
| `orchestrator/signals/format/brief.rs`                  | renders the Knowledge Brief into a stage signal                                                                          |
| `orchestrator/core/stage_telemetry.rs`                  | the only telemetry writer, called from `stage_executor.rs:570`                                                           |
| `orchestrator/merge_lifecycle.rs`                       | merge/verify/cleanup ordering; the single door to post-merge cleanup                                                     |

## Source Graph as a Retrieval Channel, and Its Lifecycle

| Surface | File | Notes |
| --- | --- | --- |
| `context::rank_source` | `context/rank_source.rs` | ranks source-graph nodes for `Channel::Source`; re-exported at `context/mod.rs:74` |
| shared BM25 core | `context/rank/corpus.rs` | `prepare_lexical` / `prepare_lexical_cached` / `score_bm25`, re-exported `pub(crate)` from `rank.rs`, used by both rankers and by the persistent index (`context/lexical_index.rs`, A.13) |
| `context/local_overlay.rs` | whole file | the ONE definition of the working-tree overlay address: `LOCAL_PLAN_KEY`, `local_overlay_stage_name`, `local_overlay_key`, `OverlayScope` |
| `advisory_source_graph_preflight` | `commands/run/checks.rs:103-111` | never returns `Result`; called from `init/execute.rs:187`, `run/mod.rs:101`, `run/foreground.rs:39` |
| `SemanticLayer` / `SemanticOutcome` | `context/refresh/semantic.rs:50-64` | what `loom knowledge sync` reports: `base` \| `local-overlay` \| `skipped` |
| `stage_overlay_scope` | `orchestrator/signals/retrieval.rs:~110` | gives a stage brief its own overlay; plan component MUST equal `delivery::plan_key(stage)` |
| `loom stage amend` | `commands/stage/amend.rs` | operator repair of an impossible criterion; thin wrapper over the pre-existing `apply_amendment` (atomic, flock, snapshot + audit row) |
| `criterion_needs_ungrantable_resource` | `plan/schema/validation.rs:647` | plan-time warning when a criterion needs `loom map`, `loom knowledge context`, `tmux` or `docker` — resources a worktree sandbox cannot grant |
| memory spool | `fs/memory/spool.rs:33,59,191` + `orchestrator/core/spool_drain.rs:38` + `git/cleanup/batch.rs:67` | see the spool-and-drain pattern |

`loom map` (`MapArgs`, `commands/map.rs`; views in `map/views/`) takes one or more view flags:

| Flag | View |
| --- | --- |
| `--outline <PATH>` | file symbols as `L<a>-L<b>  kind  name` (the contract of `loom-hooks/_read_discipline.sh`) |
| `--find-all <SYMBOL\|ID>` | every node with that name; honours `--limit`, `--path`, `--lang` while scanning |
| `--impact <SYMBOL\|PATH\|ID>` | transitive reverse reachability; `--depth`, `--kinds`, `--evidence`, `--min-confidence` apply during traversal |
| `--callers` / `--callees <SYMBOL\|ID>` | direct `Calls` edges, one hop, with call sites (`caller path:line -> target path:line  symbol=…  <provenance> <confidence>  sites=n`) |
| `--references <SYMBOL\|ID>` | direct incoming `References` edges, same row shape |
| `--window <ID>` (`--window-lines`, default 60) | exact source of a node id or site id `path@start-end`; exit 2 unknown id, 3 changed since snapshot |
| `--census [--root <DIR>]...` | what the graph can and cannot see (conflicts with every other view) |
| `--eval-edges <DIR> [--thresholds <FILE>]` | score labelled corpora; exit 1 on a failing threshold, 2 with no corpus |

Shared modifiers: `--limit` (default 50, `0` unlimited), `--path` (component-aware prefix), `--lang <DIALECT>`,
`--evidence <LIST>` (provenance classes), `--json` (top-level `"schema": "loom-map/2"`, with `snapshot`, `views`
and `coverage`), `--timings` (phases `snapshot`, `load`, `resolve`, `query`, `render`, `total` and `peak_rss_kb`
on stderr and in JSON). The CLI's `--kinds` default is `calls,references,implements,extends`; `contains` and
`imports` only when listed. An argument containing `#` is an exact id and never substring-falls-back; otherwise
an exact name is tried, then a case-insensitive substring fallback that every view labels (`(substring matches)`,
JSON `"match": "id"|"exact"|"substring"`). Candidate edges print as `candidate (N total)`. The agent doctrine
templates at the repository root and `loom-hooks/codex-forward.sh` name `--outline`, `--find-all` and `--impact`
only.

| Module | What it owns |
| --- | --- |
| `context/view/` | `GraphStore::view`, `build_cold`, `relink`, `ViewIdentity`, `RESOLVER_VERSION` ([view](../architecture/source-graph-view.md)) |
| `context/resolve/` | `resolve_graph*`, `resolve_edges`, `impact_with`, path conventions ([resolution](../architecture/source-graph-resolution.md)) |
| `context/extract/dialect.rs` | `DIALECTS`, `GrammarPack`, `dialect_for_path`, `dialect_by_id` |
| `context/census/` | the `--census` classifier and report |
| `context/window.rs` | `read_window`, `SourceWindow`, `WindowError` |
| `context/eval_edges/` | `evaluate_dir`, `Thresholds`, label loading ([evaluation](../architecture/source-graph-evaluation.md)) |
| `context/freshness.rs` | `GraphState`, `Freshness::state` |
| `commands/hook/reconcile_graph/` | `wants_rebuild`, the lease (`lock.rs`), `--cancel` (`cancel.rs`) |
| `context/rank_source/{intent,routing,expand}.rs` | intent routing and capped expansion ([routing](../architecture/context-retrieval-routing.md)) |
