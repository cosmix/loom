# Plan: Automatic Knowledge Base and Source Graph

## Overview

Loom's source graph is fully built and completely inert. It has three producers, one
consumer (`loom map`'s view flags), and no path into the thing it was built to make
cheaper: the per-stage Knowledge Brief. Meanwhile the only command that publishes a
graph refuses whenever the working tree is dirty and reports that refusal to a JSON
file instead of the terminal — so a project can run loom for months and never have a
graph, with nothing on screen ever saying so.

This plan does three things, in dependency order: makes the graph feed retrieval
(where the token savings actually are), makes creation and refresh automatic so no
user ever asks for a graph per file, and deletes the CLI sprawl that grew around
knowledge management down to one manual verb.

## Goals

- **Token savings, realized.** A stage's Knowledge Brief carries symbol-level source
  facts (`file:line`, kind, signature) alongside curated prose, so an agent reads the
  40 lines that matter instead of three files to find them. Note this needs FOUR
  things, not three: the ranker must return source nodes, the packer must build a
  source item, the item must carry a line span — and
  `orchestrator/signals/format/brief.rs` must actually RENDER that span. Today
  `render_pointer` (`brief.rs:98-106`) prints only `<path>#<anchor>` and the brief
  never renders `item.summary` at all, so the first three alone would ship a brief
  with no line numbers while every acceptance gate passed. `source-channel` owns
  `brief.rs` for exactly this reason.
- **Zero-ceremony lifecycle.** Knowledge base and source graph are created at
  `loom init`, published at `loom run` startup, and refreshed per stage before the
  session spawns — on the RECOVERY path too, not just fresh spawns, which needs a
  second call site because a recovery signal embeds its pack when it is written
  rather than when it is read (`graph-lifecycle` G3). No user action, ever, and
  never a per-file argument.
- **One manual verb.** `loom knowledge sync` does everything and reports both layers
  honestly, including refusals — and what it writes on a dirty tree is addressed so
  that retrieval can actually read it back. A write nobody can read is not a
  fallback; that is why `overlay-contract` exists.
- **Delete the sprawl.** Ten `loom knowledge` subcommands plus `loom map`'s analysis
  mode removed, with every consumer — signal doctrine, pinned tests, both skills,
  README, `CLAUDE.md.template`, this repository's own root `CLAUDE.md`, and five
  tracked sibling plans (one of which needs its stage YAML amended, not merely its
  prose) — rewritten in the same change.

### Non-goals

- No change to the extractor registry, the four supported grammars, or the honesty
  contract (`EdgeProvenance`, the 0.5/0.9/1.0 confidence ceilings). The graph's
  content is correct; only its reachability and lifecycle are wrong.
- No new retrieval algorithm. The source channel reuses the existing exact-match
  ladder and RRF fusion rather than introducing a second ranking philosophy. This
  does, however, require ONE refactor that is not optional: `score_bm25`
  (`rank.rs:110-118`) takes five whole-corpus statistics produced by
  `Corpus::prepare`, which is hard-typed to `&[KnowledgeChunk]` (`rank.rs:161-171`).
  Reusing the scoring means extracting a document-agnostic corpus builder that both
  channels feed — not merely widening a visibility modifier. `rank.rs` is in
  `source-channel`'s `files:` for that reason.
- **"The same ladder" means THREE of its five rungs, and one exact matching
  primitive — say so rather than implying parity.** `score_exact_match_ladder`
  (`rank.rs:57-104`) has five rungs: `ExplicitId`, `ExactPath`, `ExactSymbol`,
  `LinkedFrom`, `StageDependency`. Only the first three have a source-graph
  analogue. `LinkedFrom` scores knowledge-file `[[links]]` between chunks; the
  graph's edges are call/reference edges with different semantics and are out of
  scope here. `StageDependency` matches `query.stage_dependency_ids`, which
  `commands/knowledge/context.rs:81` populates from `dependency_chunk_ids` — CHUNK
  ids, never node ids, so it can never fire on a source node. `rank_source`
  therefore emits exactly `ExplicitId` / `ExactPath` / `ExactSymbol` / `Lexical`,
  and this plan does not claim otherwise anywhere.
- **The exact-match primitive is `contains_whole_term`, not tokenizer equality.**
  An earlier draft of S1 said "any query token equals … (case-sensitive)". That is
  a DIFFERENT algorithm from the one it claims to reuse, and it cannot work:
  `tokenize` (`lexical.rs:24-63`) lowercases every term and splits on every
  non-`[A-Za-z0-9_]` character, so `Foo::Bar` becomes `foo`,`bar` and
  `src/context/pack.rs` becomes `src`,`context`,`pack`,`rs` — a case-sensitive
  comparison against a lowercased token stream can never match a `CamelCase`
  symbol, and a split path can never equal a whole path. The knowledge ladder does
  not do this: it calls `contains_whole_term(&query.text, candidate)`
  (`rank.rs:71,80`) — searching the RAW query text for the candidate's own
  path/symbol string, case-INSENSITIVELY, delimited by identifier boundaries
  (`lexical.rs:138-172`). `rank_source` uses the same call with the node's own
  strings. Two token streams therefore exist and must not be confused: the raw
  query text feeds the exact rungs, and `tokenize`'s output feeds BM25 only.
- `loom memory *` is out of scope and untouched.

## Grounded findings this plan rests on

Every claim below was read at the cited line before this plan was written.

**The graph is unreachable from retrieval.** `rank_channels`
(`loom/src/context/retrieve.rs:130-142`) matches `Channel::Source` to
`rank(rank_query, &[], Channel::Source)` — a literal empty slice — and `rank`
(`rank.rs:206-208`) returns immediately on an empty input. `ItemKind::SourceNode`
(`schema.rs:79`) and `Channel::Source` (`schema.rs:52`) exist as unused variants; the
type contract already has the slots.

**`pack` cannot render a source item.** `build_item` (`pack.rs:70-90`) hardcodes
`kind: ItemKind::KnowledgeChunk` regardless of `candidate.channel`, and `pack`'s
lookup map (`pack.rs:126-129`) is built only from `&[KnowledgeChunk]`. Test
`rule_29_included_candidates_become_fully_mapped_context_items`
(`context/tests/pack.rs:92-120`) pins that behavior today: it packs a
`Channel::Source`-tagged candidate and still asserts `ItemKind::KnowledgeChunk`. That
test changes in this plan.

**There are three producers of graph data, not two.** `reconcile_source_graph`
(`context/refresh/source_graph.rs:58`) is called from `commands/map.rs:147` (view
flags only), `orchestrator/merge_lifecycle.rs:186` (pre-merge overlay, post-merge
base), and `context/refresh/source_graph.rs:393` via
`reconcile_semantic_best_effort` ← `refresh()` ← `loom knowledge sync`.

**`sync` reports only half of what it does.** `refresh()`
(`context/refresh.rs:140-167`) calls `reconcile_semantic_best_effort` on BOTH
branches — the catalog-current early return at `:151` and the post-rebuild path at
`:163`. `commands/knowledge/sync.rs:13-23` prints `rebuilt`, `revision`, `files`,
`chunks`, `issues` — all structural. The semantic layer is never mentioned in either
human or `--json` output, success or failure.

**Base publishing refuses on a dirty tracked tree.** `dirty_tree_reason`
(`refresh/source_graph.rs:128-139`) runs `git status --porcelain=v1
--untracked-files=no` and returns a refusal for any modification. This is correct and
load-bearing: a base layer is immutable and keyed to a git revision
(`graph_store/mod.rs:196-200`), so stamping HEAD's SHA on a layer walked from a
non-matching working tree would describe a revision that never existed. Verified live
on this repo: two modified knowledge files produced
`"working tree is dirty; base layer not published for 8c842e76…"` in
`.loom/cache/context-v1/state.json`, with `graph/base/` still absent entirely.

**`loom run` guarantees a clean tree before the plan rename.**
`prepare_repo_for_run` (`commands/run/checks.rs:12`) calls
`check_for_uncommitted_changes` (`checks.rs:99`), which bails on a dirty tree, and it
runs before `mark_plan_in_progress` in both paths
(`run/foreground.rs:27` then `:35`; `run/mod.rs:81` then `:97`). A base publish
inserted before the rename is therefore always on a provably clean tree.

**`INDEX.md` already regenerates on every knowledge write.**
`refresh_index_if_hierarchical` (`fs/knowledge/dir.rs:269`) is called after
append and replace-section (`dir.rs:211`, `dir.rs:244`). The explicit
`loom knowledge index` command is redundant in a hierarchical layout — but it is the
only path that upgrades a flat directory to hierarchical (`dir.rs:100`,
`index::write_index`), so that upgrade must move into `sync` rather than vanish.

**Deletion blast radius is in the signal doctrine, not just the CLI.** The stage
prefix generators embed literal command text that pinned tests assert on:
`loom knowledge show` at `orchestrator/signals/cache.rs:784`; `loom knowledge index`
at `cache.rs:590` and `format/sections.rs:239`. Tests at `cache.rs:878-1105` and
`tests_cache.rs:220-267` assert those exact strings, and `tests_doctrine.rs:383` pins
`KNOWLEDGE_CONSUMPTION_CONTRACT` byte-for-byte against `CLAUDE.md.template`'s
`## KNOWLEDGE-FIRST` section. `loom knowledge check` is a literal acceptance
criterion in `IN_PROGRESS-PLAN-concerns-sweep.md:546` and
`PLAN-loom-config-command.md:169,1176`, and both `skills/loom-plan-writer/SKILL.md`
and `skills/loom-usage/SKILL.md` prescribe it.

**…and it is wider than those three code sites.** An earlier draft of this plan
stated the three doctrine strings were the whole of it inside `loom/src`. They are
not. `context/retrieve.rs:66` tells the user to run `loom knowledge init`;
`fs/knowledge/index.rs:18` writes ``<!-- generated by `loom knowledge index` … -->``
into every generated `INDEX.md`; `fs/knowledge/dir.rs:273` prints an
`eprintln!` naming the same command; and `fs/knowledge/{gc.rs,types.rs,summary.rs}`
plus `context/ingest.rs:19` carry more. Deleting `commands/knowledge/{gc,audit}.rs`
additionally strands `fs/knowledge/gc.rs` (~270 lines) with test-only callers,
invisible to clippy because its items are `pub` re-exported at
`fs/knowledge/mod.rs:27` — the `mistakes/store-without-consumer.md` shape, inside
the change that cites it. `commands/knowledge/spawn.rs` strands the same way.
Two whole test modules (`commands/knowledge/tests.rs`, `tests_legacy.rs`) drive
deleted verbs, and one of their tests —
`tests.rs:266 test_index_upgrades_legacy_dir` — is the ONLY proof of the
flat-to-hierarchical upgrade this plan migrates into `sync`. `cli-collapse`'s
`files:` was widened and its C1 brief rewritten accordingly.

**Five tracked plans reference the deleted verbs, not two, and one of them BREAKS.**
Beyond `IN_PROGRESS-PLAN-concerns-sweep.md` (`:99`, `:531`, `:546`),
`PLAN-multimodel-phase2.md:60,380` and `PLAN-multimodel-phase3.md:40,235` carry
`loom knowledge check`; `PLAN-multimodel-phase1.md` carries it at `:46,:234` AND
owns and edits `commands/knowledge/bootstrap.rs` (`:132,:134,:499,:521`); and
`PLAN-secure-distilled-loom-v2.md:519-532`'s code-surface map names the same file.
`PLAN-loom-config-command.md` is the one that does not merely mention the verbs:
its Subagent C exists to add per-user model defaults for `loom knowledge bootstrap`
and `loom knowledge gc`, and it carries two `wiring:` entries (`:1026-1031`) whose
`source:` files this plan DELETES. A wiring check against a nonexistent file can
never pass, so an annotation would leave a permanently unexecutable plan on disk.
`cli-collapse` therefore AMENDS that plan's YAML as well as annotating it; the full
per-plan amendment set is the "Sibling-plan amendments" table above.

**Completions degrade silently, not loudly.** Subcommand *names* come from clap
introspection (`completions/dynamic/commands.rs:28`), so removals propagate free. But
`completions/dynamic/mod.rs:293` hardcodes `("knowledge", "show" | "update")` for
file-name completion, and `tests/tests_commands.rs:50-59` asserts the exact
subcommand-name list — that test fails loudly and is the change's own tripwire.

**`loom knowledge sync` has no test coverage at all.** Per the inventory sweep,
`sync`, `knowledge list`, `knowledge status`, and `replace-section` have zero tests;
a rename or removal of any of them would be caught by nothing.

## The dirty-tree decision (settled)

Base publishing stays clean-only — immutability keyed to a revision is correct and
this plan does not weaken it. Instead, **`sync` always WRITES a usable graph** by
falling back to the working-tree overlay path that `loom map` already uses
(`SourceGraphScope::Overlay { plan: "_local", stage: "map-<dir>" }`,
`commands/map.rs:133-157`) whenever the tree is dirty, and says which of the two it
did. Automatic publishing then happens at the moments loom already guarantees
cleanliness: `loom run` startup (before the plan rename) and post-merge (existing).

**A write nobody can read is not a fallback — the reader half is part of this
decision, and an earlier draft omitted it entirely.** `GraphStore::resolved`
(`graph_store/mod.rs:251-271`) applies an overlay ONLY when the caller supplies a
`(plan, stage)` key; with `None` it loads `load_base(revision)` and
`unwrap_or_default()`s a miss into an EMPTY graph. On a first dirty sync there is
no base for HEAD at all, so:

```text
dirty tree → loom knowledge sync            → writes _local/map-<dir> overlay ✓
           → loom knowledge context --scope source → resolved(HEAD, None)
                                                   → load_base(HEAD) == None
                                                   → empty graph, zero source items ✗
```

Both halves are therefore one contract, landed by the `overlay-contract` stage
before either consumer stage starts:

```rust
// loom/src/context/local_overlay.rs
pub const LOCAL_PLAN_KEY: &str = "_local";
pub fn local_overlay_stage_name(project_root: &Path) -> String;
pub fn local_overlay_key(project_root: &Path) -> (String, String);

/// Which overlay a retrieval reads on top of the base layer.
pub enum OverlayScope {
    /// The working-tree overlay `loom map` and a dirty `loom knowledge sync`
    /// maintain for THIS checkout. The default: a CLI or hook query means
    /// "the tree in front of me", not "the last clean revision".
    Local,
    /// A stage's own overlay, written by `MergeLifecycle::reconcile_overlay`.
    Stage { plan: String, stage: String },
}
```

`OverlayScope` is a typed scope, not a hidden fallback: there is no code path that
silently retries a different key, and no query reads base-only by accident.
`StageQuery.overlay` is `OverlayScope` (default `Local`); the stage-signal path
sets `Stage`. The verified sequence — dirty repo, no base, sync, then a source
query that returns a real item — is `integration-verify`'s
`dirty-tree fallback is readable` wiring test, not prose.

## Verification facts every stage needs (measured, not assumed)

These decide whether a gate is honest, and none of them is derivable from reading
the plan. Verified on this repo before this plan was finalized.

- **Acceptance criteria run through `sh -c`** (`verify/criteria/confine.rs:28`,
  `:164-167`), so `&&`, `|`, `!`, `2>&1` and `>` all work. But `sh` does **not**
  set `pipefail`, so a piped criterion returns only the LAST command's status.
  `cargo test … | rg "N passed"` therefore reports green on a FAILING run, because
  a failure still prints `test result: FAILED. 3 passed; 1 failed`. Every filtered
  test criterion in this plan is paired: one `rg "[1-9][0-9]* passed"` (rejects an
  empty filter, which exits 0) and one `rg "test result: ok"` (rejects failures).
  Neither alone is honest.
- **Plain `cargo test` is NOT this repository's test gate — it truncates.** It
  stops at the first failing TARGET and silently skips the rest, so a run can end
  on a plausible `test result: ok` while `tests/maintainability` and seven other
  targets never executed. This repo has been burned by it twice in one day and
  wrote it down: `doc/loom/knowledge/mistakes/testing-and-lint.md:66-76`, whose
  detection rule is mechanical — `error: test failed, to rerun pass --test <name>`
  means the run is INCOMPLETE regardless of what precedes it. The pre-push hook was
  fixed to `cargo test --all-targets --no-fail-fast` on 2026-08-14 and CI runs
  `cargo test --all-targets` plus a separate `cargo test --test maintainability`
  (`.github/workflows/ci.yml:59`, `:144`). Every stage in this plan uses
  `cargo test --all-targets --no-fail-fast`; an earlier draft used plain
  `cargo test` everywhere, which would have let the ledger failures below through.
- **`cargo test --all-targets` is also what compiles `loom/tests/**`.** That is not
  a nicety here: `loom/tests/integration/context_catalog.rs:184,188,204` calls
  `pack` with three arguments, and `source-channel` makes it four. The file is in
  that stage's `files:` for exactly this reason.
- **The maintainability ledger is an EXACT-match debt ledger and this plan moves
  five of its entries.** `loom/CONTRIBUTING.md:36-46`: it "fails on shrinkage just
  as loudly as on growth", and `cargo test --test maintainability` runs as part of
  a plain `cargo test` too. Measured entries this plan disturbs
  (`loom/maintainability-baseline.txt`):
  - `file src/orchestrator/core/stage_executor.rs 933` and
    `function … start_stage 411` — G3 adds a call inside `start_stage`.
  - `file src/context/refresh/tests_source_graph.rs 476` — G4 adds tests.
  - `function src/commands/run/foreground.rs execute_foreground 75` and
    `function src/commands/init/execute.rs execute 122` — G2 inserts a call in each.
  - `file src/orchestrator/signals/cache.rs 1121`,
    `file src/orchestrator/signals/format/sections.rs 1176`,
    `file src/orchestrator/signals/tests_cache.rs 687` and their function entries —
    C3 shrinks all three.
  - `function src/commands/knowledge/mod.rs list 70`,
    `function src/commands/knowledge/{audit,bootstrap,check,gc}.rs …` (6 entries),
    `function src/fs/knowledge/gc.rs analyze_gc_metrics_with_promoted 174`,
    `function src/map/analyzer.rs analyze_codebase 55`,
    `function src/map/detectors.rs analyze_dir_recursive 58` — C1/C2 delete the
    code, so the ENTRIES must be deleted too.
  `loom/maintainability-baseline.txt` is therefore in `graph-lifecycle`,
  `cli-collapse` and `integration-verify`'s `files:`. It is deliberately NOT in
  `source-channel`'s: none of that stage's files is ledgered, it runs CONCURRENTLY
  with `graph-lifecycle`, and two concurrent worktrees editing one ledger is the
  lost-work shape this plan avoids everywhere else.
- **`loom/src/context/refresh/source_graph.rs` is 398 lines against a 400-line
  limit.** G1 cannot add the overlay fallback there without creating a NEW ledger
  violation. It extracts the semantic-reconcile helpers into a sibling module
  instead, which is what `CONTRIBUTING.md:43` tells you to prefer.
- **`allow_write` cannot grant the main-repo cache, and a denial is silent.**
  `ContextStore::open` (`store.rs:43-57`) resolves the cache under the canonical
  MAIN project root, which from a worktree is `../../.loom/cache/context-v1`;
  overlays go to `.work/context` (`graph_store/mod.rs:145-163`), and `.work` is a
  symlink out of the worktree. Both emitters strip every `../` entry
  (`sandbox/settings/policy.rs:130`, `sandbox/settings.rs:225`), so no plan-level
  `allow_write` line can express those paths — this plan grants what it can
  (`.loom/**`, `.work/context/**`) and cannot grant more. That matters because a
  denial exits 0: `resolve_catalog` downgrades a refresh failure to a warning
  (`retrieve.rs:76-83`) and `reconcile_source_graph` degrades to a zero-count
  `Freshness` (`refresh/source_graph.rs:349-360`). Rule 13 applies literally here,
  so `graph-lifecycle` and `integration-verify` each assert the ARTIFACT appeared
  on disk rather than trusting an exit code.
- **The binary really is at `loom/target/debug/loom`.** There is no workspace
  `Cargo.toml` and no `.cargo/config.toml` redirecting `target/`, so
  `cd loom && ./target/debug/loom …` resolves. The lockfile is `loom/Cargo.lock`.
  There are no git dependencies, so the plan's `allowed_domains` suffice.
- **`loom map --outline src/main.rs | rg -q "function"` genuinely passes** — the
  outline renderer prints `SourceNodeKind::as_str()` lowercase, e.g.
  `L15-L37  function  main`. Confirmed by running it.
- **A `wiring:` pattern is a regex over raw file text, comments included.** Several
  of this plan's original patterns (`overlay:`, the bare word `semantic`,
  `reconcile_overlay` at a site where the plan also instructs the agent to write a
  comment saying `reconcile_overlay`) would have matched a comment. They were
  re-anchored on the code that can only exist if the behaviour does.
- **`deny_write: doc/loom/knowledge/**` is correct and must NOT be "fixed" — it
  does not block `knowledge-distill`.** `default_deny_write()` is exactly
  `["../../**", "doc/loom/knowledge/**"]` (`models/stage/types.rs:422-434`), which
  this plan restates because a plan-level block replaces the defaults, and a
  stage-level `filesystem` block REPLACES rather than merges (`sandbox/config.rs:89-96`).
  The entry is an Edit/Write tool-permission deny; knowledge writes go through the
  `loom knowledge` CLI as a Bash subprocess. `sandbox/settings/policy.rs:113-120`
  strips that exact path (and any `../` entry) from the OS-level `denyWrite`, and
  `models/stage/types.rs:423-432`'s own docstring says so, citing
  `test_merge_config_knowledge_stage` which pins that even a knowledge stage gets
  no `allow_write` carve-out for it. No stage in this plan needs a `filesystem`
  override; `CONTRIBUTING.md`, `.loom/**` and `.work/context/**` were added to the
  plan-level `allow_write` instead (see the previous bullet for what cannot be
  granted at all).
- **The C5 sweep corpus is wider than `doc/plans/**` and includes this plan
  itself.** Running the sweep today over `.` (excluding `doc/plans/DONE-*`,
  `doc/loom/knowledge/**`, `.work/**`) returns 58 files. Three facts an earlier
  draft's exclusion list got wrong:
  - the archive directory is literally named `doc/plans/ archive` — with a LEADING
    SPACE — so `--glob '!doc/plans/DONE-*'` does not exclude its 17 `DONE-*` and
    `REVIEW-*` plans;
  - root `CLAUDE.md:5-8` tells every agent in this repo to run
    `loom knowledge show`, and it is not `CLAUDE.md.template`;
  - `doc/PROPOSAL-loom-overhead-reduction.md`,
    `doc/REPORT-graft-inspired-loom-distillation.md`, `planning-omissions.md`, this
    plan file, and its `codex-` review all name the deleted verbs and always will.
  The sweep's exclusion set is defined precisely in C5 and its acceptance criteria
  name the four sibling plans explicitly rather than globbing a directory that
  contains this plan.
- **`loom plan verify` on this file reports 0 errors.** Its `Cargo.toml not found`
  warnings are a false positive of the verifier (it does not follow the `cd loom`
  prefix); the paths are real.

## Sibling-plan amendments this plan REQUIRES (blocking)

Deleting ten commands is a contract change against five unbuilt sibling plans. An
earlier draft treated this as a string-replacement chore in two files and recorded
the config-command consequence as a note. That is not enough for one of them:
`PLAN-loom-config-command.md` does not merely MENTION the deleted verbs, it has
`wiring:` entries pointing at files this plan deletes, which makes that plan
permanently unexecutable rather than merely stale. C5 performs each amendment
below; none is optional, and none may be deferred to "when that plan is next
revised".

| Sibling plan | What actually breaks | Required amendment (C5) |
| --- | --- | --- |
| `PLAN-loom-config-command.md` | Its whole Subagent C is "Knowledge Command Integration": two config keys (`knowledge_bootstrap_model`, `knowledge_gc_model`), a new `commands/knowledge/model_default.rs`, edits to `bootstrap.rs`/`gc.rs`, interactive prompt entries, unit tests, `files:` entries, and TWO `wiring:` entries (`:1026-1031`) whose `source:` files this plan DELETES. A wiring check against a deleted file can never pass. Sites: `:17-20`, `:35-36`, `:70-71`, `:254-258`, `:315`, `:382-383`, `:444-445`, `:592-593`, `:642-643`, `:688-689`, `:855-938`, `:1026-1031`, `:1223`. | Amend the YAML, not just the prose: delete the two `wiring:` entries, drop `src/commands/knowledge/{bootstrap,gc}.rs` from that stage's `files:`, remove the two config keys from the schema table, the struct, the `set` arms, the interactive prompt indices and the toggle list, and delete Subagent C's C1–C5 block and its acceptance strings. Leave the rest of that plan (its other config keys) intact, and put the removal note at the top as before. |
| `PLAN-multimodel-phase1.md` | Its driver-wiring stage OWNS `loom/src/commands/knowledge/bootstrap.rs` (`:134`, `:521`) and instructs an edit to it (`:132`, `:499-500`); its knowledge stage runs `loom knowledge check` (`:46`, `:234`) and `loom map --deep` (`:235`). An earlier draft of this plan named only phases 2 and 3 and missed phase 1 entirely. | Remove `bootstrap.rs` from that stage's `files:` and its two edit instructions (the `find_claude_path()` → `resolve_binary()` swap survives for `commands/review/generate.rs` and `commands/self_update/mod.rs`), and replace the two knowledge-stage command lines as for phases 2/3. |
| `IN_PROGRESS-PLAN-concerns-sweep.md` | `:99` prose, `:531` an executable task line, `:546` an acceptance string. It ALSO overlaps this plan's code territory: `stage_executor.rs` and the signal cache. | Rewrite all three sites. **AND** record the blocking ordering note: these two plans must not run concurrently — `loom run` one to completion before starting the other, because both write `orchestrator/core/stage_executor.rs` and `orchestrator/signals/cache.rs`. |
| `PLAN-multimodel-phase2.md`, `PLAN-multimodel-phase3.md` | `loom knowledge check` inside knowledge-bootstrap recipes: phase 2 `:60`, `:380` (executable stage prose); phase 3 `:40`, `:235`. | Replace with the `rg -q "## "` assertion, as C5 already specifies. |
| `PLAN-secure-distilled-loom-v2.md` | Its code-surface map names `commands/knowledge/bootstrap.rs` (stage 2) and `commands/knowledge/**` (stages 9, 12) at `:519-532`. This is a surface map, not executable acceptance, so a note is the proportionate weight — but it must not be left silently wrong. | Add one line to that table's stage-2 row and a short note under the map recording that the bootstrap/gc/check/audit surface was removed here and that the intended capabilities now live in `loom knowledge sync` plus the automatic init/run lifecycle. |

`DONE-*` plans, and everything under `doc/plans/ archive/`, are history and are
never edited.

## Execution Diagram

```mermaid
graph LR
    overlay-contract --> source-channel & graph-lifecycle
    source-channel & graph-lifecycle --> cli-collapse
    cli-collapse --> integration-verify
    integration-verify --> knowledge-distill
```

`overlay-contract` exists because `source-channel` and `graph-lifecycle` run
CONCURRENTLY and both need ONE new symbol: the local-overlay key that
`graph-lifecycle` WRITES on a dirty tree and `source-channel` READS. Two concurrent
worktrees cannot share a symbol neither of them has yet, and two private copies of
an overlay address is the data-corruption bug the "move, don't copy" rule exists to
prevent — so a small stage lands the contract first and both depend on it. It is
sonnet-tier: the design below is fully specified, and the work is a file move plus
one enum.

No `knowledge-bootstrap` stage: `loom knowledge check` passes on this repo today
(hierarchical layout, 51 files, 617 chunks), which is the skill's stated skip
condition.

## Stages

### 0. overlay-contract — one address for the working-tree overlay

**Why a separate stage:** the only shared new symbol between the two concurrent
stages (see the Execution Diagram note). Small, fully specified, sonnet-tier.

Creates `loom/src/context/local_overlay.rs` holding `LOCAL_PLAN_KEY`,
`local_overlay_stage_name`, `local_overlay_key` and the `OverlayScope` enum, moved
down out of `commands/map.rs` so `context` never depends upward on `commands`.

### 1. source-channel — make `Channel::Source` real

**Why a separate stage (Q4):** a cross-cutting redesign of the ranking and packing
pipeline, in a different subsystem from stage 2 and with a disjoint file set, so the
two run concurrently rather than serially.

Adds `loom/src/context/rank_source.rs` scoring `SourceNode`s from a `ResolvedGraph`
against the same `RankQuery` the knowledge channel uses, threads graph access into
`retrieve_for_stage`, and makes `pack` kind-aware so a source node renders as a real
`ContextItem` with `ItemKind::SourceNode`.

Source items are deliberately **cheap and I/O-free**: the summary is
`<kind> <scope> — <path>:<line_start>-<line_end>` and the excerpt is the node's own
stored `signature`, truncated. Retrieval never re-reads project files, so a pack
stays a pure function of bytes already on disk.

### 2. graph-lifecycle — honest reporting, then automatic creation

**Why a separate stage (Q4):** cross-cutting wiring across `commands/init`,
`commands/run`, and the orchestrator's spawn path, disjoint from stage 1's files.

Five changes: `sync` reports the semantic layer in both output modes, through a
typed `SemanticOutcome` rather than a `Freshness` that cannot carry counts; `sync`
falls back to the shared working-tree overlay address when Base is refused;
`loom init` and `loom run` startup publish automatically; `start_stage` refreshes a
stage's overlay before its signal is generated; and the recovery-signal producer
does the same, because that path writes its pack at signal-write time and never
reaches `start_stage`'s reconcile.

### 3. cli-collapse — one verb

**Why a separate stage (Q2 + Q1):** it edits `commands/knowledge/context.rs` (stage
1's file) and `commands/knowledge/sync.rs` (stage 2's file), and it rewrites doctrine
whose replacement text must describe the behavior stages 1 and 2 actually shipped.

Deletes ten surfaces and rewrites every consumer in the same change: signal doctrine,
pinned tests, both skills, README, `CLAUDE.md.template`, this repo's own root
`CLAUDE.md`, and five tracked sibling plans — `PLAN-loom-config-command.md` by
amending its stage YAML, the rest by rewriting their command references. See
"Sibling-plan amendments this plan REQUIRES (blocking)".

### Integration Verification

The repository's canonical gate set at zero tolerance, parallel review subagents,
and four executable functional gates — clean-clone `loom init` publishes a base;
a dirty tree with no base syncs to an overlay a source query can still read; a
brief carries a real `path:line-line` source item; bare `loom map` errors and names
its three view flags.

### Knowledge Distillation

Curates stage memories; updates README and `CLAUDE.md.template` for the collapsed
surface.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    auto_allow: true
    # `excluded_commands` is deliberately absent: validate_emittable
    # (loom/src/sandbox/settings/policy.rs:13-20) rejects any entry outright.
    # A plan-level filesystem block REPLACES the built-in defaults for any stage
    # that does not override it, so the defaults are restated here.
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
        - "~/.config/gcloud/**"
        - "~/.gnupg/**"
      deny_write:
        - "../../**"
        - "doc/loom/knowledge/**"
        - ".work/stages/**"
      allow_write:
        - "loom/src/**"
        - "loom/tests/**"
        - "loom/resources/**"
        - "loom/Cargo.toml"
        - "loom/Cargo.lock"
        - "loom/target/**"
        # Three stages edit the exact-match debt ledger (see "Verification facts").
        - "loom/maintainability-baseline.txt"
        - "skills/**"
        - "hooks/**"
        - "commands/**"
        - "CLAUDE.md.template"
        # Root CLAUDE.md names `loom knowledge show` (CLAUDE.md:5-8) and is a
        # cli-collapse target; CONTRIBUTING.md is a knowledge-distill output.
        # Neither was grantable in an earlier draft.
        - "CLAUDE.md"
        - "README.md"
        - "CONTRIBUTING.md"
        - "doc/plans/**"
        # Every graph command writes the derived cache and the overlay root.
        # These grants cover the in-tree paths; the MAIN-repo cache a worktree
        # stage reaches through `../../` CANNOT be granted here, because both
        # emitters strip `../` (policy.rs:130, settings.rs:225). That is why
        # graph-lifecycle and integration-verify assert the artifact exists on
        # disk rather than trusting an exit code - see "Verification facts".
        - ".loom/**"
        - ".work/context/**"
    network:
      allowed_domains:
        - "crates.io"
        - "index.crates.io"
        - "static.crates.io"
        - "github.com"
        - "api.github.com"
        - "raw.githubusercontent.com"
      allow_local_binding: false
      allow_unix_sockets: []

  stages:
    - id: overlay-contract
      name: "One Address for the Working-Tree Overlay"
      stage_type: standard
      model: "sonnet"
      reasoning_effort: "high"
      working_dir: "."
      dependencies: []
      description: |
        Land the ONE symbol that source-channel and graph-lifecycle both need,
        before either starts. They run concurrently, so neither can define it:
        graph-lifecycle WRITES a working-tree overlay when the tree is dirty and
        source-channel READS it, and two private copies of an overlay address is a
        data-corruption bug waiting to happen.

        This stage is small and fully specified. Do NOT expand its scope: no
        ranking, no sync output, no lifecycle hooks. Delegate the edit to a
        loom-software-engineer subagent, verify, commit.

        BACKGROUND - READ FIRST (do not re-derive):
        - commands/map.rs:19 has `const LOCAL_PLAN: &str = "_local";` with a
          docstring explaining the underscore prefix keeps it out of any real
          plan's namespace.
        - commands/map.rs:126-138 has `fn local_stage_name(project_root: &Path)
          -> String` returning "map-<project dir name>", falling back to "map"
          when file_name() is None, with a docstring explaining that a fixed name
          would make every worktree clobber one overlay (`.work` is a symlink to
          the main repo in every worktree).
        - Both are private to commands/map.rs. Its only uses are :146, :152, :157.
        - GraphStore::resolved (graph_store/mod.rs:251-271) applies an overlay ONLY
          when given Some((plan, stage)); with None it loads the base alone and
          `unwrap_or_default()`s a miss into an EMPTY graph.

        THE WORK, in one file plus two registrations:

        1. CREATE loom/src/context/local_overlay.rs containing:

               /// Plan name for the graph overlay the CLI and a dirty `sync`
               /// build for the working tree. Not a real stage: the underscore
               /// prefix keeps it out of any plan's namespace.
               pub const LOCAL_PLAN_KEY: &str = "_local";

               /// Stage name for the working-tree overlay, keyed by the tree's
               /// own directory name.
               pub fn local_overlay_stage_name(project_root: &Path) -> String

               /// The full `(plan, stage)` address of this checkout's
               /// working-tree overlay.
               pub fn local_overlay_key(project_root: &Path) -> (String, String)

               /// Which overlay a retrieval reads on top of the base layer.
               pub enum OverlayScope { Local, Stage { plan: String, stage: String } }

               impl OverlayScope {
                   /// The `(plan, stage)` pair to hand `GraphStore::resolved`.
                   pub fn resolve(&self, project_root: &Path) -> (String, String)
               }

           `local_overlay_stage_name` is MOVED, not copied: carry its docstring
           across verbatim and DELETE the original. Same for the constant.
           `OverlayScope::Local` is the DEFAULT for a query that names no stage,
           and its docstring must say why: a CLI or hook query means "the tree in
           front of me", not "the last clean revision". There is deliberately no
           `BaseOnly` variant - nothing needs it, and Rule B says do not add it.
           Direction matters: `context` must not depend upward on `commands`, so
           these move DOWN. They need only `std::path::Path`, so this adds no
           upward import: after the move,
           `rg '^use crate::[a-z_]+' loom/src/context` must still show only
           context / fs / language / models / git.

        2. Register `pub mod local_overlay;` in loom/src/context/mod.rs. The module
           is `pub`, not `pub(crate)`: `commands::map` must name it, and a
           `pub(crate)` item inside a privately-declared module is not nameable
           from a sibling subtree - the mistake this stage exists to avoid
           repeating (`mod source_graph;` at context/refresh.rs:18 is exactly that
           shape).

        3. Point loom/src/commands/map.rs at the moved items: :146 uses
           `local_overlay_stage_name`, :152 and :157 use `LOCAL_PLAN_KEY`. Prefer
           `local_overlay_key(project_root)` at :146 and destructure. Change
           NOTHING else in map.rs - its analysis mode is cli-collapse's to delete.

        4. Unit tests, inline in local_overlay.rs (`#[cfg(test)] mod tests`):
             - local_overlay_stage_name("/a/b/myrepo") == "map-myrepo"
             - the root path "/" falls back to "map"
             - OverlayScope::Local.resolve(root) == local_overlay_key(root)
             - OverlayScope::Stage{..}.resolve(root) returns the stage's own pair
               and ignores project_root

        LEDGER: none of these files is in loom/maintainability-baseline.txt, and
        this stage must not put one there. Keep local_overlay.rs well under 400
        lines and every function under 50; it will be roughly 80.

        MEMORY: record mistakes, decisions and surprises via `loom memory note` /
        `loom memory decision` IMMEDIATELY. NEVER `loom knowledge` (implementation
        stage). NEVER Claude Code auto-memory.
      acceptance:
        - "cd loom && cargo fmt --check"
        - "cd loom && cargo clippy --all-targets -- -D warnings"
        - "cd loom && cargo test --all-targets --no-fail-fast"
        - "cd loom && cargo build"
        - "cd loom && cargo test --test maintainability"
        # A filter matching nothing exits 0, and a pipe under `sh -c` reports only
        # the last command's status, so both halves are required. See the same
        # note on source-channel.
        - 'cd loom && cargo test --lib local_overlay 2>&1 | rg -q "[1-9][0-9]* passed"'
        - 'cd loom && cargo test --lib local_overlay 2>&1 | rg -q "test result: ok"'
        # MOVED, not copied: the originals must be gone from map.rs.
        - 'rg --files loom/src/commands > /dev/null && ! rg -q "const LOCAL_PLAN\b|fn local_stage_name" loom/src/commands/map.rs'
        # The views still work, which is what proves the move did not break the
        # only existing consumer.
        - 'cd loom && ./target/debug/loom map --outline src/main.rs | rg -q "function"'
      files:
        - "loom/src/context/local_overlay.rs"
        - "loom/src/context/mod.rs"
        - "loom/src/commands/map.rs"
      artifacts:
        - "loom/src/context/local_overlay.rs"
      wiring:
        - source: "loom/src/context/mod.rs"
          pattern: "pub mod local_overlay"
          description: "Local-overlay contract is a real, publicly nameable module"
        - source: "loom/src/commands/map.rs"
          # Tolerates both spellings: a full path call and a braced `use` import.
          pattern: "local_overlay::\\{?\\s*(local_overlay_key|LOCAL_PLAN_KEY|local_overlay_stage_name)"
          description: "map's views read the moved contract, not a private copy"
      dead_code_check:
        command: "cd loom && cargo clippy --all-targets -- -D warnings 2>&1"
        fail_patterns:
          - "never used"
          - "never read"
        ignore_patterns: []

    - id: source-channel
      name: "Source Graph Feeds Retrieval"
      stage_type: standard
      model: "opus"
      reasoning_effort: "xhigh"
      implementers: ["claude", "codex"]
      subagent_timeout_secs: 900
      working_dir: "."
      dependencies: ["overlay-contract"]
      description: |
        Make Channel::Source produce real retrieval results, so every stage's
        Knowledge Brief carries symbol-level source facts alongside curated prose.
        This is the token-saving change: the rest of this plan is worth nothing
        without it.

        Use parallel subagents and skills to maximize performance.

        BACKGROUND - READ FIRST (do not re-derive):
        - rank_channels (loom/src/context/retrieve.rs:130-142) currently matches
          Channel::Source to `rank(rank_query, &[], Channel::Source)` - a literal
          empty slice. rank() (rank.rs:206-208) returns immediately on empty input.
        - ItemKind::SourceNode (schema.rs:79) and Channel::Source (schema.rs:52)
          already exist as unused variants.
        - build_item (pack.rs:70-90) hardcodes ItemKind::KnowledgeChunk regardless
          of candidate.channel; pack's lookup map (pack.rs:126-129) is built only
          from &[KnowledgeChunk].
        - GraphStore::resolved already flattens base+overlay into a ResolvedGraph
          with nodes()/edges()/node(id) iterators. Read it, do not rebuild it.
          It applies an overlay ONLY when given Some((plan, stage)); with None it
          reads the base alone and turns a base miss into an EMPTY graph. That is
          why StageQuery carries an OverlayScope and why its default is Local.
        - loom/src/context/local_overlay.rs already exists (overlay-contract, this
          stage's dependency) and exports LOCAL_PLAN_KEY, local_overlay_stage_name,
          local_overlay_key and OverlayScope. Use them; do not redefine them.
        - SourceNode carries its OWN `coverage: FileCoverage` field
          (source_graph/node.rs:148). You never need to walk back to the owning
          FileEntry to learn a node's coverage.

        FOUNDATION STEP - the orchestrator does this FIRST, alone, before any
        subagent fans out, because three subagents compile against it:
        Extend StageQuery (loom/src/context/retrieve.rs:30) with one field:
            pub overlay: OverlayScope,
        using crate::context::local_overlay::OverlayScope. Default it to
        OverlayScope::Local in StageQuery::new (retrieve.rs:45), NOT to a
        base-only read: a query that names no stage means "the tree in front of
        me". Document that on the field.
        THE FOUNDATION STEP IS TWO EDITS, NOT ONE. StageQuery is also built as a
        STRUCT LITERAL at loom/src/commands/knowledge/context.rs:103-109
        (work_dir_hint / text / required_ids / stage_dependency_ids / scope).
        Adding a field breaks that file immediately, so the same foundation edit
        MUST add `overlay: OverlayScope::Local,` there - which is also the line
        that makes `loom knowledge context --scope source` see a dirty tree's
        working-tree overlay at all. Do NOT hand this to a subagent: the tree must
        compile before S1-S5 start, and context.rs is S4's file. Verify with
        `rg -n "StageQuery *\{" loom/src` - it must return exactly THREE lines:
        retrieve.rs:30 (the struct declaration itself, which the pattern also
        matches - an earlier draft forgot it and told you to expect two),
        retrieve.rs:46 and context.rs:103. Every other caller uses StageQuery::new
        and is unaffected.

        SUBAGENT FILE ASSIGNMENTS - NO FILE OVERLAP:

        | Subagent | Files Owned (write) | Files Read-Only |
        | --- | --- | --- |
        | S1 - source ranker (loom-senior-software-engineer, opus) | loom/src/context/rank_source.rs (new), loom/src/context/mod.rs | rank.rs, source_graph/node.rs, graph_store/mod.rs, schema.rs |
        | S2 - retrieval wiring (loom-software-engineer, sonnet) | loom/src/context/retrieve.rs | rank_source.rs, graph_store/mod.rs, store.rs |
        | S3 - kind-aware packing (loom-software-engineer, sonnet) | loom/src/context/pack.rs | schema.rs, source_graph/node.rs |
        | S4 - signal + brief + CLI plumbing (loom-senior-software-engineer, opus) | loom/src/orchestrator/signals/retrieval.rs, loom/src/orchestrator/signals/format/brief.rs, loom/src/commands/knowledge/context.rs, loom/src/commands/knowledge/tests_context.rs | retrieve.rs, delivery.rs, merge_lifecycle.rs |
        | S5 - tests (loom-codex-forwarder, --model gpt-5.6-terra) | loom/src/context/tests/rank_source.rs (new), loom/src/context/tests/mod.rs, loom/src/context/tests/pack.rs, loom/tests/integration/context_catalog.rs | all of the above |

        S4 IS OPUS, NOT CODEX, and that is deliberate: it now owns the brief
        renderer and the overlay-key correctness argument, both of which are
        judgement calls with a silent-failure mode, not mechanical edits. S1, S2,
        S3 and S5 keep their assigned tiers.

        S1 - SOURCE RANKER. Create loom/src/context/rank_source.rs with:
            pub fn rank_source(
                query: &RankQuery,
                graph: &ResolvedGraph,
            ) -> Vec<RankedCandidate>
        RankQuery (rank.rs:30) is:
            pub struct RankQuery {
                pub text: String,
                pub required_ids: Vec<String>,
                pub stage_dependency_ids: Vec<String>,
            }
        RankedCandidate (rank.rs:41) is:
            pub struct RankedCandidate {
                pub id: ChunkId,
                pub channel: Channel,
                pub score: f32,
                pub reasons: Vec<SelectionReason>,
                pub token_count: usize,
            }
        Iterate graph.nodes() (SourceNode). Skip SourceNodeKind::File nodes - a
        whole-file node carries no signature and would crowd out symbols.

        THE EXACT RUNGS USE contains_whole_term, NOT TOKEN EQUALITY. An earlier
        draft said "any query token equals ... (case-sensitive)". That is a
        different algorithm from the one this plan promises to reuse and it cannot
        work: tokenize (lexical.rs:24-63) lowercases every term and splits on every
        non-[A-Za-z0-9_] character, so `Foo::Bar` becomes foo,bar and
        `src/context/pack.rs` becomes src,context,pack,rs - a case-sensitive
        comparison against a lowercased, split token stream matches neither a
        CamelCase symbol nor a whole path. What rank.rs:71,80 actually does is
        search the RAW query text for the CANDIDATE's own string:
            crate::context::lexical::contains_whole_term(&query.text, candidate)
        which is case-INSENSITIVE and delimited by identifier boundaries
        (lexical.rs:138-172), so `Foo::Bar`, `src/context/pack.rs`, `snake_case`
        and file stems all match while a bare `n` does not. contains_whole_term is
        already pub(crate). Use it verbatim. Two token streams therefore exist and
        must not be mixed: raw query text feeds the exact rungs below; tokenize's
        output feeds BM25 only.
        For each remaining node compute:
          - ExactSymbol when contains_whole_term(&query.text, terminal) fires for
            the node's terminal scope segment (node.scope.last()), reusing the
            BOOST_* constants at rank.rs:18-26 rather than inventing new weights.
            Skip the rung when scope is empty.
          - ExactPath when contains_whole_term fires for the node's path string OR
            for its file stem. Test both spellings: `src/context/pack.rs` and
            `pack`.
          - Lexical via the same BM25 scoring the knowledge channel uses, over the
            node's scope segments and signature.
            READ THIS BEFORE STARTING - "just make score_bm25 pub(crate)" DOES NOT
            WORK. score_bm25 (rank.rs:110-118) takes SEVEN arguments, five of which
            are whole-corpus statistics: query_terms, document_frequencies,
            documents, lengths, average_length, corpus_size, index. They are all
            produced by `Corpus::prepare(query, chunks: &'a [KnowledgeChunk])`
            (rank.rs:161-171), which is hard-typed to KnowledgeChunk through
            `field_tokens`. A ResolvedGraph has none of it.
            THE ACTUAL WORK, and it is a refactor of rank.rs (which is in this
            stage's files: for exactly this reason): extract the corpus machinery
            into a document-agnostic form that both channels feed. Concretely,
            introduce in rank.rs
                pub(crate) struct LexicalCorpus { /* the current Corpus fields */ }
                pub(crate) fn prepare_lexical(
                    query_terms: &[String],
                    documents: Vec<Vec<(String, f32)>>,
                ) -> LexicalCorpus
            and have BOTH `Corpus::prepare` (knowledge, via field_tokens) and
            rank_source (graph, via the node's scope segments + signature) build
            their `documents` vector and then call it. Keep score_bm25's body
            unchanged; widen it to pub(crate).
            HAZARD, do not skip: rank.rs:122 does `document_frequencies[term]` -
            a BTreeMap index, which PANICS on a missing key. It is safe today only
            because Corpus::prepare inserts an entry for EVERY query term,
            including zero-frequency ones. Your source-side preparation must do the
            same, or an unmatched query term panics the whole retrieval. Add a test
            for a query term that appears in no node.
          - ExplicitId when node.id appears in query.required_ids.
        A node with no fired reason is EXCLUDED, matching rank.rs:233. Set
        channel: Channel::Source and id: node.id.clone().
        token_count: estimate as signature.len()/4 + 16, floored at 8 - a source
        item is a few lines, and over-estimating starves the pack.
        Sort descending by score, then ascending by (path, line_start) so two runs
        over identical bytes produce identical order. `score` is f32: use
        `partial_cmp(...).unwrap_or(Ordering::Equal)` exactly as rank.rs:241-244
        does, never `total_cmp` and never `unwrap()` - a NaN must not panic
        retrieval.

        CAP AND BUDGET SHARE - SETTLE THIS HERE, do not leave it to
        integration-verify. Cap at 60 source candidates, not 200. The reason is
        `fuse` (fuse.rs:21-49): it is pure reciprocal-rank fusion, so a rank-1
        source node contributes exactly what a rank-1 knowledge chunk contributes
        (fuse.rs:26), and there is NO per-channel budget anywhere in the pipeline.
        `pack` then walks the fused list in order taking whole units until the
        budget is spent. A stage brief's budget is STAGE_BRIEF_BUDGET_TOKENS
        (orchestrator/signals/retrieval.rs) and a source item costs ~20-30 tokens
        against a knowledge chunk's few hundred, so an unbounded source list does
        not "crowd out" prose by token volume - it crowds it out by SLOT, one
        alternating rank at a time. 60 keeps the source channel able to answer a
        symbol query fully while leaving the fused head dominated by prose, which
        is the intended balance: source facts are a pointer INTO the code, prose is
        the thing that cannot be re-derived from the code.
        Put that reasoning in the constant's docstring, not at the call site.

        ID-COLLISION NOTE for whoever touches fuse later: fuse keys its accumulator
        by ChunkId ACROSS channels (fuse.rs:22,27). Knowledge ids are
        `<path>#<heading>#<occurrence>` and source ids are
        `<path>#<kind>:<scope>` over disjoint path spaces, so a collision is not
        reachable today - but if one ever occurred, fuse would merge the two and
        adopt the better-ranked channel (fuse.rs:34-37), after which pack's
        channel-dispatch would consult the wrong map and silently drop the item.
        Do not "simplify" pack's dispatch to try both maps; that would hide it.
        Register `pub mod rank_source;` and re-export `rank_source` in
        loom/src/context/mod.rs beside the existing rank export.
        CONFIDENCE DISCIPLINE - AND IT IS THREE REASONS, NOT TWO. A node whose
        coverage is not FileCoverage::Full must NOT be allowed to reach
        Confidence::High. Confidence::from_reasons (schema.rs:183-192) classifies
        High from ANY of ExplicitId | ExactPath | ExactSymbol - all THREE, in one
        `matches!`. An earlier draft enforced the invariant by withholding only
        ExactSymbol and ExactPath while still emitting ExplicitId for every
        coverage level, which left the stated non-negotiable invariant violated by
        the single most likely query shape: a caller that passes --require-id for a
        node in a partial or lexical-only file.
        Encode it as ONE guard covering all three: compute the exact rungs, then,
        when `!matches!(node.coverage, FileCoverage::Full)`, drop EVERY high-tier
        reason (ExplicitId, ExactPath, ExactSymbol) and their boosts, leaving
        Lexical only - Confidence::from_reasons then yields Low. A required id that
        loses its rung must still be RANKED (it keeps whatever lexical score it
        earned) and must still be able to appear; it simply cannot claim High.
        node.coverage is a field on SourceNode itself (source_graph/node.rs:148),
        so this needs no lookup back to the owning FileEntry.
        Put the reasoning in a docstring, not a call-site comment. S5 tests it
        (see "an exact required node id plus non-Full coverage").

        S2 - RETRIEVAL WIRING. In loom/src/context/retrieve.rs:
        1. Change rank_channels' signature to take the resolved graph:
               fn rank_channels(
                   channels: &[Channel],
                   rank_query: &RankQuery,
                   catalog: &Catalog,
                   graph: Option<&ResolvedGraph>,
               ) -> Vec<Vec<RankedCandidate>>
           and replace the Channel::Source arm's `rank(rank_query, &[], ...)` with
           `graph.map(|g| rank_source(rank_query, g)).unwrap_or_default()`.
        2. In retrieve_for_stage (retrieve.rs:153), after resolve_catalog and
           before rank_channels, load the graph best-effort:
               let graph = load_resolved_graph(&query.work_dir_hint, &store, &semantic_revision, &query.overlay);
           Write load_resolved_graph as a private fn in this file that opens
           WorkDir::new(project_root), builds
           GraphStore::new(store.root(), work_dir.root()) (graph_store/mod.rs:159),
           turns the scope into an address with
               let (plan, stage) = query.overlay.resolve(project_root);
           and calls
               pub fn resolved(&self, revision: &str, stage: Option<(&str, &str)>) -> Result<ResolvedGraph>
           (graph_store/mod.rs:251) with `Some((&plan, &stage))` - ALWAYS Some.
           ANY error returns None - a missing graph is a degraded pack, never a
           failed retrieval. This mirrors resolve_catalog's existing silent-degrade
           behavior at retrieve.rs:76-83.
           WHY ALWAYS Some, and why this line is the whole dirty-tree story: with
           None, resolved() reads the base alone and turns a base miss into an
           empty graph (graph_store/mod.rs:252). On a dirty tree there IS no base
           for HEAD - graph-lifecycle's fallback wrote a `_local` overlay instead -
           so a None read would return zero source items while every acceptance
           gate in this stage still passed. OverlayScope::Local resolves to exactly
           the address graph-lifecycle writes. Missing overlay + missing base is
           still an empty graph, and that is correct: nothing has been built yet.
        3. The revision passed to resolved() is the semantic freshness revision from
           the store state, NOT the structural one. They are different hash domains
           (see the comment at refresh.rs:177-182).
        Do NOT change retrieve_for_stage's public signature.

        S3 - KIND-AWARE PACKING. In loom/src/context/pack.rs:
        1. Change pack's signature to accept the graph:
               pub fn pack(
                   request: &PackRequest,
                   ranked: &[RankedCandidate],
                   chunks: &[KnowledgeChunk],
                   graph: Option<&ResolvedGraph>,
               ) -> ContextPack
        2. Build a second lookup: BTreeMap<&str, &SourceNode> from graph.nodes(),
           keyed by node.id, alongside the existing chunk map at pack.rs:126-129.
        3. Split build_item into build_chunk_item (the current body, unchanged) and
           build_source_item. Dispatch on candidate.channel, not on which map hits
           first - a channel is authoritative about what its ids mean.
        4. build_source_item produces a ContextItem. ContextItem (schema.rs:276-307)
           has TWELVE fields and Rust will not let you omit one, so all twelve are
           specified here - an earlier draft listed six and left the rest to guesswork:
             id: ChunkId::from(node.id.as_str())   // ChunkId is a newtype over
                                                   // String (schema.rs:87-89);
                                                   // node.id is a plain String,
                                                   // so the conversion is required
             kind: ItemKind::SourceNode
             pointer: a SourcePointer carrying node.path, and
                      line_start: Some(node.span.line_start),
                      line_end: Some(node.span.line_end),
                      anchor: String::new()
                      (SourcePointer is schema.rs:121-133 - path, anchor,
                      line_start: Option<usize>, line_end: Option<usize>. Span is
                      source_graph/node.rs: start_byte, end_byte, line_start,
                      line_end - all usize, lines 1-indexed inclusive.)
             summary: format!("{} {} - {}:{}-{}", node.kind.as_str(),
                      node.scope.join("::"), node.path.display(),
                      node.span.line_start, node.span.line_end)
             source: Channel::Source
             token_count: candidate.token_count
             score: candidate.score
             reasons: candidate.reasons.clone()
             confidence: Confidence::from_reasons(&candidate.reasons)
             state: LifecycleState::Active
                    (schema.rs:209-219 - Active is #[default]. A source node has no
                    curation lifecycle: it is neither a draft nor deprecated, it is
                    whatever the code currently says. Say this in a comment so a
                    later reader does not try to derive a state from coverage.)
             content_hash: node.body_hash.clone()
                    (source_graph/node.rs: `body_hash` is already
                    `sha256:<hex>` over the node's exact source bytes - it is the
                    per-node hash, strictly better than the owning file's hash for
                    delivery-record suppression, which is what content_hash feeds,
                    schema.rs:293-299)
             excerpt: Some(bounded_excerpt(&node.signature))
                    USE THE EXISTING HELPER, not truncate_for_display. bounded_excerpt
                    (pack.rs:51-67) is what enforces ContextItem.excerpt's DOCUMENTED
                    contract (schema.rs:300-306): bounded by EXCERPT_MAX_TOKENS (400,
                    schema.rs:37) and, when cut, ending with EXCERPT_TRUNCATION_MARKER
                    on its own line. truncate_for_display (utils.rs:276) is a
                    different contract entirely and would silently make source
                    excerpts the only items in the corpus that violate the documented
                    one. A signature is short, so this is nearly always a no-op.
                    node.signature is a String (empty for file nodes, which this
                    stage excludes anyway), not an Option.
           NO FILE READS. Retrieval must stay a pure function of bytes already on
           disk; re-reading project files at pack time would make a pack depend on
           the working tree at query time.
        5. A candidate whose id is in neither map is skipped exactly as today - it
           must not panic and must not consume budget.

        S4 - SIGNAL, BRIEF RENDERING, AND CLI PLUMBING.
        1. loom/src/orchestrator/signals/retrieval.rs: in retrieve_stage_pack
           (retrieval.rs:65), set the new StageQuery.overlay field so a stage's
           brief describes its OWN worktree rather than the base revision.
           CORRECTION - THERE IS NO `plan_id` IN THAT SCOPE. The signature is
           `retrieve_stage_pack(work_dir: &Path, stage: &Stage)`. Do NOT invent a
           binding. Use the SAME derivation the delivery records already use, which
           the function calls one line below at retrieval.rs:69:
               query.overlay = OverlayScope::Stage {
                   plan: crate::context::delivery::plan_key(stage).to_string(),
                   stage: stage.id.clone(),
               };
           (OverlayScope is crate::context::local_overlay::OverlayScope, landed by
           the overlay-contract stage. A stage pack reads the STAGE's own overlay,
           never the checkout-wide Local one - that is the whole point of the two
           variants.)
           WHY THIS EXACT DERIVATION AND NO OTHER - this is a correctness trap, not
           a style note. The overlay is WRITTEN by MergeLifecycle::reconcile_overlay
           (merge_lifecycle.rs:73-84), which keys it by `self.plan_id()`
           (merge_lifecycle.rs:237-245) - read from .work/config.toml. The reader
           here keys it by delivery::plan_key(stage) (delivery.rs:42-56), which
           reads `stage.plan_id` and falls back to DEFAULT_PLAN_KEY = "default"
           (delivery.rs:33). `Stage.plan_id` is Option<String> and defaults to None
           (models/stage/types.rs:638, :1138). If those two disagree the reader
           looks in .work/context/default/<stage>/ while the writer wrote
           .work/context/<real-plan-id>/<stage>/ - and GraphStore::resolved returns
           the base layer with NO error (graph_store/mod.rs:259-268). The brief
           silently degrades and nothing reports it. This is the exact failure
           documented in architecture/context-retrieval.md:123-128.
           SO: S4 must ALSO add a test (in loom/src/context/tests/, which S5 owns -
           coordinate) that writes an overlay through the MergeLifecycle key and
           reads it back through the retrieval key, asserting the SAME directory is
           used. If the two derivations cannot be made to agree, STOP and report:
           the correct fix is then to make MergeLifecycle call
           delivery::plan_key_from(config.plan_id()) so there is ONE derivation, and
           that is a change to merge_lifecycle.rs which belongs to graph-lifecycle.

        1b. loom/src/orchestrator/signals/format/brief.rs - WITHOUT THIS THE WHOLE
           PLAN SHIPS INERT. The brief is the surface this plan exists to improve,
           and today it THROWS AWAY exactly the facts this stage adds:
             - render_pointer (brief.rs:98-106) formats only `<path>` plus
               `#<anchor>`. It never reads pointer.line_start / pointer.line_end.
             - the brief NEVER renders item.summary at all - format_item_entry
               (brief.rs:79-95) emits id, pointer, reasons and state, and
               brief.rs:39-41 appends the excerpt. Nothing else.
           So a source item would reach the brief as
           "- `<node-id>` — `loom/src/context/rank.rs`" with no line span, while
           `loom knowledge context` WOULD show the span (it renders summary at
           commands/knowledge/context.rs:170-184). Every acceptance gate in this
           plan would pass and the stated token saving would not land.
           CHANGE: in render_pointer, when pointer.line_start is Some, render
           `<path>:<line_start>-<line_end>` (and still append `#<anchor>` when the
           anchor is non-empty, which is the knowledge-chunk case; the two are
           mutually exclusive in practice because pack.rs:76-78 leaves line_start
           None for chunks). Keep the value going through inline_safe exactly as
           today (brief.rs:84) - a path is untrusted.
           Update the pinned brief tests in the same edit: the item fixture at
           brief.rs:139-158 builds a SourcePointer literal, so it gains the two new
           fields, and any test asserting a rendered pointer string must gain a
           sibling case with a line span present. Do NOT weaken an existing
           assertion to make it pass; add the span case alongside it.
        2. loom/src/commands/knowledge/context.rs: DELETE inert_source_notice
           (context.rs:51-57) and its call site (context.rs:115-117). The source
           channel is no longer inert and the notice would now be a lie.
        3. loom/src/commands/knowledge/tests_context.rs: delete the
           inert_source_notice test (tests_context.rs:178-192). Do not weaken it -
           delete it, and add nothing in its place here; S5 owns the new coverage.

        S5 - TESTS. Create loom/src/context/tests/rank_source.rs, register it in
        tests/mod.rs, and follow the hand-built-fixture style of
        context/coverage.rs's inline tests (build SourceNode/FileEntry/ResolvedGraph
        literals; no filesystem). Cover, one test each:
          - an exact symbol match outranks a lexical-only match
          - a node in a file whose FileCoverage is not Full never reaches
            Confidence::High
          - AND the same for an EXPLICIT ID, named exactly
            `test_explicit_id_on_non_full_coverage_is_not_high`: a node named in
            query.required_ids whose coverage is not Full must NOT serialize as
            Confidence::High. This is the case an earlier draft's invariant missed
            entirely (Confidence::from_reasons treats ExplicitId as High alongside
            the two suppressed rungs, schema.rs:183-192), so it is the one test
            here that would have caught a shipped violation.
          - a CamelCase symbol and a full path both match through
            contains_whole_term: assert that a query text of "Foo::Bar" fires
            ExactSymbol on a node whose terminal scope is `Bar`, and that
            "src/context/pack.rs" fires ExactPath on a node in that file, and that
            the bare stem "pack" does too. These are the cases token equality would
            silently have failed.
          - File-kind nodes are excluded
          - a node with no fired reason is excluded
          - ordering is deterministic across two runs over identical input
          - a query term matching NO node does not panic (see the BTreeMap-index
            hazard in S1)
        In tests/pack.rs, UPDATE
        rule_29_included_candidates_become_fully_mapped_context_items
        (tests/pack.rs:92-120): it currently pins that a Channel::Source candidate
        still yields ItemKind::KnowledgeChunk (it asserts
        `item.kind == ItemKind::KnowledgeChunk` at :108 and
        `line_start == None` at :111). That is the behavior this stage removes.
        Rewrite it to assert a Source-channel candidate backed by a SourceNode
        yields ItemKind::SourceNode with a pointer carrying real line numbers, and
        keep a sibling case pinning that a Knowledge candidate still yields
        ItemKind::KnowledgeChunk.
        Also assert the WHOLE serialized source item, not just kind and lines: one
        test named exactly `test_source_item_carries_every_field`, that packs a
        Source candidate and checks all twelve ContextItem fields
        against the backing SourceNode - id, kind, pointer.path/line_start/line_end,
        empty anchor, summary text, source == Channel::Source, token_count and score
        carried from the candidate, reasons cloned, confidence derived,
        state == LifecycleState::Active, content_hash == node.body_hash, and excerpt
        == the bounded signature. A field silently left at Default is exactly the
        failure this catches, and `kind == "source-node"` alone would not.

        THE `pack(` CALL INVENTORY IS NOT CONFINED TO src/. An earlier draft said
        "pack's only PRODUCTION caller is retrieve.rs:177, so the signature change
        is fully contained inside this stage" - true for src/, and wrong for the
        crate. `loom/tests/integration/context_catalog.rs` calls the three-argument
        `pack` at :184, :188 and :204. That file is a `tests/` target, so
        `cargo clippy --all-targets` and `cargo test --all-targets` both fail on it
        while `cargo build` stays green. It is in this stage's `files:` and in your
        ownership above: add the fourth argument (`None` - those cases are
        knowledge-only) and change nothing else there.
        Note every other `pack(` call in tests/pack.rs (:47, 57, 69, 83, 106, 126,
        140, 155, 164, 170, 191, 211, 298 and any added since) gains the new graph
        argument too - pass None wherever the case is knowledge-only. Confirm the
        full inventory yourself before you start with
        `rg -n "\bpack\(" loom/src loom/tests`, and ignore the same-named local
        test-fixture helper in orchestrator/signals/format/brief.rs, which is a
        different function.

        LEDGER: none of this stage's files appears in
        loom/maintainability-baseline.txt, and this stage does NOT own that file -
        graph-lifecycle runs concurrently and does. So do not create a NEW
        violation: keep rank_source.rs under 400 lines and every function under 50,
        and if the LexicalCorpus extraction threatens to push rank.rs (250 lines
        today) or pack.rs (159) over 400, extract into an unledgered sibling module
        as loom/CONTRIBUTING.md:43 advises rather than recording a new entry. If a
        new violation is genuinely unavoidable, STOP and report rather than editing
        the ledger from this worktree.

        MEMORY: record mistakes, decisions and surprises via `loom memory note` /
        `loom memory decision` IMMEDIATELY. NEVER `loom knowledge` (implementation
        stage). NEVER Claude Code auto-memory.
      acceptance:
        - "cd loom && cargo fmt --check"
        - "cd loom && cargo clippy --all-targets -- -D warnings"
        # --all-targets --no-fail-fast, NOT plain `cargo test`: plain cargo test
        # stops at the first failing TARGET and silently skips the rest, and
        # --all-targets is also what compiles loom/tests/integration/, where the
        # three-argument `pack` callers live. See "Verification facts" and
        # doc/loom/knowledge/mistakes/testing-and-lint.md:66-76.
        - "cd loom && cargo test --all-targets --no-fail-fast"
        - "cd loom && cargo build"
        # The exact-match debt ledger. Not owned by this stage, so this criterion
        # is the tripwire that says a new violation was introduced.
        - "cd loom && cargo test --test maintainability"
        # `cargo test <filter>` exits 0 when the filter matches NOTHING, so a bare
        # filtered run would pass green if the test module were never registered.
        # Requiring a non-zero pass count is what makes this criterion discriminate.
        #
        # Acceptance criteria run through `sh -c`
        # (loom/src/verify/criteria/confine.rs:28,164-167), so `&&`, `|`, `!` and
        # `2>&1` all work - but `sh` does NOT set pipefail, so a piped criterion
        # returns only the LAST command's status. `cargo test ... | rg "N passed"`
        # would therefore report green on a FAILING run, because a failure prints
        # "test result: FAILED. 3 passed; 1 failed" and "[1-9][0-9]* passed"
        # matches "3 passed". The bare `cargo test` above is what catches failures;
        # this criterion exists ONLY to prove the module is registered and
        # non-empty. Both are required - neither alone is honest.
        - 'cd loom && cargo test --lib rank_source 2>&1 | rg -q "[1-9][0-9]* passed"'
        - 'cd loom && cargo test --lib rank_source 2>&1 | rg -q "test result: ok"'
        - 'rg -q "rank_source" loom/src/context/retrieve.rs'
        # The brief must actually RENDER the span, not merely carry it. A grep for
        # line_start on brief.rs proves the renderer reads it at all - without this
        # the whole plan can ship with every other gate green (see S4 step 1b).
        # Anchored on `pointer.line_start`, NOT the bare word `line_start`: the
        # latter already appears today in brief.rs's own test fixture (:146-147),
        # so it would pass without the renderer ever reading the span.
        - 'rg -q "pointer\.line_start" loom/src/orchestrator/signals/format/brief.rs'
        # The exact rungs must go through contains_whole_term, not tokenizer
        # equality (see the non-goals). A grep for the call is the cheapest proof
        # the ranker reuses the ladder's primitive rather than reinventing one.
        - 'rg -q "contains_whole_term" loom/src/context/rank_source.rs'
        # The confidence invariant covers ExplicitId too - Confidence::from_reasons
        # classifies all three exact reasons as High in one matches!, and an
        # earlier draft suppressed only two of them. Named-test filters, because a
        # module-wide filter cannot prove this specific case exists.
        - 'cd loom && cargo test --lib test_explicit_id_on_non_full_coverage_is_not_high 2>&1 | rg -q "1 passed"'
        - 'cd loom && cargo test --lib test_explicit_id_on_non_full_coverage_is_not_high 2>&1 | rg -q "test result: ok"'
        # A ContextItem field silently left at Default serialises fine and reads
        # wrong; `kind == "source-node"` alone would never catch it.
        - 'cd loom && cargo test --lib test_source_item_carries_every_field 2>&1 | rg -q "test result: ok"'
        # A stage-scoped query must address the STAGE overlay, and a scopeless one
        # the local overlay - a base-only read is the silent-degrade this stage
        # exists to close.
        - 'rg -q "OverlayScope::Local" loom/src/commands/knowledge/context.rs'
      files:
        # Enumerated, NOT "loom/src/context/**": that glob would swallow
        # refresh.rs and refresh/**, which graph-lifecycle owns and which runs
        # CONCURRENTLY with this stage. Two stages writing one file is lost work.
        - "loom/src/context/rank_source.rs"
        - "loom/src/context/rank.rs"
        - "loom/src/context/retrieve.rs"
        - "loom/src/context/pack.rs"
        - "loom/src/context/mod.rs"
        - "loom/src/context/tests/**"
        - "loom/src/orchestrator/signals/retrieval.rs"
        # brief.rs is the surface this whole stage exists to improve. It was
        # read-only in an earlier draft, which meant NO stage could make the brief
        # render the line span - the plan's own thesis, unbuilt, with every gate
        # green. It is disjoint from graph-lifecycle's territory.
        - "loom/src/orchestrator/signals/format/brief.rs"
        - "loom/src/commands/knowledge/context.rs"
        - "loom/src/commands/knowledge/tests_context.rs"
        # pack's signature change breaks this tests/ target too. Absent from an
        # earlier draft, which made `cargo test --all-targets` unpassable for this
        # stage no matter how correct src/ was. Narrow on purpose: NOT
        # "loom/tests/**", because nothing else here belongs to this stage.
        - "loom/tests/integration/context_catalog.rs"
      artifacts:
        - "loom/src/context/rank_source.rs"
        - "loom/src/context/tests/rank_source.rs"
      wiring:
        - source: "loom/src/context/retrieve.rs"
          pattern: "rank_source\\(rank_query"
          description: "Source ranker invoked from rank_channels, not an empty slice"
        - source: "loom/src/context/pack.rs"
          pattern: "ItemKind::SourceNode"
          description: "Packer emits real source items"
        # Anchored on OverlayScope::Stage plus plan_key, not the bare word
        # `overlay:`, because `overlay:` also matches the struct-field declaration
        # and any comment. plan_key is the one correct derivation (see S4 step 1),
        # and the Stage variant is what distinguishes a stage's own overlay from
        # the checkout-wide Local one.
        - source: "loom/src/orchestrator/signals/retrieval.rs"
          pattern: "OverlayScope::Stage\\s*\\{[\\s\\S]{0,80}?crate::context::delivery::plan_key"
          description: "Stage brief queries its own overlay under the delivery plan key"
        - source: "loom/src/orchestrator/signals/format/brief.rs"
          pattern: "pointer\\.line_start"
          description: "Brief renderer reads the source span it was given"
      dead_code_check:
        command: "cd loom && cargo clippy --all-targets -- -D warnings 2>&1"
        fail_patterns:
          - "never used"
          - "never read"
        ignore_patterns: []

    - id: graph-lifecycle
      name: "Automatic Graph Creation and Honest Reporting"
      stage_type: standard
      model: "opus"
      reasoning_effort: "xhigh"
      implementers: ["codex", "claude"]
      subagent_timeout_secs: 900
      working_dir: "."
      dependencies: ["overlay-contract"]
      description: |
        Make the source graph appear without anyone asking for it, and make the one
        manual verb tell the truth about both layers it drives.

        Use parallel subagents and skills to maximize performance.

        BACKGROUND - READ FIRST (do not re-derive):
        - refresh() (loom/src/context/refresh.rs:140-167) calls
          reconcile_semantic_best_effort on BOTH branches: the catalog-current early
          return at :151 and the post-rebuild path at :163. The semantic layer IS
          being reconciled on every sync today.
        - commands/knowledge/sync.rs:13-23 prints only structural fields (rebuilt,
          revision, files, chunks, issues). The semantic result is never printed in
          either human or --json mode. This is the defect users experience as
          "sync does nothing".
        - dirty_tree_reason (refresh/source_graph.rs:128-139) refuses a Base publish
          on any tracked modification. This is CORRECT and stays - a base layer is
          immutable and keyed to a revision (graph_store/mod.rs:196-200).
        - prepare_repo_for_run (commands/run/checks.rs:12) calls
          check_for_uncommitted_changes (checks.rs:99) which bails on a dirty tree,
          and runs BEFORE mark_plan_in_progress in both paths. A base publish placed
          before the rename is always on a clean tree.
        - loom/src/context/local_overlay.rs already exists (overlay-contract, this
          stage's dependency) and exports LOCAL_PLAN_KEY, local_overlay_stage_name
          and local_overlay_key, moved down out of commands/map.rs. USE THEM. An
          earlier draft told G1 to perform that move itself, into
          refresh/source_graph.rs, which was wrong twice over: `mod source_graph;`
          is declared PRIVATE at context/refresh.rs:18, so a pub(crate) item inside
          it is not nameable from commands::map at all (a guaranteed compile
          error), and source_graph.rs is at 398 of its 400 permitted lines.
        - reconcile_semantic_best_effort (refresh/source_graph.rs:355-372) and its
          two helpers derive_project_root (:377) and try_reconcile_semantic (:388)
          are ~45 lines at the bottom of that 398-line file. Because this stage
          must GROW that logic, MOVE those three items into a new
          loom/src/context/refresh/semantic.rs (declared `mod semantic;` in
          refresh.rs beside `mod source_graph;`) before touching them. That is the
          "extract into an unledgered sibling" remedy loom/CONTRIBUTING.md:43
          prescribes, and it keeps source_graph.rs off the ledger.

        SUBAGENT FILE ASSIGNMENTS - NO FILE OVERLAP:

        | Subagent | Files Owned (write) | Files Read-Only |
        | --- | --- | --- |
        | G1 - sync honesty + overlay fallback (loom-codex-forwarder, --model gpt-5.6-terra) | loom/src/commands/knowledge/sync.rs, loom/src/context/refresh.rs, loom/src/context/refresh/semantic.rs (new), loom/src/context/refresh/source_graph.rs | context/local_overlay.rs, commands/map.rs |
        | G2 - run + init hooks (loom-codex-forwarder, --model gpt-5.6-terra) | loom/src/commands/run/checks.rs, loom/src/commands/run/mod.rs, loom/src/commands/run/foreground.rs, loom/src/commands/init/execute.rs | refresh/source_graph.rs, context/local_overlay.rs, fs/plan_lifecycle.rs |
        | G3 - per-stage overlay before spawn (loom-software-engineer, sonnet) | loom/src/orchestrator/core/stage_executor.rs, loom/src/commands/stage/skip_retry.rs | orchestrator/merge_lifecycle.rs, orchestrator/signals/recovery.rs |
        | G4 - tests (loom-codex-forwarder, --model gpt-5.6-terra) | loom/src/context/refresh/tests_source_graph.rs, loom/src/commands/run/tests.rs | all of the above |

        THE ORCHESTRATOR OWNS loom/maintainability-baseline.txt, not any subagent.
        Four ledgered entries move in this stage and the ledger is an EXACT match
        that fails on shrinkage as loudly as on growth (loom/CONTRIBUTING.md:36-46),
        so `cargo test --test maintainability` - which a plain `cargo test` also
        runs - goes red until you update them: `file
        src/orchestrator/core/stage_executor.rs 933`, `function
        src/orchestrator/core/stage_executor.rs start_stage 411`, `file
        src/context/refresh/tests_source_graph.rs 476`, `function
        src/commands/run/foreground.rs execute_foreground 75`, and `function
        src/commands/init/execute.rs execute 122`. Update each to its new exact
        count AFTER all four subagents return, in one edit, and re-run the test.
        Do not hand the ledger to a subagent: two agents editing it is lost work.

        G1 - SYNC HONESTY AND OVERLAY FALLBACK.
        Current signature (commands/knowledge/sync.rs:13):
            pub fn sync(structural_only: bool, json: bool) -> Result<()>

        0. FIRST, THE EXTRACTION. Move reconcile_semantic_best_effort (:355-372),
           derive_project_root (:377-385) and try_reconcile_semantic (:388-394) out
           of refresh/source_graph.rs into a new refresh/semantic.rs, declaring
           `mod semantic;` in context/refresh.rs beside `mod source_graph;` and
           re-pointing refresh.rs:151 and :164. Pure move, no behaviour change, no
           renames - commit it as its own `refactor(context):` commit so the real
           change below reads clean. Reason: source_graph.rs is 398 lines against
           the 400 limit and everything below GROWS this logic.

        1. A TYPED SEMANTIC OUTCOME, because `Freshness` cannot say what happened.
           RefreshOutcome.semantic is a `Freshness` (refresh.rs:52-61), which has
           exactly revision / computed_at / stale / detail. The human line this
           stage promises needs FILE AND NODE COUNTS, and those live on
           SourceGraphOutcome (refresh/source_graph.rs:47-52) where
           try_reconcile_semantic throws them away, returning only `.freshness`
           (:393). An earlier draft said "RefreshOutcome already carries
           `semantic: Freshness`. Print it." and then showed an example line with
           counts in it - not producible. So define, in refresh/semantic.rs:

               /// What the semantic half of `refresh` actually did.
               pub struct SemanticOutcome {
                   pub layer: SemanticLayer,
                   pub files_extracted: usize,
                   pub nodes: usize,
                   pub edges: usize,
                   pub freshness: Freshness,
               }

               /// Which layer the semantic reconcile ended up writing.
               pub enum SemanticLayer {
                   /// Immutable base published for a clean revision.
                   Base { revision: String },
                   /// Working-tree overlay, because the base publish was refused.
                   LocalOverlay { plan: String, stage: String, refusal: String },
                   /// Nothing ran: --structural-only, or an unresolvable root.
                   Skipped { reason: String },
               }

           Change RefreshOutcome.semantic from `Freshness` to `SemanticOutcome`.
           The blast radius is small and entirely inside this stage: refresh.rs
           :124-127, :149, :151, :161-164, and the three assertions at
           tests_source_graph.rs:432, :447, :473 (G4's file). `StoreState.semantic`
           stays a `Freshness` and is untouched - do not confuse the two.
           MACHINE-READABLE STATE GOES IN `layer`, NOT IN `freshness.detail` PROSE.
           A caller must never have to substring-match an English sentence to learn
           which layer it got.

        2. PRINT IT, BOTH MODES. Human mode gains one line per layer, e.g.
               Catalog already current
                 Revision: <structural revision>
               source graph: published base for <short rev> (1284 files, 21903 nodes)
           or, on refusal:
               source graph: working-tree overlay _local/map-loom (base not published - tree dirty)
           JSON mode gains a "semantic" OBJECT beside the existing five keys, with
           keys: layer ("base" | "local-overlay" | "skipped"), revision, plan,
           stage, files, nodes, edges, stale, detail. print_json (sync.rs:25-35) is
           a serde_json::json! literal, so this is one nested object.
           A refusal must ALSO be visible on stderr, prefixed with the literal
           "source graph: " (the same prefix G2 introduces - see G2 step 1; there
           is NO pre-existing shared advisory marker in this codebase, so do not go
           hunting for one), so a scripted caller that only reads stdout still sees
           it.

        3. OVERLAY FALLBACK. In refresh/semantic.rs, when reconcile_source_graph
           returns a degraded outcome because the Base publish was refused for
           dirtiness (dirty_tree_reason fired - detect it by the Base call yielding
           a never-built Freshness whose detail names the refusal, or better, by
           calling dirty_tree_reason yourself BEFORE choosing a scope, which is
           unambiguous), retry against the same project root with

               let (plan, stage) = crate::context::local_overlay::local_overlay_key(project_root);
               SourceGraphScope::Overlay { plan, stage }

           and return `SemanticLayer::LocalOverlay { plan, stage, refusal }` with
           the overlay's real counts. Do NOT re-derive the naming rule or re-spell
           "_local": overlay-contract moved both into
           loom/src/context/local_overlay.rs precisely so there is exactly ONE
           definition, and source-channel's reader resolves the SAME function. Two
           independently-maintained copies of an overlay address is a
           data-corruption bug; a mismatched pair here means sync writes a graph
           that `loom knowledge context` can never find.
           Result: `loom knowledge sync` ALWAYS produces a usable graph, and
           because the reader defaults to OverlayScope::Local, always a READABLE
           one. Clean tree gets an immutable base; dirty tree gets a working-tree
           overlay; the output says which.

        4. RULE 13 - PROVE THE WRITE LANDED. A sandboxed worktree stage cannot be
           granted write access to the MAIN repo's .loom/cache (both emitters strip
           `../`; see "Verification facts"), and a denial exits 0 because
           reconcile_source_graph degrades to a zero-count Freshness. So after your
           first `loom knowledge sync` in this worktree, run
           `fd . .loom/cache/context-v1/graph -t f | head` (or the main repo's path
           if `.loom` here is empty) and confirm a layer file actually exists. If
           nothing was written, that is a BLOCKER to report, not a test to relax.

        5. Preserve the existing --structural-only semantics exactly: that flag
           still skips the semantic half entirely, reports
           SemanticLayer::Skipped { reason }, and its output says so rather than
           printing a stale semantic line.

        G2 - RUN AND INIT HOOKS.
        1. Add to loom/src/commands/run/checks.rs, following the exact shape of
               pub fn advisory_codex_lane_preflight(repo_root: &Path)
           (checks.rs:38-57) - eprintln! only, never bail!, never Result:
               pub fn advisory_source_graph_preflight(repo_root: &Path, work_dir: &WorkDir)
           It opens ContextStore::open(work_dir), calls store.ensure()? (store.rs:70,
           idempotent - commands/map.rs:143-144 does exactly this pair, and skipping
           it makes the very first publish on a fresh checkout fail on a missing
           directory), builds GraphStore::new(store.root(), work_dir.root()),
           resolves HEAD via run_git_checked(&["rev-parse", "HEAD"], repo_root)
           (git/runner.rs:70 - it already trims, so the value is safe to use as a
           filename component), and if graph_store.load_base(&head)? is None, calls
               reconcile_source_graph(&store, &graph_store, repo_root, SourceGraphScope::Base { revision: head })
           printing one line on success ("✓ Source graph published for <short rev>")
           and one advisory on failure. It returns nothing and swallows every error.
           If a base already exists for HEAD it is SILENT - no output on the common
           path.
           ADVISORY PREFIX - do not go looking for a shared marker constant; there
           is none. advisory_codex_lane_preflight prints BARE text with no marker,
           check_for_uncommitted_changes uses `"✗".red().bold()` (checks.rs:103-104),
           and foreground.rs uses a literal "Warning: ". Use the string
           "source graph: " as the prefix on every advisory line this function and
           G1 emit, and state in the docstring that it is a NEW convention chosen
           here because no shared one existed. G1 uses the same prefix so the two
           surfaces agree.
        2. Call it from BOTH run paths, in each case AFTER the tree-clean check and
           BEFORE the plan rename:
             - loom/src/commands/run/foreground.rs: between the existing
               `resolve_backend_flag` call (:32) and
               `plan_lifecycle::mark_plan_in_progress(&work_dir)?` (:35).
             - loom/src/commands/run/mod.rs: inside prepare_background_run, after
               advisory_codex_lane_preflight (:94) and before
               mark_plan_in_progress (:97).
           The ordering is load-bearing and must be stated in a comment at both
           sites: the rename dirties a tracked file, so a base publish after it is
           refused every time.
        3. loom/src/commands/init/execute.rs: after the
           `if let Some(path) = plan_path { ... } else { ... }` block that ends at
           :180 (it contains initialize_with_plan_acknowledgement at :171-176) and
           before guard.disarm() (:183), call the same advisory. Place it AFTER the
           if/else, not inside it, so a plan-less `loom init` publishes too.
           `repo_root` and `work_dir` are both already in scope.
           CORRECTION TO AN EARLIER DRAFT - it claimed "the G1 overlay fallback is
           what makes this useful". THAT IS FALSE AS SPECCED and you must not rely
           on it: G1 adds the fallback inside the semantic-reconcile path
           (refresh/semantic.rs after G1 step 0), whereas the advisory in step 1
           calls reconcile_source_graph directly with a Base scope. Nothing falls
           back.
           SO: advisory_source_graph_preflight takes a `allow_overlay_fallback: bool`
           third parameter. Both `loom run` call sites pass false (the tree is
           provably clean there, and a base is what a run needs). `loom init`
           passes true, and on that path a dirty-tree refusal is followed by an
           Overlay publish using
               let (plan, stage) =
                   crate::context::local_overlay::local_overlay_key(repo_root);
               SourceGraphScope::Overlay { plan, stage }
           - the SAME helper G1 and source-channel's reader use (overlay-contract
           landed it). Never spell "_local" or "map-<dir>" by hand anywhere.
           Print which of the two ran. Without this, `loom init` on any repo with
           uncommitted work - the common case, since `loom init` is often the first
           thing run in a dirty checkout - publishes nothing and says so, which is
           honest but useless, and the plan's "created at loom init" goal goes
           unmet.

        G3 - PER-STAGE OVERLAY BEFORE SPAWN.
        In loom/src/orchestrator/core/stage_executor.rs, inside start_stage (:182):
        after the worktree exists (get_or_create_worktree at :347-365) and BEFORE
        generate_signal_with_skills (:465), add:
            MergeLifecycle::new(stage_id, &self.config.repo_root, &self.config.work_dir)
                .reconcile_overlay();
        This is the same call merge_handler.rs:612-613 already makes before a merge;
        reconcile_overlay (merge_lifecycle.rs:73-84) already checks the worktree
        exists and already degrades instead of failing, so no new error handling is
        needed. MergeLifecycle::new(stage_id, repo_root, work_dir) is
        merge_lifecycle.rs:61 and takes three &-refs; `self.config.repo_root` and
        `self.config.work_dir` both exist on the executor's config.
        Ordering is the whole point and must be commented: the overlay must be
        current BEFORE the signal is generated, or the stage's Knowledge Brief
        describes the pre-stage tree.

        THAT CALL SITE COVERS FRESH SPAWNS ONLY - THE RECOVERY PATH BYPASSES IT,
        and an earlier draft missed this entirely, leaving the stage's headline
        promise ("refreshed per stage before the session spawns") false on exactly
        the reruns that need it most. Read stage_executor.rs:412-449: when
        `pending_recovery_signal` returns Some, start_stage REUSES the already
        written recovery signal file (:447-449) and never reaches
        generate_signal_with_skills at all. Those bytes were produced earlier, by
        generate_recovery_signal (signals/recovery.rs:19-50), which builds and
        embeds the retrieval pack itself at recovery.rs:38
        (`embedded_context.context_pack = retrieve_stage_pack(work_dir, stage)`).
        Reconciling the overlay later in start_stage cannot change a file that is
        already on disk. So `loom stage retry --context`, crash recovery and hang
        recovery would all brief the agent from the PRE-crash tree.
        FIX AT THE PRODUCER, NOT THE CONSUMER. The single production caller of
        generate_recovery_signal is loom/src/commands/stage/skip_retry.rs:192
        (confirm with `rg -n "generate_recovery_signal" loom/src`; if a second
        production caller has appeared, patch it the same way and say so). In that
        function, immediately BEFORE the generate_recovery_signal call and after
        the RecoverySignalContent is built, add the same reconcile:
            MergeLifecycle::new(&stage.id, repo_root, work_dir).reconcile_overlay();
        with a comment saying the pack is embedded at signal-WRITE time here, not
        at spawn time, so the overlay must be current now or the retry is briefed
        from the pre-crash tree. reconcile_overlay already early-returns when the
        worktree is missing (merge_lifecycle.rs:76-79), which is the right
        behaviour for a stage whose worktree was cleaned up. Derive repo_root and
        work_dir from what that function already has in scope; do not add
        parameters to the public `loom stage retry` surface.

        TWO THINGS TO SETTLE HERE RATHER THAN LEAVE TO INTEGRATION-VERIFY:

        (a) COST. reconcile_overlay walks the worktree with tree-sitter. On this
        repo `loom map` reports 901 files / 8714 nodes / 35033 edges, and the
        daemon's poll loop is 5s. Measure it once before wiring: run
        `time ./target/debug/loom map --outline src/main.rs` (which performs the
        same working-tree overlay reconcile via commands/map.rs load_graph) and
        record the number in a `loom memory note`. reconcile_source_graph is
        incremental - it reuses a cached entry whenever body_hash matches
        (refresh/source_graph.rs:212-245) - so the steady-state cost is proportional
        to CHANGED files, and only the very first call on a fresh worktree pays the
        full walk. If the cold call exceeds ~10s, say so in the note and place the
        call so it does not block the poll loop; do NOT silently accept an unbounded
        synchronous walk on the spawn path.

        (b) SCOPE. Patch start_stage ONLY. start_knowledge_stage
        (stage_executor.rs:110) runs in the main repo with no worktree, so
        reconcile_overlay would early-return at merge_lifecycle.rs:76-79 anyway.
        State that in the comment so a later reader does not "fix" the omission and
        introduce a pointless walk of the main repo on every knowledge stage.

        (c) KEY AGREEMENT. The overlay this writes is read back by source-channel's
        S4 through crate::context::delivery::plan_key(stage). This writer keys it by
        MergeLifecycle::plan_id() (merge_lifecycle.rs:237-245), read from
        .work/config.toml. THOSE ARE TWO DIFFERENT DERIVATIONS and if they disagree
        the reader silently gets the base layer with no error. Confirm they agree
        for a real stage before completing; if they do not, change
        MergeLifecycle::plan_id to route through
        crate::context::delivery::plan_key_from(...) so there is exactly ONE
        derivation - merge_lifecycle.rs is in this stage's files: for that reason.

        G4 - TESTS. Follow the existing style in
        context/refresh/tests_source_graph.rs (TempDir + ContextStore::with_root +
        GraphStore::new, no daemon). NAME each new test exactly as given below:
        this stage's acceptance filters on these names one by one, because a
        filter on `context::refresh` proves nothing - that module already has many
        passing tests and its file is ledgered at 476 lines, so "some nonzero tests
        passed" would be green before you wrote a line.
          - `test_dirty_tree_falls_back_to_local_overlay` - a dirty tree yields
            SemanticLayer::LocalOverlay carrying the local_overlay_key pair and
            NON-ZERO counts, not a silent zero-count outcome
          - `test_dirty_tree_overlay_is_readable_through_local_scope` - THE
            REGRESSION GUARD FOR THE WHOLE FALLBACK: on a dirty tree with NO base
            published, run the fallback, then resolve the graph the way retrieval
            does - `graph_store.resolved(&revision, Some((&plan, &stage)))` with
            the pair from `local_overlay_key` - and assert the resolved graph has
            files and nodes. A write nobody can read is what an earlier draft
            shipped; this test is what makes that impossible to reintroduce.
          - `test_clean_tree_publishes_base` - a clean tree yields
            SemanticLayer::Base and a layer file under graph/base/
          - `test_structural_only_skips_semantic_layer` - --structural-only leaves
            the semantic layer untouched and reports SemanticLayer::Skipped
          - `test_preflight_silent_when_base_exists` - the advisory is silent when
            a base already exists for HEAD
        Update the three existing assertions that read RefreshOutcome.semantic as
        a Freshness (tests_source_graph.rs:432, :447, :473) for the new
        SemanticOutcome shape - reach through `.freshness` rather than weakening
        them.
        ORDERING TEST - loom/src/commands/run/tests.rs DOES NOT EXIST. Check with
        `fd -H tests loom/src/commands/run` first; if it is still absent, create it
        and register `#[cfg(test)] mod tests;` in loom/src/commands/run/mod.rs.
        Note also that both insertion sites are free functions with side effects and
        no injectable seam, so a runtime ordering assertion is not writable without
        inventing one. Do NOT invent a seam, and do NOT write a test whose name
        claims an ordering it cannot observe - that is the failure shape in
        mistakes/tests-that-cannot-fail.md. Write the honest structural guard
        instead, and say in the test's own doc comment that it is structural:
            read loom/src/commands/run/mod.rs and loom/src/commands/run/foreground.rs
            with include_str!, find the byte offset of
            "advisory_source_graph_preflight" and of "mark_plan_in_progress" in each,
            and assert the former is less than the latter in BOTH files, with a
            message explaining that the plan rename dirties a tracked file so a base
            publish after it is refused every time.
        That is a real regression guard (it goes red if anyone reorders the calls)
        and it does not pretend to be an integration test.

        MEMORY: record mistakes, decisions and surprises via `loom memory note` /
        `loom memory decision` IMMEDIATELY. NEVER `loom knowledge` (implementation
        stage). NEVER Claude Code auto-memory.
      acceptance:
        - "cd loom && cargo fmt --check"
        - "cd loom && cargo clippy --all-targets -- -D warnings"
        # --all-targets --no-fail-fast: plain `cargo test` stops at the first
        # failing TARGET and silently skips the rest, including maintainability.
        # See "Verification facts" and mistakes/testing-and-lint.md:66-76.
        - "cd loom && cargo test --all-targets --no-fail-fast"
        - "cd loom && cargo build"
        # This stage moves FIVE exact-match ledger entries (stage_executor file +
        # start_stage, tests_source_graph file, execute_foreground, init execute).
        # The ledger fails on shrinkage as loudly as on growth.
        - "cd loom && cargo test --test maintainability"
        # A filter on `context::refresh` is NOT discriminating: that module is
        # already full of passing tests (its file is ledgered at 476 lines), so it
        # would be green before this stage wrote anything. Filter on each NEW test
        # by name instead. Criteria run via `sh -c` with NO pipefail, so a pipe
        # returns only the last command's status - the "test result: ok" half
        # rejects a failing run and the "N passed" half rejects an empty filter.
        # Both are required; see the same note on source-channel.
        - 'cd loom && cargo test --lib test_dirty_tree_falls_back_to_local_overlay 2>&1 | rg -q "1 passed"'
        - 'cd loom && cargo test --lib test_dirty_tree_falls_back_to_local_overlay 2>&1 | rg -q "test result: ok"'
        # The single most important test in this stage: the dirty-tree overlay is
        # READABLE through the same key retrieval resolves. An earlier draft wrote
        # an overlay no reader could ever find and called the stage done.
        - 'cd loom && cargo test --lib test_dirty_tree_overlay_is_readable_through_local_scope 2>&1 | rg -q "1 passed"'
        - 'cd loom && cargo test --lib test_dirty_tree_overlay_is_readable_through_local_scope 2>&1 | rg -q "test result: ok"'
        - 'cd loom && cargo test --lib test_clean_tree_publishes_base 2>&1 | rg -q "test result: ok"'
        - 'cd loom && cargo test --lib test_structural_only_skips_semantic_layer 2>&1 | rg -q "test result: ok"'
        # The fallback must address the overlay through the ONE shared helper, not
        # a re-spelled literal. Anchored on the call, which only the behaviour can
        # produce - `local_overlay` alone would match a `use` line or a comment.
        - 'rg -q "local_overlay_key\(" loom/src/context/refresh/semantic.rs'
        - 'rg --files loom/src/context > /dev/null && ! rg -q "\"_local\"|\"map-" loom/src/context/refresh'
        # The typed outcome, not prose in Freshness.detail: a caller must be able
        # to learn which layer it got without substring-matching English.
        - 'rg -q "SemanticLayer::LocalOverlay" loom/src/context/refresh/semantic.rs'
        # `loom init` must publish, not just `loom run` - it has no wiring entry
        # otherwise and an earlier draft left it entirely unproven.
        - 'rg -q "advisory_source_graph_preflight" loom/src/commands/init/execute.rs'
        # The recovery path embeds its pack at signal-WRITE time, so the fresh-spawn
        # reconcile in start_stage cannot reach it. Without this the stage's headline
        # promise is false on every retry.
        - 'rg -q "reconcile_overlay" loom/src/commands/stage/skip_retry.rs'
      files:
        # Disjoint from source-channel's enumerated territory, which runs
        # CONCURRENTLY with this stage. commands/map.rs is NOT here any more: the
        # helper relocation an earlier draft assigned to G1 moved to the
        # overlay-contract stage, so this stage has no reason to touch map.rs.
        - "loom/src/commands/knowledge/sync.rs"
        - "loom/src/context/refresh.rs"
        - "loom/src/context/refresh/**"
        - "loom/src/commands/run/**"
        - "loom/src/commands/init/execute.rs"
        - "loom/src/orchestrator/core/stage_executor.rs"
        # The recovery signal embeds its pack at write time, so the reconcile has
        # to happen at its producer, not at the spawn path. See G3.
        - "loom/src/commands/stage/skip_retry.rs"
        # G3(c) may need to unify the overlay plan-key derivation here. Disjoint
        # from source-channel's territory.
        - "loom/src/orchestrator/merge_lifecycle.rs"
        # Five exact-match ledger entries move in this stage. Orchestrator-only -
        # never hand this file to a subagent. source-channel runs concurrently and
        # deliberately does NOT list it, so there is exactly one writer.
        - "loom/maintainability-baseline.txt"
      artifacts:
        - "loom/src/commands/run/checks.rs"
        - "loom/src/commands/knowledge/sync.rs"
        - "loom/src/context/refresh/semantic.rs"
      wiring:
        - source: "loom/src/commands/run/mod.rs"
          pattern: "advisory_source_graph_preflight"
          description: "Background run publishes the base graph at startup"
        - source: "loom/src/commands/run/foreground.rs"
          pattern: "advisory_source_graph_preflight"
          description: "Foreground run publishes the base graph at startup"
        - source: "loom/src/commands/init/execute.rs"
          pattern: "advisory_source_graph_preflight"
          description: "loom init publishes the graph - the plan's 'created at init' promise"
        - source: "loom/src/orchestrator/core/stage_executor.rs"
          pattern: "reconcile_overlay"
          description: "Stage overlay refreshed before its signal is generated"
        # The fresh-spawn site above never runs on a retry: start_stage reuses the
        # pre-written recovery signal, whose pack was embedded when it was written.
        - source: "loom/src/commands/stage/skip_retry.rs"
          pattern: "reconcile_overlay"
          description: "Recovery signal's brief is built from a current overlay, not the pre-crash tree"
        # `semantic` alone matches a comment, a doc line, or an unused import.
        # Anchor on the JSON key actually being emitted.
        - source: "loom/src/commands/knowledge/sync.rs"
          pattern: "\"semantic\":"
          description: "sync's JSON payload carries the source-graph layer, not only the catalog"
        # The dirty-tree fallback must address the overlay through the one shared
        # helper. A `_local` literal here is the duplication bug this plan bans.
        - source: "loom/src/context/refresh/semantic.rs"
          pattern: "local_overlay_key\\("
          description: "Dirty-tree fallback writes the same address retrieval reads"
      dead_code_check:
        command: "cd loom && cargo clippy --all-targets -- -D warnings 2>&1"
        fail_patterns:
          - "never used"
          - "never read"
        ignore_patterns: []

    - id: cli-collapse
      name: "Collapse the Knowledge and Map CLI Surface"
      stage_type: standard
      model: "opus"
      reasoning_effort: "xhigh"
      implementers: ["codex", "claude"]
      subagent_timeout_secs: 900
      working_dir: "."
      dependencies: ["source-channel", "graph-lifecycle"]
      description: |
        Reduce the knowledge/map CLI to one manual verb plus the agent-facing
        surface, and rewrite every consumer in the same change. A deletion that
        leaves its consumers behind is worse than no deletion.

        Use parallel subagents and skills to maximize performance.

        DELETE these ten surfaces:
          loom knowledge bootstrap, init, index, check, audit, gc, list, show,
          status, replace-section
          and loom map's analysis mode (--deep / --focus / --overwrite)

        KEEP - user-facing (one verb):
          loom knowledge sync

        KEEP - agent-facing, removed from all user documentation:
          loom knowledge context   (the retrieval pull; embedded in every signal)
          loom knowledge update    (the only write path for knowledge stages)
          loom map --outline / --find-all / --impact
          loom memory *            (entirely out of scope, untouched)

        WHY context AND update SURVIVE, non-negotiable: KNOWLEDGE_CONSUMPTION_CONTRACT
        (orchestrator/signals/cache.rs:56) is embedded in every stage signal and is
        pinned byte-for-byte against CLAUDE.md.template by tests_doctrine.rs:384;
        `loom knowledge update` appears at ~20 sites across cache.rs and sections.rs
        and in hooks/post-tool-use.sh:126 and hooks/commit-guard.sh:295-297. These
        are the token-saving mechanism itself, not sprawl.

        SUBAGENT FILE ASSIGNMENTS - NO FILE OVERLAP:

        | Subagent | Files Owned (write) | Files Read-Only |
        | --- | --- | --- |
        | C1 - CLI + command removal (loom-codex-forwarder, --model gpt-5.6-terra) | loom/src/cli/types_memory.rs, loom/src/cli/types.rs, loom/src/cli/dispatch.rs, loom/src/commands/knowledge/mod.rs, and deletion of check.rs, audit.rs, gc.rs, bootstrap.rs, status.rs and their tests_*.rs | commands/knowledge/sync.rs, commands/knowledge/context.rs |
        | C2 - map collapse (loom-codex-forwarder, --model gpt-5.6-luna) | loom/src/commands/map.rs, loom/src/map/analyzer.rs, loom/src/map/detectors.rs, loom/src/map/knowledge_sync.rs, loom/src/map/mod.rs | commands/knowledge/sync.rs |
        | C3 - signal doctrine (loom-senior-software-engineer, opus) | loom/src/orchestrator/signals/cache.rs, loom/src/orchestrator/signals/format/sections.rs, loom/src/orchestrator/signals/tests_cache.rs, loom/src/orchestrator/signals/tests_doctrine.rs, CLAUDE.md.template, CLAUDE.md | all |
        | C4 - completions + docs (loom-codex-forwarder, --model gpt-5.6-terra) | loom/src/completions/dynamic/mod.rs, loom/src/completions/dynamic/knowledge.rs, loom/src/completions/dynamic/tests/tests_commands.rs, loom/src/completions/dynamic/tests/tests_other.rs, loom/src/completions/dynamic/tests/tests_parser.rs, README.md | cli/types_memory.rs |
        | C5 - skills + sibling plans (loom-codex-forwarder, --model gpt-5.6-terra) | skills/loom-plan-writer/SKILL.md, skills/loom-usage/SKILL.md, commands/distill.md, doc/plans/IN_PROGRESS-PLAN-concerns-sweep.md, doc/plans/PLAN-loom-config-command.md, doc/plans/PLAN-multimodel-phase1.md, doc/plans/PLAN-multimodel-phase2.md, doc/plans/PLAN-multimodel-phase3.md, doc/plans/PLAN-secure-distilled-loom-v2.md | README.md |

        THE ORCHESTRATOR OWNS loom/maintainability-baseline.txt, not any subagent.
        This stage DELETES eleven ledgered function entries and SHRINKS three
        ledgered files, and the ledger is an exact match that fails on shrinkage
        (loom/CONTRIBUTING.md:36-46). After all five subagents return, in one edit:
        DELETE `function src/commands/knowledge/mod.rs list 70`, `function
        src/commands/knowledge/audit.rs audit 133`, `function
        src/commands/knowledge/bootstrap.rs execute 91`, `function
        src/commands/knowledge/check.rs check 86`, `function
        src/commands/knowledge/check.rs print_check_results 67`, `function
        src/commands/knowledge/gc.rs build_gc_system_prompt 118`, `function
        src/commands/knowledge/gc.rs gc 144`, `function
        src/fs/knowledge/gc.rs analyze_gc_metrics_with_promoted 174`, `function
        src/map/analyzer.rs analyze_codebase 55`, `function
        src/map/detectors.rs analyze_dir_recursive 58`; and UPDATE the counts on
        `file src/orchestrator/signals/cache.rs 1121`, `file
        src/orchestrator/signals/format/sections.rs 1176`, `file
        src/orchestrator/signals/tests_cache.rs 687` plus any of their function
        entries C3's edits shortened. Then `cargo test --test maintainability`.
        Take the new numbers from the test's own failure output, not from `wc -l`.

        C1 - CLI AND COMMAND REMOVAL.
        READ THIS FIRST - loom/src/cli/types_memory.rs holds TWO enums, and
        `loom memory *` is explicitly out of scope for this whole plan.
        MemoryCommands (from :151) has its own `List` at :208 and `Show` at :219 -
        the same variant names you are deleting from KnowledgeCommands. Delete BY
        ENUM, never by a name match over the file: a grep-driven edit will silently
        remove `loom memory list` and `loom memory show`. After editing, confirm
        with `./target/debug/loom memory --help | rg -q "list"` and the same for
        "show".
        Remove these variants from KnowledgeCommands (loom/src/cli/types_memory.rs):
        Show (:10), Update stays, ReplaceSection (:27), Init (:40), List (:43),
        Index (:46), Check (:49-61), Audit (:64-76), Gc (:79-91),
        Bootstrap (:94-106), Status (:134-138). Context (:109-131) and Sync
        (:141-148) STAY.
        Remove their dispatch arms in loom/src/cli/dispatch.rs:83-129 (the
        dispatch_knowledge function) - keep only the Update, Context and Sync arms.
        Delete the implementation modules: commands/knowledge/check.rs, audit.rs,
        gc.rs, bootstrap.rs, status.rs, and their sibling tests_check.rs,
        tests_audit.rs, tests_gc.rs, tests_spawn.rs, and the corresponding `mod`
        declarations in commands/knowledge/mod.rs (:2-9). Also delete the
        now-unreferenced handler bodies in commands/knowledge/mod.rs - note the
        declaration ORDER is not the order they are listed in prose:
        show (:16), replace_section (:105), init (:137), index (:170), list (:207).
        `update` (:72) and the private read_content_from_stdin (:56) STAY.

        ALSO DELETE commands/knowledge/spawn.rs, and its `pub mod spawn;` at
        commands/knowledge/mod.rs:7. An earlier draft removed tests_spawn.rs while
        keeping spawn.rs, which would leave a module with tests deleted and no
        callers at all. spawn.rs is "Shared helpers for knowledge commands that
        spawn Claude sessions" (resolve_project_root, KnowledgeSandboxGuard::install,
        arm_sandbox_restore, restore_after) and its ONLY consumers are gc.rs:14,56,
        57,122 and bootstrap.rs:13,49,50,82 - both deleted here. Confirm with
        `rg -n "spawn::" loom/src` before deleting; if a consumer outside
        commands/knowledge/ has appeared, STOP and report instead of deleting.
        Because every item is `pub`, clippy's dead_code lint will NOT catch this if
        you miss it - that is why it is called out (see mistakes/store-without-consumer.md).

        TWO TEST MODULES AN EARLIER DRAFT MISSED ENTIRELY. commands/knowledge/mod.rs
        declares `#[cfg(test)] mod tests;` (:278-279) and
        `#[cfg(test)] #[path = "tests_legacy.rs"] mod tests_legacy;` (:281-283).
        Both exercise deleted verbs and both will fail to compile:
          - commands/knowledge/tests.rs: test_knowledge_init (:96),
            test_init_creates_index (:116), test_list_with_topics_does_not_crash
            (:248), test_index_upgrades_legacy_dir (:266),
            test_index_errors_when_knowledge_dir_missing (:288).
            test_parse_file_type (:23), test_knowledge_update (:131),
            test_knowledge_update_in_worktree_writes_to_worktree (:155) and
            test_update_with_explicit_content (:226) cover surviving surface - KEEP them.
          - commands/knowledge/tests_legacy.rs:
            test_replace_section_legacy_and_hierarchical_identical (:50),
            test_show_legacy_no_arg_does_not_auto_migrate (:111),
            test_show_hierarchical_no_arg_uses_index (:133),
            test_list_legacy_dir_never_auto_migrates (:171) all drive deleted verbs.
            test_update_tier1_byte_identical_legacy_and_hierarchical (:15),
            test_update_topic_target_writes_under_category_dir (:89) and
            test_update_legacy_dir_never_creates_index_or_category_dir (:148) cover
            `update` - KEEP them.

        DO NOT DELETE test_index_upgrades_legacy_dir - REWRITE IT. It is at
        commands/knowledge/tests.rs:266-286 and it is the ONLY test proving the
        flat-to-hierarchical upgrade that this stage promises to MIGRATE into
        `sync`. It currently calls init() then index() and asserts INDEX.md now
        exists on a legacy dir. Rewrite it to call KnowledgeDir::initialize() and
        then sync(false, false), asserting the same thing, and rename it
        `test_sync_upgrades_legacy_dir`. Deleting it would leave the migration
        proven only by a `write_index` grep, and a grep proves presence, not
        behaviour.
        AND AMEND THAT MODULE'S CONTRACT DOCSTRING IN THE SAME EDIT.
        tests_legacy.rs:1-5 states the pinned contract as "a flat dir behaves
        exactly as it did before, and nothing auto-migrates it". Moving the
        flat-to-hierarchical upgrade into `sync` makes the second clause false for
        one command, and leaving the docstring asserting it would leave the file
        lying about its own purpose. Rewrite it to: no READ or UPDATE path
        auto-migrates a flat dir - `update`, and the retrieval paths, leave it flat
        forever - and `loom knowledge sync` is the single, explicit upgrade. The
        three surviving `update`/legacy tests still pin exactly that, so the
        contract narrows rather than disappears.

        DO NOT delete fs/knowledge/dir.rs's write_index or KnowledgeDir::initialize:
        per doc/loom/knowledge/mistakes/knowledge-cli-invariants.md:5-19,
        KnowledgeDir::initialize() is called directly by fs/work_dir.rs:164 (loom
        init) and index::write_index by dir.rs:100 and dir.rs:269
        (refresh_index_if_hierarchical, which runs after every knowledge write).
        Removing the CLI wrappers must not touch the library behavior underneath.

        C1 ALSO OWNS THE fs/knowledge ORPHANS AND THE STALE COMMAND STRINGS.
        loom/src/fs/** is now in this stage's files: because deleting the CLI
        strands library code and user-facing text that name the deleted verbs:

        (i) fs/knowledge/gc.rs becomes a store without a consumer. After gc.rs and
        audit.rs are deleted, the only callers of analyze_gc_metrics are the thin
        wrapper KnowledgeDir::analyze_gc_metrics (fs/knowledge/dir.rs:296-301) and
        fs/knowledge/tests_gc.rs - i.e. tests only. That is ~270 lines plus a
        wrapper plus a test file with zero production callers, and because
        analyze_gc_metrics / GcMetrics / Tier1Metrics / TopicMetrics are `pub`
        re-exported at fs/knowledge/mod.rs:27, clippy will NEVER flag it. Delete
        fs/knowledge/gc.rs, its tests_gc.rs, the wrapper at dir.rs:296-301, and the
        re-export line at mod.rs:27, then re-run `rg -n "GcMetrics|Tier1Metrics|
        TopicMetrics|analyze_gc_metrics|DEFAULT_MAX_PROMOTED_BLOCKS" loom/src` and
        confirm it is empty. Also drop the now-unused
        DEFAULT_MAX_TIER1_LINES / DEFAULT_MAX_TOPIC_LINES import at
        cli/types_memory.rs:3 (they existed only for the Audit variant's defaults)
        and the constants themselves if nothing else uses them.
        Leaving this in place would reproduce, inside the very change that cites it,
        the exact defect class of mistakes/store-without-consumer.md.

        (ii) These live in surviving files and name commands that will no longer
        exist. An earlier draft claimed only three such strings existed in
        loom/src; these are the rest. Rewrite each:
          - context/retrieve.rs:66 - `bail!("Knowledge directory not found. Run
            'loom knowledge init' to create it.")`. `init` is gone and the directory
            is now created by `loom init` / KnowledgeDir::initialize. Point the user
            at `loom init` instead. (retrieve.rs is source-channel's file, but
            cli-collapse DEPENDS on source-channel, so the two are serialized by the
            DAG and this edit is safe here.)
          - fs/knowledge/index.rs:18 - INDEX_HEADER, the literal
            "<!-- generated by `loom knowledge index` — do not edit by hand -->",
            is written into EVERY generated INDEX.md (see doc/loom/knowledge/INDEX.md:1).
            Change it to name the automatic mechanism, e.g.
            "<!-- generated automatically on knowledge writes — do not edit by hand -->".
            Also the module docstring at index.rs:8.
          - fs/knowledge/dir.rs:73, :268 (docstrings) and :273 - a user-facing
            eprintln! telling the operator to run `loom knowledge index`. It now
            regenerates automatically; say that instead.
          - fs/knowledge/types.rs:15 and fs/knowledge/summary.rs:4 - docstrings.
            (summary.rs itself STAYS: dir.rs:280 still calls generate_summary.)
          - fs/knowledge/tests_dir.rs:256 - a comment.
          - context/ingest.rs:19 - docstring naming `loom knowledge status`.
        Finish by re-running the C5 sweep command over loom/src too.
        MIGRATE, do not drop: the flat-to-hierarchical upgrade that
        `loom knowledge index` uniquely performed (index::write_index on a flat
        directory) moves into `loom knowledge sync` - sync calls write_index when
        KnowledgeDir::layout() reports flat. State this in sync's docstring.

        C2 - MAP COLLAPSE. In loom/src/commands/map.rs remove the deep/focus/
        overwrite fields from MapArgs (:24-43) and the entire analysis branch
        (:60-85). `loom map` with no view flag becomes an error naming the three
        view flags. Delete loom/src/map/analyzer.rs, detectors.rs and
        knowledge_sync.rs, and their `pub use`/`mod` lines in loom/src/map/mod.rs;
        keep loom/src/map/views/**, which the surviving flags use.
        The one caller of analyze_codebase outside map.rs is
        commands/knowledge/bootstrap.rs:29, which C1 deletes in the same stage -
        confirm with `rg -n "analyze_codebase" loom/src` before removing, and if any
        other caller has appeared, STOP and report rather than deleting.

        C3 - SIGNAL DOCTRINE. Three literal command strings in generated signal text
        refer to deleted verbs and must be rewritten, then their pinning tests
        updated in the same edit:
          - cache.rs:784 - the knowledge-stage prefix's "Verify what you just wrote"
            block runs `loom knowledge show` / `loom knowledge show entry-points`.
            Replace with reading the file directly (the agent has Read) and keep the
            `loom knowledge context` line that follows it, which is unaffected.
          - cache.rs:590 - the knowledge-distill prefix's "ALWAYS finish distillation
            with `loom knowledge index`" instruction. INDEX.md now regenerates on
            every write (fs/knowledge/dir.rs:269), so the instruction becomes a
            statement that the index is maintained automatically. Do not simply
            delete the line: an agent that believes it must run an index step and
            finds no command will improvise.
          - format/sections.rs:239 - the knowledge-distill table row
            "| Regenerate the index (always run last) | `loom knowledge index` |".
            Remove the row.
        Then update the assertions at cache.rs:878-1105 and tests_cache.rs:220-267
        that assert those literal strings (tests_cache.rs:267 and cache.rs:1105 both
        assert `contains("loom knowledge index")`). tests_doctrine.rs:383 pins
        KNOWLEDGE_CONSUMPTION_CONTRACT byte-for-byte against CLAUDE.md.template's
        `## KNOWLEDGE-FIRST` section: if you change either side you MUST change both,
        and that test passing is the proof you did.

        C3 AND C5 SHARE A TEST FILE - COORDINATE BEFORE EITHER STARTS. This is not
        obvious from the file-ownership table and it has one shared failure mode:
        tests_doctrine.rs pulls in BOTH surfaces with include_str! -
        CLAUDE.md.template at tests_doctrine.rs:39 (C3's file) AND
        skills/loom-plan-writer/SKILL.md at :40 (C5's file) - and
        block_a_agrees_across_every_surface (:172) and
        block_b_agrees_across_every_surface (:192-203) assert BLOCK-A and BLOCK-B
        appear VERBATIM in both. So a C5 edit to the plan-writer skill can turn
        C3's test red, and neither agent owns both sides.
        RULE: C5 must not touch any text inside the BLOCK-A or BLOCK-B regions of
        skills/loom-plan-writer/SKILL.md. C5's edits there are confined to the
        knowledge-bootstrap recipe (:234, :236) and the Section 10 template
        (:747, :757). Before completing, the orchestrator runs
        `cargo test --lib tests_doctrine` and treats any failure as a C3/C5 conflict
        to resolve itself, not to hand back.
        In CLAUDE.md.template, fix the CLI reference line (:332) to name
        `loom knowledge sync` in place of `loom map [--deep] [--focus <area>]`, and
        update Rule 12's knowledge-update guidance so it no longer prescribes
        `loom knowledge index` as a final step.

        C3 ALSO OWNS THE REPOSITORY'S OWN ROOT CLAUDE.md - not the template, the
        checked-in file at ./CLAUDE.md. An earlier draft's sweep list omitted it,
        which meant this plan's stated success condition ("the first sweep must
        return nothing") was unreachable while every acceptance criterion passed.
        CLAUDE.md:5-8 is the KNOWLEDGE-FIRST block and it instructs every agent
        working in THIS repo to "read the index first (`loom knowledge show`)".
        `show` is deleted here. Rewrite that clause to read the index file directly
        (the agent has Read; the path is doc/loom/knowledge/INDEX.md) and keep the
        rest of the layout-detection predicate intact. Then `rg -n "loom knowledge
        (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)|loom
        map (--deep|--focus|--overwrite)" CLAUDE.md` must be empty.
        Root CLAUDE.md is NOT generated from CLAUDE.md.template - the template is
        what loom installs into OTHER projects - so both need the edit and neither
        is derivable from the other.

        C4 - COMPLETIONS AND README.
        completions/dynamic/mod.rs:293 hardcodes `("knowledge", "show" | "update")`
        for file-name completion - reduce to `("knowledge", "update")`.
        mod.rs:202's `cmdline.contains("memory")` check is untouched (memory is out
        of scope). tests/tests_commands.rs:50-59 is
        test_complete_subcommands_knowledge - note it is a set of `contains`
        assertions, not an exact-list equality; rewrite it to assert
        ["update", "context", "sync"] are present AND that "show", "bootstrap",
        "check", "gc", "index" are absent, so it discriminates in both directions.
        TWO MORE TESTS IN THAT SAME FILE break and an earlier draft missed both:
          - test_complete_subcommands_knowledge_index_prefix
            (tests_commands.rs:60-65) asserts BOTH "index" and "init" complete for
            the prefix "in". Delete it; nothing beginning "in" survives.
          - test_complete_flags_knowledge_audit (tests_commands.rs:67-74) asserts
            `knowledge audit`'s flags. Delete it.
        Then remove the bootstrap --model completion case in tests_other.rs
        (around :247, plus the `loom knowledge show` case at :100) and the
        "loom knowledge bootstrap --model opus" parse case in tests_parser.rs:22-23.
        Also fix the docstring at completions/dynamic/knowledge.rs:24, which says
        "Complete knowledge file names for `loom knowledge show/update`".
        In README.md rewrite the Knowledge/Memory CLI block (:236-262 - it starts at
        the "### Knowledge / Memory" heading at :236, not :239), the
        bootstrapping-and-maintenance table (:492-499), the `loom map` lines
        (:276-279), and the `--scope source` paragraph (:486) which currently states
        the source channel searches nothing - that is no longer true after
        source-channel ships. Also remove `loom knowledge bootstrap` and
        `loom knowledge gc` from the headless-billing table (:288-298): with those
        commands gone, the only remaining `claude -p` opt-in is `loom review
        --ai-summary` (verified - those are the table's only three rows).
        SIX MORE README SITES an earlier draft's line ranges did not cover:
          - :55 - the overview sentence "`loom knowledge check` reports coverage,
            `audit` reports structural rot ..., `gc` restructures, and `loom map`
            seeds the base from static analysis". This is the FIRST description a
            reader meets; rewrite it for the collapsed surface.
          - :468 - "The index is regenerated automatically on every knowledge write,
            and by `loom knowledge index`." Drop the second clause.
          - :472 - names `loom knowledge audit` as the arbiter of link form.
          - :474 - `loom knowledge audit` line-budget paragraph.
          - :488 - "Run `loom knowledge sync` ...; `loom knowledge status` reports
            whether the derived artifacts are current." `status` is deleted and
            `sync` now reports both layers itself - rewrite as one sentence.
          - :501 - "`loom knowledge audit` will advise it, and `loom knowledge index`
            (structure only) or `loom knowledge gc` performs the opt-in upgrade."
            The upgrade now happens inside `loom knowledge sync`; say that.

        C5 - SKILLS AND PLAN CRITERIA. This is the load-bearing half of the deletion.
        skills/loom-plan-writer/SKILL.md: the canonical knowledge-bootstrap stage
        recipe prescribes `loom knowledge check --min-coverage 50` as an acceptance
        criterion (:757, in the Section 10 template) and `loom map --deep` as the
        coverage fallback (:234 and :747). Replace the acceptance criterion with the
        artifact and `rg -q "## "` checks the same template already carries, and
        replace the `loom map --deep` recipe with `loom knowledge sync`. Note in the
        skill that knowledge scaffolding and the source graph are now automatic at
        init/run, so a knowledge-bootstrap stage is about CONTENT, not creation.
        ALSO :236 - "Skip ONLY if `doc/loom/knowledge/` is already populated AND
        `loom knowledge check` shows coverage >= 50%". An earlier draft missed this
        line, and it is the most important one in the file: it is the skip condition
        every future plan author evaluates, and it is the same condition THIS plan
        invoked to justify having no knowledge-bootstrap stage. Rewrite it as a
        condition an author can still evaluate once `check` is gone - populated
        knowledge tree plus a clean `loom knowledge sync`.
        Do not touch BLOCK-A or BLOCK-B regions of this file - see the C3/C5 note.
        skills/loom-usage/SKILL.md: remove the deleted verbs from its command
        inventory at :406 (`loom map --deep`), :409 (`loom knowledge check`), :722
        (`loom knowledge check`) and :724 (`loom map --deep`) - an earlier draft
        named only :406 and :724 - plus the knowledge section, and describe the new
        two-verb surface.
        commands/distill.md: remove the `loom knowledge index` final step.
        FIVE tracked plans, not four, and one of them needs its YAML amended.
        The plan-level section "Sibling-plan amendments this plan REQUIRES
        (blocking)" is the authority - read it and execute every row. Summary of
        the sites, all measured:
          - doc/plans/IN_PROGRESS-PLAN-concerns-sweep.md - :99 (prose), :531 (inside
            a stage description) and :546 (the acceptance string). ALSO add the
            blocking note that plan and this one must not run concurrently: both
            write orchestrator/core/stage_executor.rs and
            orchestrator/signals/cache.rs.
          - doc/plans/PLAN-loom-config-command.md - see the dedicated block below.
          - doc/plans/PLAN-multimodel-phase1.md - :46 and :234 (`loom knowledge
            check`), :235 (`loom map --deep`), and the bootstrap.rs OWNERSHIP at
            :132, :134, :499-500, :521. An earlier draft of this plan named only
            phases 2 and 3 and missed phase 1 entirely, which would have left a
            sibling stage owning and editing a file this plan deletes.
          - doc/plans/PLAN-multimodel-phase2.md - :60 and :380.
          - doc/plans/PLAN-multimodel-phase3.md - :40 and :235.
            Both are `loom knowledge check` inside knowledge-bootstrap recipes.
            Replace with the same `rg -q "## "` assertion.
          - doc/plans/PLAN-secure-distilled-loom-v2.md - :519-532, a code-surface
            map naming commands/knowledge/bootstrap.rs and commands/knowledge/**.
            One amended table row plus a short note under the map; that plan has no
            executable acceptance against these files, so a note is the right
            weight.
        Do NOT edit any DONE-* plan, and do NOT edit anything under
        `doc/plans/ archive/` (note the LEADING SPACE in that directory's name) -
        those are history.

        PLAN-loom-config-command.md NEEDS AN AMENDMENT, NOT A NOTE. An earlier draft
        said "editing the sibling's stage YAML beyond its acceptance strings is out
        of scope" and left a note at the top. That is not survivable, because the
        breakage is not prose: that plan has `wiring:` entries at :1026-1031 whose
        `source:` files are `src/commands/knowledge/bootstrap.rs` and
        `src/commands/knowledge/gc.rs`, both DELETED here. A wiring check against a
        nonexistent file can never pass, so the note would leave a permanently
        unexecutable plan on disk. Its whole Subagent C is "Knowledge Command
        Integration". Amend it:
          - delete the two `wiring:` entries at :1026-1031;
          - remove `src/commands/knowledge/bootstrap.rs` and
            `src/commands/knowledge/gc.rs` from that stage's `files:` (:444-445);
          - delete the Subagent C block C1-C5 (:855-938) and any acceptance string
            naming resolve_bootstrap_model / resolve_gc_model / model_default;
          - remove the two config keys and everything that carries them: the schema
            table rows (:17-20), the ownership table rows (:35-36), the
            file-by-file list (:70-71), the struct fields (:254-258), the
            deserialization note (:315), the `knowledge_gc_model` wiring pattern
            (:382-383), the `set` arms (:592-593), the constructor (:642-643), and
            the interactive toggle indices (:688-689, renumbering the remaining
            toggles);
          - replace its knowledge-bootstrap acceptance strings (:132, :135, :169,
            :1169, :1176) with the `rg -q "## " doc/loom/knowledge/<file>.md`
            assertion that plan already uses elsewhere, and the `loom map --deep`
            fallback at :135 with `loom knowledge sync`;
          - leave every OTHER config key in that plan intact - the feature is not
            cancelled, only the two knowledge-model keys are.
        Then add the short marked note at the TOP recording what this plan removed
        and why, and record the same via `loom memory note` for knowledge-distill.
        If any of the line numbers above has drifted, find the construct by name
        rather than by line and say so in your report - do NOT skip a site.

        Before finishing, run BOTH sweeps and confirm each is clean:
          rg -n "loom knowledge (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)|loom map (--deep|--focus|--overwrite)" \
            --glob '!doc/plans/DONE-*' \
            --glob '!doc/plans/ archive/**' \
            --glob '!doc/plans/codex-*' \
            --glob '!doc/plans/*PLAN-automatic-knowledge-and-source-graph.md' \
            --glob '!doc/loom/knowledge/**' \
            --glob '!doc/PROPOSAL-*' --glob '!doc/REPORT-*' \
            --glob '!planning-omissions.md' \
            --glob '!.work/**' .
          rg -n "loom knowledge (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)" loom/src
        THE EXCLUSION LIST IS PART OF THE CONTRACT, not a convenience, and an
        earlier draft's two-glob version made its own stated success condition
        unreachable. Each exclusion is a category, justified:
          - `doc/plans/DONE-*` and `doc/plans/ archive/**` (LEADING SPACE in that
            directory name - `!doc/plans/DONE-*` does not cover its 17 files):
            completed work, never edited.
          - `doc/plans/codex-*` and this plan's own file, under either its PLAN- or
            IN_PROGRESS-PLAN- name: this plan and its review DESCRIBE the deletion,
            so they name the deleted verbs by construction and always will. A sweep
            that fails on its own instructions is not a gate.
          - `doc/PROPOSAL-*`, `doc/REPORT-*`, `planning-omissions.md`: dated
            analysis documents, historical by nature - the same category as a DONE
            plan. Do not rewrite them.
          - `doc/loom/knowledge/**`: historical prose that knowledge-distill
            updates, not this stage.
        Anything else the sweep returns is a live consumer and must be fixed here.

        CODEX SUBAGENT DISCIPLINE: spawn each loom-codex-forwarder in the FOREGROUND
        with an explicit Bash timeout of 900000 ms, the stated --model, and
        --effort xhigh. Codex subagents must NOT run any git command; check
        `git status --short` yourself after each codex run.

        MEMORY: record mistakes, decisions and surprises via `loom memory note` /
        `loom memory decision` IMMEDIATELY. NEVER `loom knowledge` (implementation
        stage). NEVER Claude Code auto-memory.
      acceptance:
        - "cd loom && cargo fmt --check"
        - "cd loom && cargo clippy --all-targets -- -D warnings"
        # --all-targets --no-fail-fast: plain `cargo test` stops at the first
        # failing TARGET. See "Verification facts".
        - "cd loom && cargo test --all-targets --no-fail-fast"
        - "cd loom && cargo build"
        # Ten ledgered function entries are deleted and three ledgered files shrink
        # here; the ledger fails on shrinkage as loudly as on growth.
        - "cd loom && cargo test --test maintainability"
        - 'cd loom && ./target/debug/loom knowledge sync --help'
        - 'cd loom && ./target/debug/loom knowledge --help | rg -q "context"'
        - 'cd loom && ./target/debug/loom knowledge --help | rg -q "update"'
        # Each absence check carries a positive control FIRST. A bare `! rg ...`
        # passes when rg exits 2 on an unreadable path, so it would report green
        # while verifying nothing; `rg --files` proves the corpus is readable, and
        # `loom --version` proves the binary exists, before absence is asserted.
        #
        # ALL TEN removed subcommands, not three. An earlier draft checked
        # bootstrap/check/gc only, so a half-finished deletion leaving `audit`,
        # `list`, `show`, `status`, `init`, `index` or `replace-section` dispatching
        # would have passed every gate.
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge bootstrap --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge init --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge index --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge check --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge audit --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge gc --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge list --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge show --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge status --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge replace-section --help'
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom map --deep'
        # `! loom map --deep` proves the FLAG is gone, not that bare `loom map`
        # stopped running an analysis - clap rejects an unknown flag either way.
        # These two are what pin the new behaviour: bare map exits nonzero and its
        # error names all three surviving view flags.
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom map'
        - 'cd loom && ./target/debug/loom map 2>&1 | rg -q -- "--outline" && ./target/debug/loom map 2>&1 | rg -q -- "--find-all" && ./target/debug/loom map 2>&1 | rg -q -- "--impact"'
        # Deleted MODULES, not just deleted dispatch. `fd` exits 0 with no output
        # when nothing matches, so count the lines instead of trusting the status.
        - 'test "$(fd -a "^(check|audit|gc|bootstrap|status|spawn|tests_check|tests_audit|tests_gc|tests_spawn)\.rs$" loom/src/commands/knowledge | wc -l)" -eq 0'
        - 'test "$(fd -a "^(analyzer|detectors|knowledge_sync)\.rs$" loom/src/map | wc -l)" -eq 0'
        - 'test "$(fd -a "^(gc|tests_gc)\.rs$" loom/src/fs/knowledge | wc -l)" -eq 0'
        - 'rg --files skills README.md CLAUDE.md.template > /dev/null && ! rg -q "loom knowledge check" skills README.md CLAUDE.md.template'
        - 'rg --files skills README.md CLAUDE.md.template commands > /dev/null && ! rg -q "loom knowledge index" skills README.md CLAUDE.md.template commands'
        - 'rg --files skills README.md CLAUDE.md.template > /dev/null && ! rg -q "loom map --deep" skills README.md CLAUDE.md.template'
        # The greps above never look at loom/src, root CLAUDE.md, or the sibling
        # plans - which is where the deleted verbs are most likely to survive (an
        # earlier draft undercounted all three). These close that gap.
        - 'rg --files loom/src > /dev/null && ! rg -q "loom knowledge (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)" loom/src'
        # Root CLAUDE.md:5-8 tells every agent in this repo to run
        # `loom knowledge show`. It is not CLAUDE.md.template and was unowned in an
        # earlier draft.
        - 'rg --files CLAUDE.md > /dev/null && ! rg -q "loom knowledge (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)|loom map (--deep|--focus|--overwrite)" CLAUDE.md'
        # The five sibling plans are named EXPLICITLY rather than globbing
        # doc/plans: that directory also holds this plan (which describes the
        # deletion and therefore names every deleted verb), its codex- review, and
        # a `doc/plans/ archive/` subdirectory - with a LEADING SPACE, so
        # `!doc/plans/DONE-*` does not exclude its 17 historical files. An earlier
        # draft's directory-wide criterion could therefore never pass.
        - 'rg --files doc/plans/IN_PROGRESS-PLAN-concerns-sweep.md doc/plans/PLAN-loom-config-command.md doc/plans/PLAN-multimodel-phase1.md doc/plans/PLAN-multimodel-phase2.md doc/plans/PLAN-multimodel-phase3.md > /dev/null && ! rg -q "loom knowledge (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)|loom map (--deep|--focus|--overwrite)" doc/plans/IN_PROGRESS-PLAN-concerns-sweep.md doc/plans/PLAN-loom-config-command.md doc/plans/PLAN-multimodel-phase1.md doc/plans/PLAN-multimodel-phase2.md doc/plans/PLAN-multimodel-phase3.md'
        # PLAN-loom-config-command.md is amended, not annotated: its two wiring
        # entries point at files this stage deletes, so leaving them makes that
        # plan permanently unexecutable.
        - 'rg --files doc/plans/PLAN-loom-config-command.md > /dev/null && ! rg -q "resolve_bootstrap_model|resolve_gc_model|knowledge_bootstrap_model|knowledge_gc_model" doc/plans/PLAN-loom-config-command.md'
        # PLAN-multimodel-phase1.md must stop owning a file that no longer exists.
        - 'rg --files doc/plans/PLAN-multimodel-phase1.md > /dev/null && ! rg -q "commands/knowledge/bootstrap.rs" doc/plans/PLAN-multimodel-phase1.md'
        # PLAN-secure-distilled-loom-v2.md's code-surface map names the deleted
        # files as PATHS, so the command sweep above cannot see it. Prove the note
        # naming this plan was added instead.
        - 'rg -q "PLAN-automatic-knowledge-and-source-graph" doc/plans/PLAN-secure-distilled-loom-v2.md'
        # The concerns-sweep plan overlaps this one on stage_executor.rs and
        # signals/cache.rs; the two must not run concurrently and that has to be
        # written down where its next reader will see it.
        - 'rg -q "PLAN-automatic-knowledge-and-source-graph" doc/plans/IN_PROGRESS-PLAN-concerns-sweep.md'
        # The legacy-contract docstring said "nothing auto-migrates it", which sync
        # now makes false for exactly one command. It must say which.
        - 'rg -q "loom knowledge sync" loom/src/commands/knowledge/tests_legacy.rs'
        # `loom memory list` / `loom memory show` share their names with deleted
        # KnowledgeCommands variants in the SAME file. Memory is out of scope, so
        # prove it survived rather than trusting the edit.
        - 'cd loom && ./target/debug/loom memory --help | rg -q "list"'
        - 'cd loom && ./target/debug/loom memory --help | rg -q "show"'
        # The flat-to-hierarchical upgrade migrated from `index` into `sync` must be
        # proven by BEHAVIOUR, not by a grep for write_index. This is the rewritten
        # test named in C1.
        - 'cd loom && cargo test --lib test_sync_upgrades_legacy_dir 2>&1 | rg -q "test result: ok"'
        - 'cd loom && cargo test --lib test_sync_upgrades_legacy_dir 2>&1 | rg -q "1 passed"'
      files:
        - "loom/src/cli/**"
        - "loom/src/commands/knowledge/**"
        - "loom/src/commands/map.rs"
        - "loom/src/map/**"
        - "loom/src/completions/**"
        # Deleting the CLI strands library code (fs/knowledge/gc.rs) and leaves
        # user-facing strings naming deleted verbs in fs/knowledge/* and in
        # context/{retrieve.rs,ingest.rs}. See C1 (i) and (ii). Both concurrent
        # stages have finished by the time this one runs, so these are safe to own.
        - "loom/src/fs/knowledge/**"
        - "loom/src/context/retrieve.rs"
        - "loom/src/context/ingest.rs"
        - "loom/src/orchestrator/signals/cache.rs"
        - "loom/src/orchestrator/signals/format/sections.rs"
        - "loom/src/orchestrator/signals/tests_cache.rs"
        - "loom/src/orchestrator/signals/tests_doctrine.rs"
        - "skills/**"
        - "commands/**"
        - "README.md"
        - "CLAUDE.md.template"
        # Root CLAUDE.md is a separate checked-in file from the template and names
        # `loom knowledge show` at :5-8. Unowned in an earlier draft, which made
        # this stage's own sweep condition unreachable.
        - "CLAUDE.md"
        # FIVE sibling plans, not two. config-command needs its YAML amended (its
        # wiring entries point at files deleted here); phase1 owns bootstrap.rs;
        # secure-distilled-v2 needs its code-surface map corrected.
        - "doc/plans/IN_PROGRESS-PLAN-concerns-sweep.md"
        - "doc/plans/PLAN-loom-config-command.md"
        - "doc/plans/PLAN-multimodel-phase1.md"
        - "doc/plans/PLAN-multimodel-phase2.md"
        - "doc/plans/PLAN-multimodel-phase3.md"
        - "doc/plans/PLAN-secure-distilled-loom-v2.md"
        # Ten ledger entries deleted, three ledgered files shrunk. Orchestrator
        # only. graph-lifecycle also lists it, but this stage DEPENDS on that one,
        # so the DAG serializes the two writers.
        - "loom/maintainability-baseline.txt"
      artifacts:
        - "loom/src/cli/dispatch.rs"
        - "loom/src/commands/knowledge/mod.rs"
      wiring:
        - source: "loom/src/commands/knowledge/sync.rs"
          pattern: "write_index"
          description: "Flat-to-hierarchical upgrade migrated into sync"
        - source: "loom/src/completions/dynamic/tests/tests_commands.rs"
          pattern: "\"sync\""
          description: "Completion surface test pins the collapsed subcommand list"
        # The rewritten migration test is the only BEHAVIOURAL proof of the
        # index-into-sync migration; the write_index grep above proves presence only.
        - source: "loom/src/commands/knowledge/tests.rs"
          pattern: "test_sync_upgrades_legacy_dir"
          description: "Migrated flat-to-hierarchical upgrade is proven by a test, not a grep"
      dead_code_check:
        # This stage deletes ten commands. Partial deletion leaving orphaned
        # library code is its characteristic failure mode - and because the
        # stranded items are `pub` re-exports, clippy alone will NOT catch them
        # (see C1 (i)). The grep below is the backstop for the specific modules
        # this stage strands.
        command: "cd loom && cargo clippy --all-targets -- -D warnings 2>&1"
        fail_patterns:
          - "never used"
          - "never read"
        ignore_patterns: []

    - id: integration-verify
      name: "Integration Verification"
      stage_type: integration-verify
      model: "opus"
      reasoning_effort: "xhigh"
      working_dir: "."
      dependencies: ["cli-collapse"]
      description: |
        Final verification. Verify FUNCTIONAL INTEGRATION, not just tests passing.
        NEVER Claude Code auto-memory.

        CONTEXT: read this plan, `loom memory show --all`, and doc/loom/knowledge/.

        BUILD AND TEST, zero tolerance - fix ALL warnings, lints and failures.
        Nothing is "pre-existing".

        RUN THIS REPOSITORY'S CANONICAL GATE SET, NOT A SUBSET. loom/CONTRIBUTING.md
        :13-21 and .github/workflows/ci.yml:46-62, :115-144, :169-189 define it;
        an earlier draft of this stage ran four commands and none of the rest.
        Every one of these is an acceptance criterion below EXCEPT the two noted:
            cargo fmt --check
            cargo clippy --all-targets -- -D warnings
            cargo test --all-targets --no-fail-fast
            cargo build --all-targets
            cargo build --no-default-features      # tree-sitter grammars are a
                                                   # feature boundary and this plan
                                                   # touches the source graph
            cargo test --test maintainability
            RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
        NOT criteria, run them yourself and fix or report:
          - `cargo audit` - CONTRIBUTING lists it, but it needs the cargo-audit
            binary, and a criterion that fails on a missing tool is a false blocker.
            Run it; if the binary is absent, say so explicitly in your report
            rather than silently skipping.
          - plain `cargo test` - deliberately NOT used anywhere in this plan: it
            stops at the first failing TARGET and silently skips the rest
            (doc/loom/knowledge/mistakes/testing-and-lint.md:66-76). If you see
            `error: test failed, to rerun pass --test <name>` in any output, the
            run was INCOMPLETE - re-run with --no-fail-fast before saying anything
            about the suite.

        CODE REVIEW: spawn parallel loom-code-reviewer subagents across security,
        architecture, and test coverage; fix ALL findings with an engineer agent
        (the reviewer is read-only).

        FUNCTIONAL VERIFICATION - this is the gate that matters, because every
        defect this plan targets was a thing that existed but was never reached:
        1. Prove the graph is created without being asked. Use a scratch clone
           (`git clone . $TMPDIR/loom-verify && cd $TMPDIR/loom-verify`) with no
           .loom/cache, run `loom init`, and confirm a layer appears under
           .loom/cache/context-v1/graph/. On a freshly cloned (clean) tree that is
           graph/base/<rev>.json; the overlay fallback path is covered by step 2.
           DO NOT run `loom run --foreground` here. It spawns real terminal
           sessions against real stages and bills them, and it has no "startup
           only" mode - there is no safe way to run it "far enough to reach
           startup" inside a verification stage. The base publish at run startup is
           instead proven by graph-lifecycle's ordering guard plus the two `loom
           run` wiring entries, which is what those exist for. If you want a
           runtime check of the run path, invoke the advisory's own code path via
           `loom knowledge sync` on the clean clone, which reaches the same
           reconcile_source_graph Base publish.
        1b. RULE 13 - PROVE THE CACHE WRITE LANDED, do not trust exit 0. No
           plan-level `allow_write` can grant a worktree stage write access to the
           MAIN repo's `.loom/cache` (both emitters strip `../`; see "Verification
           facts"), and a denial is SILENT: `resolve_catalog` downgrades a refresh
           failure to a warning (retrieve.rs:76-83) and `reconcile_source_graph`
           degrades to a zero-count Freshness (refresh/source_graph.rs:349-360).
           So after the first graph command you run, list the layer directory and
           confirm a file is actually there; if the cache is empty while the
           command exited 0, that is a sandbox denial and a BLOCKER to report - it
           is NOT a reason to weaken criterion 3.
        2. Prove sync reports both layers. Run `loom knowledge sync --json` on a
           CLEAN tree and confirm the output carries a semantic object; dirty one
           tracked file, re-run, and confirm the output names the overlay fallback
           rather than silently succeeding.
        2b. Prove the dirty-tree fallback is READABLE, which is the half an earlier
           draft never checked. In a scratch clone with no `.loom/cache` and no
           base for HEAD: modify one tracked file, run `loom knowledge sync`,
           confirm it reports the local-overlay layer, and THEN run
           `loom knowledge context --query "reconcile_source_graph" --scope source
           --json` in that same clone and confirm it still returns a source item.
           A write nobody can read is the failure mode this plan was reviewed for;
           it is wiring test `dirty-tree fallback is readable` below.
        3. Prove a brief carries a source item. This single check is the plan's
           whole thesis - if it fails, nothing else here matters, which is exactly
           why it is an acceptance criterion below and not only prose here.
           `ItemKind` is `#[serde(rename_all = "kebab-case")]` (schema.rs:73-80), so
           a source item serialises as `"kind": "source-node"`. Run
           `loom knowledge context --query "reconcile_source_graph" --scope source --json`
           (that symbol is a real node in this repo's own graph) and confirm at
           least one item has kind "source-node" AND a non-null line_start.
           THEN prove it reaches the BRIEF, which is the surface that matters and a
           different renderer: `loom knowledge context` renders item.summary but the
           brief does not (see source-channel S4 step 1b). Generate a real signal
           for any stage and confirm its Knowledge Brief section shows a
           `path:line-line` pointer, not a bare path. A source item that only ever
           appears in `--json` is the plan failing quietly.
        4. Prove the deleted verbs are gone and nothing references them - including
           in loom/src and doc/plans, not only in skills/README/template.
        Record discoveries to `loom memory` for knowledge-distill.
      acceptance:
        # The repository's canonical gate set, verbatim. See the description for
        # the two gates deliberately left out of this list and why.
        - "cd loom && cargo fmt --check"
        - "cd loom && cargo clippy --all-targets -- -D warnings"
        - "cd loom && cargo test --all-targets --no-fail-fast"
        - "cd loom && cargo build --all-targets"
        # The source graph sits behind the default-on `source-graph` feature and
        # must degrade to lexical nodes rather than fail to build without it
        # (loom/CONTRIBUTING.md:56-62). This plan changes that subsystem, so the
        # feature boundary is now load-bearing and was untested in an earlier draft.
        - "cd loom && cargo build --no-default-features"
        - "cd loom && cargo test --test maintainability"
        - 'cd loom && RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps'
        # `rg -q "semantic"` would match the word anywhere, including an error
        # string. Strip whitespace first, then require the KEY of an OBJECT.
        - 'cd loom && ./target/debug/loom knowledge sync --json | tr -d " \n" | rg -q "\"semantic\":\{"'
        # Verified to pass today: the outline renderer prints the lowercase kind
        # name via SourceNodeKind::as_str(), e.g. "  L15-L37     function     main".
        - 'cd loom && ./target/debug/loom map --outline src/main.rs | rg -q "function"'
        # THE PLAN'S WHOLE THESIS, as a gate rather than as prose. Without these two
        # the source channel can ship ranking nothing and every other criterion
        # still passes. `reconcile_source_graph` is a real symbol in this repo.
        - 'cd loom && ./target/debug/loom knowledge context --query "reconcile_source_graph" --scope source --json | tr -d " \n" | rg -q "\"kind\":\"source-node\""'
        - 'cd loom && ./target/debug/loom knowledge context --query "reconcile_source_graph" --scope source --json | tr -d " \n" | rg -q "\"line_start\":[0-9]+"'
        # A source-scoped query must ALSO stop emitting the inert notice.
        - 'cd loom && ./target/debug/loom --version && ! ./target/debug/loom knowledge context --query "reconcile_source_graph" --scope source 2>&1 | rg -q "not yet wired into ranking"'
      files:
        # This stage is instructed to fix ALL review and verification findings at
        # zero tolerance, so it needs write territory. With `files:` absent it had
        # none declared at all - a gate that must repair what it finds cannot be
        # scoped narrower than the union of what the four code stages touched.
        - "loom/src/**"
        - "loom/tests/**"
        # Any fix that changes a ledgered file's size needs the ledger too, and
        # this stage runs after all four code stages, so it is the only writer.
        - "loom/maintainability-baseline.txt"
        - "skills/**"
        - "commands/**"
        - "README.md"
        - "CLAUDE.md.template"
        - "CLAUDE.md"
      artifacts:
        - "loom/src/context/rank_source.rs"
        - "loom/src/commands/run/checks.rs"
      wiring:
        - source: "loom/src/context/retrieve.rs"
          pattern: "rank_source\\(rank_query"
          description: "Source channel reachable from the retrieval entry point"
        - source: "loom/src/orchestrator/core/stage_executor.rs"
          pattern: "reconcile_overlay"
          description: "Per-stage overlay refresh reached from the spawn path"
        - source: "loom/src/orchestrator/signals/format/brief.rs"
          pattern: "pointer\\.line_start"
          description: "The brief renders the source span - the plan's actual payload"
      wiring_tests:
        - name: "sync reports the semantic layer as an object"
          command: "cd loom && ./target/debug/loom knowledge sync --json | tr -d ' \\n'"
          success_criteria:
            exit_code: 0
            stdout_contains: ["\"semantic\":{"]
        # `stdout_contains: [sync, context, update]` proves the three survivors are
        # present; it does NOT prove the other ten are gone, and a half-finished
        # deletion passes it. stdout_not_contains is what makes it exact.
        - name: "collapsed knowledge surface is exactly three verbs"
          command: "cd loom && ./target/debug/loom knowledge --help"
          success_criteria:
            exit_code: 0
            stdout_contains: ["sync", "context", "update"]
            stdout_not_contains: ["bootstrap", "audit", "replace-section", "status"]
        - name: "memory surface survived the collapse"
          command: "cd loom && ./target/debug/loom memory --help"
          success_criteria:
            exit_code: 0
            stdout_contains: ["list", "show", "note", "decision"]
        - name: "source channel returns real source nodes with line numbers"
          command: "cd loom && ./target/debug/loom knowledge context --query 'reconcile_source_graph' --scope source --json | tr -d ' \\n'"
          success_criteria:
            exit_code: 0
            stdout_contains: ["\"kind\":\"source-node\"", "\"line_start\":"]
        # THE THREE FUNCTIONAL PROMISES AN EARLIER DRAFT LEFT AS PROSE ONLY. Each
        # ran a scratch clone by hand in the description and had no machine gate,
        # so an implementation could break `loom init`, break the dirty-tree
        # fallback, or leave bare `loom map` running an analysis, and still satisfy
        # every criterion in this stage.
        #
        # `loom init` publishes on a CLEAN clone: a base layer file must appear.
        # The clone is fresh, so there is no .loom/cache to inherit. `fd` exits 0
        # with no output when nothing matches, so the line count is the assertion.
        - name: "loom init publishes a base graph on a clean clone"
          command: "set -e; L=\"$PWD/loom/target/debug/loom\"; D=$(mktemp -d); git clone -q . \"$D/c\"; cd \"$D/c\"; \"$L\" init >/dev/null 2>&1 || true; printf 'layers=%s\\n' \"$(fd -t f . .loom/cache/context-v1/graph/base 2>/dev/null | wc -l)\""
          success_criteria:
            exit_code: 0
            stdout_not_contains: ["layers=0"]
        # DIRTY TREE, NO BASE, THEN A SOURCE QUERY - the exact sequence the review
        # showed was broken: sync wrote an overlay under a key no reader consulted,
        # so the query returned nothing while every gate stayed green.
        - name: "dirty-tree fallback is readable"
          command: "set -e; L=\"$PWD/loom/target/debug/loom\"; D=$(mktemp -d); git clone -q . \"$D/c\"; cd \"$D/c\"; printf '\\n// dirty\\n' >> loom/src/main.rs; \"$L\" knowledge sync 2>&1 | tr -d ' \\n'; \"$L\" knowledge context --query 'reconcile_source_graph' --scope source --json | tr -d ' \\n'"
          success_criteria:
            exit_code: 0
            stdout_contains: ["overlay", "\"kind\":\"source-node\"", "\"line_start\":"]
        # Bare `loom map` must now error and name its three surviving view flags.
        # `! loom map --deep` only proves clap rejects a removed flag.
        - name: "bare loom map errors and names the view flags"
          command: "cd loom && ./target/debug/loom map 2>&1 || true"
          success_criteria:
            exit_code: 0
            stdout_contains: ["--outline", "--find-all", "--impact"]

    - id: knowledge-distill
      name: "Knowledge Distillation"
      stage_type: knowledge-distill
      model: "opus"
      reasoning_effort: "xhigh"
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs.
        NEVER Claude Code auto-memory.

        CONTEXT DISCIPLINE (200k window): delegate memory and diff gathering to
        read-only subagents that return compact summaries; stay the sole writer of
        knowledge files.

        Read this plan, `loom memory show --all`, and doc/loom/knowledge/.

        Curate mistakes as actionable prevention rules, plus patterns, decisions and
        conventions, via `loom knowledge update`. TIER ROUTING: findings of ~40 lines
        or fewer go inline into the tier-1 file; larger findings go to
        `loom knowledge update <category>/<slug>` with a 2-4 line tier-1 summary plus
        a `[Title](category/slug.md)` link. This layout is hierarchical (INDEX.md
        exists at the knowledge root).

        SPECIFIC KNOWLEDGE THIS PLAN OWES ITS SUCCESSORS:
        - architecture/source-graph.md is now WRONG in its first paragraph: it states
          the graph's only consumer is `loom map`. Update it for the retrieval
          channel and the automatic lifecycle.
        - architecture/context-retrieval.md explains why Channel::Source ranks over
          an empty slice. That explanation is now history - rewrite it, and record
          the replacement design.
        - Record the mistake class this plan fixes, because it is general and this
          codebase has hit it before (see mistakes/store-without-consumer.md): a
          derived artifact was built, persisted, and given a CLI, but never wired to
          the consumer that justified it - and a command silently degraded a whole
          layer while exiting 0 and printing a success line about the other layer.
          Prevention: a layer that a command drives must appear in that command's
          output, success or failure.
        - Record that `loom map --deep` and `loom knowledge sync` were two commands
          that appeared to do nothing for opposite reasons - one because its output
          was already present, one because its failure was written to a JSON file.
        - Record the WRITER/READER ADDRESS mistake as a prevention rule, because it
          is general and this plan was reviewed onto it: a fallback that WRITES a
          derived layer under a key no reader consults is indistinguishable from
          doing nothing, and every gate stays green because the write succeeded.
          `GraphStore::resolved` returns the base layer with NO error when the
          overlay key is absent or None (graph_store/mod.rs:251-271), so the
          degradation is silent by construction. Prevention: when a layer is
          addressed by a composite key, the key belongs to ONE shared definition
          (here `context/local_overlay.rs`), and the test that matters is the
          round trip - write through the producer's address, read through the
          consumer's, assert the same bytes come back. Two derivations that "should
          agree" is the same bug in a different costume; see
          architecture/context-retrieval.md:123-128 for the earlier instance.
        - Record the ledger + concurrency rule this plan had to design around:
          `loom/maintainability-baseline.txt` is an exact match that fails on
          shrinkage, a plain `cargo test` runs it, and it is a single shared file -
          so exactly one CONCURRENT stage may own it, and a plan that grows or
          deletes ledgered code without owning it cannot pass its own acceptance.
        - Record that plain `cargo test` is not this repo's test gate anywhere in a
          loom plan: `--all-targets --no-fail-fast` is, and `--all-targets` is also
          what compiles `loom/tests/**`, where signature changes go unnoticed.

        Update README.md and CONTRIBUTING.md for the collapsed CLI surface and the
        automatic lifecycle - only the sections that changed.
      acceptance:
        # These three prove the files still have headings - they ALSO pass before
        # this stage does anything, so they are a floor, not a gate. The criteria
        # after them are the ones that can actually fail, because each names
        # something only this stage's curation can produce.
        - 'rg -q "## " doc/loom/knowledge/architecture.md'
        - 'rg -q "## " doc/loom/knowledge/patterns.md'
        - 'rg -q "## " doc/loom/knowledge/mistakes.md'
        # Each criterion below was checked against the knowledge base AS IT STANDS
        # TODAY and fails there. A criterion that already passes before the stage
        # runs is a floor, not a gate - `rg -q "retrieval" .../source-graph.md` and
        # `rg -q "source-graph" INDEX.md` were both tried and REJECTED for exactly
        # that reason. Do not "simplify" these back into one of those.
        #
        # architecture/source-graph.md currently asserts loom map is the graph's
        # ONLY consumer. That sentence must be gone and the new consumer named.
        - '! rg -q "Its \*\*only\*\* consumer today is" doc/loom/knowledge/architecture/source-graph.md'
        - 'rg -q "rank_source" doc/loom/knowledge/architecture/source-graph.md'
        # architecture/context-retrieval.md explains why Channel::Source ranks over
        # an empty slice. That explanation is now history and must not survive.
        - '! rg -q "empty slice" doc/loom/knowledge/architecture/context-retrieval.md'
        - 'rg -q "rank_source" doc/loom/knowledge/architecture/context-retrieval.md'
        # The automatic lifecycle is the other half of this plan and is absent from
        # the knowledge base today.
        - 'rg -q "advisory_source_graph_preflight" doc/loom/knowledge'
        # The knowledge base must stop advertising the deleted verbs to future agents.
        - 'rg --files doc/loom/knowledge > /dev/null && ! rg -q "loom knowledge (check|audit|gc|bootstrap|index|init|list|show|status|replace-section)" doc/loom/knowledge'
      files:
        - "doc/loom/knowledge/**"
        - "README.md"
        - "CONTRIBUTING.md"
```

<!-- END loom METADATA -->
