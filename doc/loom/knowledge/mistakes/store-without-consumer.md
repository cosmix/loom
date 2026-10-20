# Store Without Consumer

> Store written, never read

## What Happened

A plan deliberately scoped one feature as "build the store now, wire the consumer
later": a tree-sitter source graph was built, persisted and exposed, while the
retrieval ranker that was eventually to read it was left unbuilt. That is a
legitimate way to split work. What it left behind was a trail of enum variants,
methods, struct fields and CLI values that all **look wired, compile clean, and
are individually defensible** — plus three module docstrings that described the
intended end state in the present tense and were then read as fact by every later
agent, including the distillation stage that mines docstrings for knowledge.

## Why It Is Invisible

**The compiler cannot see it.** Every one of these items is `pub` on a `pub` type in
a `pub` module, and several are re-exported, so `dead_code` never fires. A fresh
`cargo build` over the finished subsystem emitted ZERO warnings.

**Tests do not see it either.** A test can exercise a `pub` item directly, so an
item whose only callers are its own tests looks covered. Worse, a test asserting
behaviour on an unreachable path is a green test standing in for an absent control
(see `mistakes/tests-that-cannot-fail.md`).

**The docstrings actively mislead.** Written alongside the store, they describe
where the design is GOING. A later reader has no way to tell an aspiration in the
present tense from a fact.

## The Concrete Trail (as it was)

| Shape | Location | State |
| --- | --- | --- |
| `ItemKind::SourceNode` | `context/schema.rs:79` | variant NEVER constructed; `pack.rs:73` only ever writes `KnowledgeChunk` |
| `ResolvedGraph::node()` | `context/graph_store/mod.rs:131` | zero callers in src or tests; also an O(nodes) linear scan, so a footgun as well as unused — recommend deleting |
| `ContextItem.excerpt` `None` arm | `pack.rs:88` always sets `Some(bounded_excerpt(..))` | unreachable in production, so the no-excerpt branch in `brief.rs` and its test cover an untaken path |
| `Channel::Source` | `context/schema.rs:48` | parsed by `parse_scope`, in `--help`, in `PackRequest.scope`, serialized into every pack, in `Channel::all()` DEFAULT path — consulted by nothing that can produce an item |

## The Docstring Overclaims, and One Detection Trap Worth Knowing

Three docstrings in the same subsystem described the source graph as a WIRED
retrieval channel while the ranker ranked it over an empty slice. All three were
corrected; the mechanism of one is the reusable lesson:

`context/graph_store/mod.rs:22` attributed a write-timing decision to
`crate::context::reconcile` — **a module that does not exist**. The real owner is
`context::refresh::source_graph::reconcile_source_graph`.

A plain-backtick path like `` `crate::context::reconcile` `` is invisible to
rustdoc intra-doc link checking, so a wrong module path in backticks survives
`cargo build`, `cargo clippy -D warnings` and the whole test suite. Only a human or
a targeted `rg` catches it.

**Prevention:** write module cross-references as intra-doc links
(`[`crate::context::refresh`]`) rather than plain backticks, so a rename or a
typo becomes a rustdoc warning instead of a permanent lie.

## The House Style That Gets This Right

`commands/context/record_edit.rs:12-14` states its own status flatly: consumed by
nothing — it is pure input for a consumer that has not been built. Copy that tone.
A docstring that says "nothing reads this yet" costs one sentence and saves every
later reader from inferring a wiring that is not there.

## Prevention Rules

1. **Name the production caller.** For every new `pub` item, answer "which
   non-test code calls this?" out loud. If the answer is "nothing yet", say so in
   the docstring. Do not ask "does the compiler warn" — for `pub` items it never
   will.
2. **Write docstrings in the tense of the code, not the plan.** If the consumer is
   unbuilt, the present tense is a lie. "Will be read by X" and "read by X" are
   different claims and future agents cannot distinguish them once merged.
3. **A parsed-but-unconsulted input is worse than a missing one.** A flag that is
   accepted, threaded through and serialized teaches every caller it works. If you
   must ship it, emit an honest notice at the point of use rather than silently
   searching nothing.
4. **When a plan says "store now, consumer later", make the seam explicit in the
   plan** — list the shapes that will be unreachable in the interim, so the
   verification gate reviews them as intended-dead rather than rediscovering them
   as suspicious.
5. Cross-reference: the merge-side twin of this failure — writing a durable flag
   before verifying the thing it claims — is in `mistakes/phantom-merges.md`.

## Epilogue: the consumer was wired, and the trail is how you check

`ItemKind::SourceNode` is now constructed (`pack.rs::build_source_item`),
`Channel::Source` now reaches a real ranker (`context::rank_source`), and the
`ContextItem.excerpt` `None` arm is still unreachable in production. That last one
is the tell: closing a store-without-consumer gap does not retire every shape the
gap created, so the trail table is the checklist you re-walk when the consumer
finally lands. Delete what stayed dead; do not assume wiring the headline item
wired the rest.

## Read Receipts Could Not Form, and Fixtures Wrote the Pair Before Completion (2026-10-20)

**What happened:** the Rust read-receipt path (`loom hook read-receipt`, the `read_receipts` context module and rule 2 of `_read_discipline.sh`) shipped with a green suite and never produced a receipt. No `receipt-*.json` was written in real sessions, so rule 2 never escalated a reread. Prepared intents were stranded in every session (18 in one session). PLAN-02-delivery-truth removed the whole path.

**Why:** two independent causes, each enough alone. `result_body` required an `is_error` field that Claude Code omits from successful `Read` results (1,114 of 1,128 stage Read results lack it; the 14 present are `true`). And Claude Code writes a call's tool_result row only after that call's PostToolUse hooks finish (828 of 831 sampled hook rows precede their result row), while the receipt was finished at PostToolUse. The fixtures wrote the tool_use and tool_result pair before invoking the finishing mode, so both assumptions held in tests and neither held in a session. The finishing mode also ran only inside worktree sessions, so intents prepared elsewhere were never finished.

**Prevention:** build a hook fixture from a recorded payload and recorded transcript rows, in the order the harness writes them, and count the artifact the feature exists to produce in a real session before building on it. The saving was never large either: identical same-agent rereads were 29 of 7,612 stage Read results, about 0.4% of Read characters. Measure the saving first.

**Fix:** the path is removed; rereads are measured from transcripts by the `reads` section of `loom usage`. See [Read Advisories and Reread Measurement](../architecture/token-accounting-and-receipts.md#read-advisories-and-reread-measurement) for the revival design if that measurement shows same-agent rereads above 2% of Read characters. The forward receipt has the same ordering fault: [Open Runtime Uncertainties](../architecture/token-accounting-and-receipts.md#open-runtime-uncertainties).
