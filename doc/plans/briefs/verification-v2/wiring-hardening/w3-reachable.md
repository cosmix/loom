# wiring-hardening / W3 — `reachable` checks

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D3 (`ReachableCheck`), D11 (the
reachable bullet). Knowledge: `architecture/source-graph.md` (confidence and provenance);
`mistakes/visibility-and-reachability.md`. Code: `context/resolve/impact.rs::impact_with`
(L235; `ImpactOptions` L49-71; `ImpactHit` L33-44, trust is the minimum confidence along the
path), `map/views/mod.rs::find_symbol_matches` (L200-223, exact match first).

Pinned from W1: `crate::context::worktree_graph::{build_for_worktree, WorktreeGraph}`
(signatures in W1's brief).

## Files you own

`loom/src/verify/goal_backward/reachable.rs` (new),
`loom/src/verify/goal_backward/reachable_tests.rs` (new),
`loom/src/verify/goal_backward/mod.rs`, `loom/src/verify/goal_backward/result.rs`.

## Tasks

1. `result.rs`: `GapType::Unreachable`.
2. `reachable.rs`:
   `pub fn verify_reachable(checks: &[ReachableCheck], graph: &WorktreeGraph) -> Vec<VerificationGap>`,
   following DESIGN D11:
   - resolve nodes by exact name, using `find_symbol_matches`' exact branch only;
   - several exact matches ⇒ reachable when any `symbol` node is reached from any `from` node;
   - run `impact_with` from the symbol node with kinds
     `calls,references,implements,extends,contains,imports`, no depth limit, `limit 0`, and
     `min_confidence` from the check;
   - a symbol whose file has no extractor ⇒ a warning line, not a gap;
   - `graph.degraded` is added to every gap's evidence.
3. `mod.rs::run_goal_backward_verification` (already takes `plan_version` from schema-v2): for
   `plan_version == 2` and a non-empty `reachable`, call
   `worktree_graph::build_for_worktree(working_dir)` once and run `verify_reachable`. Do not
   change the function's signature or its callers: `commands/verify.rs` and
   `commands/stage/complete_verification.rs` belong to other stages running in parallel.

## Named tests (binding), in `reachable_tests.rs`

- `reachable_through_call_chain_passes`: `main → run → helper` in a temp Rust crate;
  `reachable: {symbol: helper, from: main}` → no gap.
- `unreachable_symbol_is_a_gap`: `orphan()` defined but never called → gap `GapType::Unreachable`
  with `orphan is not reachable from main`.

## Proof (one command, once, after W1 reports)

`cargo test --manifest-path loom/Cargo.toml --lib verify::goal_backward::reachable`

## Report

Files changed; confirmation that no caller's signature changed; the proof result.
