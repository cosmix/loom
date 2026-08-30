# Source Graph

> What the source graph is and is not, its honesty contract, extractor trait, node/edge and cache identity, and lifecycle.

## What It Is, and What It Is Not

`loom/src/context/source_graph/` plus `context/extract/` hold a derived,
tree-sitter-backed graph of the repository's own source: file and symbol nodes
plus edges between them. It has **two** production consumers, and both are live:

| Consumer | Route in | What it reads |
| --- | --- | --- |
| `loom map` (`--outline`, `--find-all`, `--impact`) | `context::graph_store` | the resolved layer, rendered as read-only views |
| the `Source` retrieval channel | `context::rank_source` → `fuse` → `pack` | symbol nodes, scored and fused with knowledge chunks into one `ContextPack` |

The second consumer is new. Before it existed the graph was built, persisted and
given a CLI while `Channel::Source` was ranked over nothing at all — the failure
class is in `mistakes/store-without-consumer.md`, and the ranking design that
closed it is in `architecture/context-retrieval.md`.

Nobody builds this graph by hand any more either. `loom init` and `loom run`
publish a base layer through `advisory_source_graph_preflight`, and every stage's
overlay is reconciled just before its signal is written — see *Lifecycle* below.

Types live in `context/source_graph/` (not `context/schema.rs`) because the graph
is a distinct domain from the knowledge corpus; `schema.rs` re-exports the public
names, so callers may reach them through either path
(`context/source_graph/mod.rs:8-12`). A plan that names `schema.rs` as the home
of `SourceNode`/`SourceEdge` is naming the re-export — run
`rg -n "pub struct <Type>" loom/src/` before opening the file a plan points at.

## The Honesty Contract

**This graph is never claimed to be exhaustive** (`source_graph/mod.rs:14-21`).
Every `SourceEdge` carries an `EdgeProvenance` and an explicit confidence, and a
call whose target cannot be resolved is emitted as an *inferred* edge or as
`UNRESOLVED_TARGET` (`"<unresolved>"`) — never as an authoritative parser edge,
and never silently dropped or given an invented destination. Consumers that
render or traverse the graph **must surface that confidence rather than
flattening it away**.

| Provenance | Meaning | Confidence ceiling |
| --- | --- | --- |
| `Parser` | the grammar resolved both endpoints syntactically within ONE file | `1.0` — reserved for this alone |
| `Lsp` | a language server resolved it | reserved; **nothing emits this today** |
| `Inferred` | heuristically matched across files, or unresolved | `MAX_INFERRED_CONFIDENCE = 0.5` at extraction |

`context::resolve` may raise a uniquely-matched inferred edge to at most
`MAX_RESOLVED_INFERRED_CONFIDENCE = 0.9` — deliberately below `1.0`, because
cross-file uniqueness is real evidence an extractor never had, but a unique
*name* match is still not a parse: two unrelated crates can define one name, and
a graph that omits a file omits its definitions too, so "the only match I can
see" is not "the only match" (`source_graph/mod.rs:42-51`). Resolution **only
ever raises confidence with evidence**, and never promotes `Inferred` to
`Parser`.

That ceiling discipline is the reusable idea: when a component's view of the
world is structurally narrower than the claim it is asked to make, encode the gap
as a numeric ceiling in a named constant with the reasoning in its docstring —
not as a comment at the call site.

## The Extractor Trait

`context/extract/mod.rs` — bytes in, `FileExtraction` out. Each language
implements `SourceGraphExtractor` over a pinned grammar and a tree-sitter query
embedded in that language's module. **The registry (`extract::registry()`) is the
only thing the rest of loom sees; callers never name a grammar directly.**

```rust
pub trait SourceGraphExtractor {
    fn language(&self) -> DetectedLanguage;
    fn cache_identity(&self) -> ExtractorIdentity;
    fn supports(&self, path: &Path) -> bool;
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction>;
}
```

What an extractor promises: every node it emits corresponds to a real
declaration in the bytes it was handed, and every edge carries honest
provenance. What it does **not** promise: an exhaustive call graph. Extraction is
per-file, so a call to a symbol defined in another file is inferred or
unresolved, never a parser edge (`extract/mod.rs:8-16`). Cross-file resolution is
`context::resolve`'s job.

**One shared harness, not four implementations.** `context/extract/treesitter.rs`
holds the whole tree-sitter walk, parameterized by a per-language `QueryHarness`.
A language module supplies only a grammar, a query using the
`@definition.<kind>` / `@name` / `@import.path` / `@call.name` capture protocol,
and a capture-to-kind mapping. This was deliberate: the honesty constraint
(provenance, the 0.5 ceiling) is a property four separate `extract()`
implementations would each have to remember and any one could silently break.
Centralizing makes it structural. It also made the four language workers
genuinely disjoint and parallelizable.

## Node and Edge Identity

- File node id: the relative path, forward-slashed (`file_node_id`).
- Symbol node id: `<relative-path>#<kind>:<scope-joined-by-::>`, scope
  outermost-first, joined with `::` regardless of language so ids are comparable
  across extractors. Empty scope is invalid (`source_graph/mod.rs:68-89`).
- **The kind is part of the id because scope alone is not unique.** Rust's
  `struct Widget` and `impl Widget` share a name, as do a TypeScript `interface
  Foo` and a `const Foo`. Keying on scope alone let an implementation node
  silently shadow the type it implements, collapsing two distinct nodes into one
  and making their `Contains` edges indistinguishable, so a traversal could not
  tell which parent a method belonged to. The id was scope-only when first
  written and the collision was caught inside the same stage; the docstring now
  carries the reasoning so it is not "simplified" back.
- `SourceNodeKind`: `File`, `Function`, `Type`, `Interface`, `Module`,
  `Constant`, `Implementation`. `SourceEdgeKind`: `Contains`, `Imports`, `Calls`,
  `References`, `Implements`, `Extends`. Both have `as_str()` giving the stable
  lowercase name used in ids, CLI output and fixture JSON — so renaming a variant
  breaks golden fixtures and node ids at once.

## Cache Identity

`ExtractorIdentity` (`extract/mod.rs:55-69`) is what stops a cached extraction
from an older build being silently reused:

| Field | Source |
| --- | --- |
| `grammar_version` | version of the pinned tree-sitter grammar crate |
| `query_digest` | `sha256:<hex>` over the embedded query source |
| `extractor_version` | `u32`, **bumped by hand** whenever the walking logic changes shape |

**Any change to the pinned grammar, the embedded query, or the walking logic must
change this.** The grammar and query halves are automatic; `extractor_version` is
not — changing how the walk builds nodes without bumping it serves stale cached
extractions with no error anywhere. `to_parser_version()` renders a compact form
(first 12 hex digits of the digest) stored on every node as
`SourceNode::parser_version`, small enough to repeat per node.

Content identity is `body_hash(bytes)` = `sha256:<hex>`, the one definition
(`source_graph/mod.rs:91`). `refresh::source_graph::build_layer` reuses a
previous entry when the hash matches, otherwise re-extracts
(`refresh/source_graph.rs:212-245`). The empty `content_hash` can never equal a
real `body_hash`, which is what makes an unreadable-file entry safe to keep.

## Coverage: Nothing Ever Vanishes

`FileCoverage` records why a file got less than full treatment. Every situation
still yields a file node (`extract/mod.rs:18-27`):

| Situation | Result |
| --- | --- |
| no grammar for the language | file node, `FileCoverage::LexicalOnly` |
| file over `MAX_EXTRACTED_FILE_BYTES` (512 KiB) | file node, `FileCoverage::Oversized` |
| grammar reports a syntax error | file node, `FileCoverage::ParseError` |
| `source-graph` cargo feature disabled | file node, `FileCoverage::LexicalOnly` |
| unreadable file | reported entry, see `unreadable_entry` |

That is the coverage contract: a degraded file is *reported as degraded*, never
omitted. `context::coverage::CoverageReport` aggregates it. When you add an
extractor, the degraded paths are the ones to test — the happy path fails loudly,
the degraded paths fail silently.

## Building and Persisting

`refresh::source_graph::reconcile_source_graph(store, graph_store, project_root,
scope)` is the driver: walk the repository's tracked files, run the registry over
whatever changed, persist the resulting `GraphLayer` through `GraphStore`.

- `SourceGraphScope::Overlay { plan, stage }` rebuilds a stage's overlay from the
  working tree; `SourceGraphScope::Base { revision }` publishes an immutable base
  layer for a clean revision (refused, as a degraded outcome, if the tree is
  dirty). The layering rule is in `architecture/context-retrieval.md`.
- `SourceGraphOutcome { files_extracted, nodes, edges, freshness }` describes the
  layer **as walked and built by THIS call — not necessarily what ended up on
  disk**: a `Base` republish of an already-published revision reports full
  counts even though nothing was written, and a refused publish or a listing
  failure reports all zeros (`refresh/source_graph.rs:41-52`). Do not read those
  counts as "bytes written".
- A missing git repository is **data, not a crash** — see `Freshness::never_built`.
- `EXCLUDED_ROOTS` = `.work`, `.worktrees`, `target`, `node_modules`, `.git`
  (`refresh/source_graph.rs:55`).
- `context` reaches `git` exactly once for this: `git::runner::run_git_checked`
  at `refresh/source_graph.rs:30`. That is a deliberate downward edge, not a
  layering violation.

## Stack

Six dependencies, all `optional = true`, all behind ONE default-on cargo feature
`source-graph` (`loom/Cargo.toml:41-46`, `:63-77`), exact-pinned with `=`:
`tree-sitter =0.26.12`, `tree-sitter-rust =0.24.2`,
`tree-sitter-typescript =0.23.2`, `tree-sitter-python =0.25.0`,
`tree-sitter-go =0.25.0`, `streaming-iterator =0.1.9`.

- `streaming-iterator` is not incidental: tree-sitter 0.26's
  `QueryCursor::matches` returns a `StreamingIterator`, not a plain `Iterator`.
- **Why one feature and not six.** `cargo add` generates one implicit feature per
  optional dep, which would let a host disable half the grammars and leave the
  extractor registry inconsistent. Collapsing them makes
  `--no-default-features` the only supported degraded mode, and that mode falls
  back to file-level lexical nodes rather than failing to build — the point is
  that a host without a C toolchain can still build loom.

## Lifecycle: Who Builds It, and When

Nothing in the normal path asks a human to build the graph. There are three
publish points and one fallback, and every one of them is **advisory** — it
reports failure and continues, because a missing graph must degrade retrieval,
never block a run.

| When | Call site | Scope |
| --- | --- | --- |
| `loom init` | `commands/init/execute.rs:187` | `Base`, `allow_overlay_fallback = true` |
| `loom run` (daemon) | `commands/run/mod.rs:101`, in `prepare_background_run` | `Base`, `allow_overlay_fallback = false` |
| `loom run --foreground` | `commands/run/foreground.rs:39`, in `run_startup` | same |
| before a stage's signal is written | `orchestrator/core/stage_executor.rs:429-430` (fresh spawn) and `commands/stage/skip_retry.rs:205` (recovery) | `Overlay { plan, stage }` via `MergeLifecycle::reconcile_overlay` |

`advisory_source_graph_preflight(repo_root, work_dir, allow_overlay_fallback)`
(`commands/run/checks.rs:103-111`) wraps the fallible `publish_source_graph`; on
error it prints one `eprintln!` line and swallows the result. It never returns a
`Result`, so it cannot bail startup — deliberately modelled on
`advisory_codex_lane_preflight`. `publish_source_graph` (`checks.rs:115`) is
idempotent and silent on the common path: it early-returns when a base layer for
`HEAD` already exists (`checks.rs:127-129`).

**Ordering is load-bearing in `loom run`.** The preflight must run BEFORE
`plan_lifecycle::mark_plan_in_progress`: that rename dirties a tracked file, and a
dirty tree always refuses a base publish (`run/mod.rs:96-100`). A publish that
"stopped working" after an unrelated startup reorder is this.

**Recovery signals need their own call.** Signal bytes are embedded once at write
time and `start_stage` later re-uses them verbatim from disk, so a crash/hang
retry that did not reconcile first would hand the agent a stale overlay
(`skip_retry.rs:190-202`). `start_knowledge_stage` deliberately has no reconcile
call — it runs in the main repo with no worktree, and `reconcile_overlay` would
early-return anyway.

**The dirty-tree fallback.** `try_reconcile_semantic`
(`context/refresh/semantic.rs:146-176`) asks `dirty_tree_reason`
(`refresh/source_graph.rs:128-139`, `git status --porcelain=v1 --untracked-files=no`)
first. Clean tree → publish `Base { revision }`. Dirty tree, or the check itself
erroring → build `Overlay` at the address `local_overlay_key(project_root)` owns,
reported as `SemanticLayer::LocalOverlay { plan, stage, refusal }`. A base layer is
immutable and keyed to a revision, so a dirty tree can never publish one; but
publishing NOTHING left the user with no graph at all, and the overlay address is
exactly what retrieval defaults to reading. So `sync` always leaves a usable graph
and always says which one it left.
