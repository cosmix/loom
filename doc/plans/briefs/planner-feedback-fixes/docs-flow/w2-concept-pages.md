# W2: remove the engineering overview page from the concept layer (codex sol, high)

Read `common.md` beside this file first. W1 has already removed the summary from the store (wave
1). The orchestrator pastes W1's report below this brief, including the concept symbols it found.

## Goal

The concept page layer no longer has an overview page for the engineering scope. A request for it
is refused the way the metaplan scope's overview is refused today (`page_key.rs` about line 36,
`Overview` rejects the Metaplan scope). Per-plan pages (`engineering/<doc>`, `plans/<slug>`) and the
product overview are unchanged.

## Files you own

- `loom/src/plan_server/concept/**`
- `loom/src/plan_server/wire/fixtures/concept-overview.json`
- `loom/tests/plan_concept_contracts.rs`

## Entry points

- `page_key.rs`: lines 8 to 12, 36, 55, 93 and 102 (the overview key for `Scope::Engineering`).
- `page.rs`: line 157 (the "Engineering overview" title), lines 131, 204 and 222.
- `key.rs` `key_chips`: the engineering members.
- `empty.rs`: lines 74 to 86, `PlansNoMetaplan`. Keep the empty state if the plan pages still need
  it; delete it only if its only use was the overview.
- `body.rs`: lines 254 to 285, `overview_stand_in`, and the `doc_link` fallback to
  `overview/<f>/engineering`. A plan-scope link with no overview now falls back to the metaplan
  page key.
- `delta.rs`: line 48 loops over `[Product, Engineering]`, which becomes only Product; also lines
  227 to 230 and 296 to 312.
- Tests: `concept/{engine,key,metaplan,route,feature,levels_contract}_tests.rs`, `concept/tests.rs`
  and `loom/tests/plan_concept_contracts.rs`.

## Traps

- Fix every compile error W1 left in this directory first.
- `concept-overview.json` is a wire fixture. Keep its product overview case; drop only the
  engineering one. Any Rust test that compares against it must agree.

## Done

`cargo check --manifest-path loom/Cargo.toml --all-targets` is clean. `rg -n 'Engineering overview'
loom/src` prints nothing. Report the files changed and the tests you deleted and why.
