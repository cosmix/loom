# Source Graph Known Gaps

> Language limits, decisions

What the source graph cannot see, by design or by deferral. Each gap is a place where the honesty
contract ([Source Graph](../architecture/source-graph.md#the-honesty-contract)) makes the graph say
"unresolved" or "ambiguous" instead of guessing; none is a bug to paper over with a bind.
Reviewer suggestions that are small, local fixes live in [Review Backlog](source-graph-review-backlog.md).

## Language Limits the Graph Does Not Model

- **C and C++ macros and templates.** The graph reads the syntax tree, not the preprocessed translation
  unit: a macro-generated definition has no node, a template instantiation is not a call target, and overloads
  and typed receivers are design limits (a call through a typed receiver stays ambiguous). The standard
  `#ifdef __cplusplus` / `extern "C" {` header guard becomes a whole-file `ParseError` for every `.h` (the table
  maps `.h` to `cpp`), so a common C header loses all its symbols: honest, but a recall loss to track.
- **Reflection and dynamic dispatch.** A receiver that is not a self receiver or an import is never bound:
  `obj.m()` records same-family candidates (at most 8) and nothing more. Virtual dispatch, duck typing,
  `method_missing`, `getattr` and dependency-injected receivers are invisible. A labelled corpus marks such a
  call `ambiguous` on purpose (the TSX corpus's `panel.header()` is defensible as virtual dispatch).
- **JavaScript `#private` members** are not modelled as a distinct member kind, so a `this.#m()` call is not
  bound by the receiver rule.
- **Ruby bare calls.** `bare_calls_reach_members` is true for Ruby, so a bare call may name a member of the
  enclosing type, but `method_missing` and `define_method` are unseen. Only a top-level `self` is modelled
  (`top_level_self`).
- **Inherited members** stay unbound: rule 3 looks only in `T`'s own scope (plus Rust crate `impl` blocks, C#
  `partial` classes, Ruby reopened classes, C++ out-of-line definitions). `self.source()` on a derived
  `a::Error` can bind `b::Error::source` when both crates define a type of that name.
- **Go** methods are not reachable by bare name and `import` paths are matched by directory suffix, so a slashed
  stdlib path such as `net/http` can still suffix-match a local `x/net/http` directory (accepted design behaviour,
  pinned by `tests_paths.rs`). `last_segment` splits on `.`, so `gopkg.in/yaml.v3` has local name `v3`.
- **One-segment specs match anywhere** in Python, Ruby and Go: a local `utils/os.py` captures `import os`.
  The same suffix match applies to Java `a.b.C`.
- **Kotlin and Swift** have no grammar under `github.com/tree-sitter` and are not supported; shell and
  configuration languages (proposal wave D) are out of scope. A build without a wave pack reports its
  dialects as named gaps ([Stack](../stack.md#tree-sitter-source-extraction)).

## Measurement and Storage Decisions Still Open

- **The storage engine.** Canonical JSON is the only layer and view format. The view persists no name or adjacency
  index, so every query parses the whole graph. A cold `loom map` on this repository measured 20.5 s (debug build:
  snapshot 18.0 s, resolve 2.3 s, query 19 ms, render 45 ms, peak RSS 372 MiB) over 29,835 nodes and 150,964 edges,
  and every process is cold inside a stage sandbox because nothing persists. The decision between a persisted index
  and another engine waits on warm `--timings` numbers from a host run or a `TempDir` test, never from a stage.
- **The agent-task comparison has not been run.** `doc/source-graph-evaluation.md` defines the four-mode protocol
  (`rg`-first, graph alone, graph plus `rg`, current loom guidance); it needs consenting projects and live agent
  runs. No dialect or resolver capability is promoted into agent guidance until fixtures and holdouts pass.
- **`scripts/retrieval-ab` has not been run.** It needs a pre-plan baseline binary built from another revision in a
  second worktree plus a release build, which the stage sandbox and worktree isolation rule out. It stays a
  post-merge operator check.
- **Pre-plan base layers** in a shared cache are parsed and discarded (`unknown variant 'inferred'`, a WARN each,
  twice per process) until rebuilt; `loom clean` or `loom init --clean` removes them.

## The Labelled Corpora Enter Loom's Own Graph

`EXCLUDED_ROOTS` (`.loom`, `.work`, `.worktrees`, `target`, `node_modules`, `.git`) matches only the FIRST path
segment, so the twelve corpora under `loom/tests/fixtures/source/labeled/` (and any nested `node_modules`)
enter loom's own graph: their symbols appear in `loom map --find-all`, in census counts for the loom subproject
(the go and rust corpora count as their own subprojects), and as retrieval candidates. The test-path factor
(0.4) dampens ranking but does not exclude. Fixing it means excluding by path component anywhere in the path,
which changes `excluded` for every consumer (enumeration, census, evaluator).

## Typed Receivers, Overloads and Test Selection

- **Typed receivers and overloads are never bound.** `w.run()` with `w: Widget` (C++, Rust `widget.label()`, Go
  `stock.Add()`, Python `cart.add()`, TypeScript `store.add()`) is resolved by a compiler through the receiver's static type;
  the graph treats it as a dynamic receiver and lists the single candidate without binding, so the labelled corpora count
  these as `target_recall` misses on purpose. Overload-by-arity or by argument type (Java `this.step(1)`, C# `Step(1)`, C++
  `step(1)`) stays ambiguous with both overloads as candidates, and Ruby's lexical constant lookup (`Pricing.tax`) is
  treated as dynamic.
- **Warm cost.** A debug build on this repository measured a warm `loom map` at 3.5 s against a 1.66 s release baseline:
  `ensure_base` parses the ~97 MB base layer only to check `layer_is_current`, then `GraphStore::view` parses the ~122 MB
  view. Re-measure with a release build; a follow-up could check base currency without parsing the layer (from the view
  identity or `state.json`).
- **The labelled corpora still reach the selector's graph.** Project detection skips `fixtures` directories
  (`skills/project/scan.rs::SKIP_DIRS`), so `labeled/rust` and `labeled/go` are no runner packages, and the selection set
  is the stage's diff since its merge base, so unchanged `web/` files are not picked. A JS package without `node_modules`
  gives a note, not a failure (see [verification-v2-gates](../architecture/verification-v2-gates.md)). The graph-side root
  cause, the `EXCLUDED_ROOTS` first-segment gap above, is unchanged.
