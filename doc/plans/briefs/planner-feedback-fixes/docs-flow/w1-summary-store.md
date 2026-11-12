# W1: remove the engineering overview from the summary store (codex sol, high)

Read `common.md` beside this file first.

## Goal

The summary `overview/<f>/engineering` stops existing: it is not expected, not rebuilt, not
estimated, not cited, not prompted for, and not committed. The product overview and every per-plan
summary are unchanged.

## Files you own

- `loom/src/planning/summary/**`
- `loom/src/plan_server/summaries/**`
- `loom/tests/planning_summary_contracts.rs`
- `loom/tests/plan_summary_rebuild_contracts.rs`
- `plan-plugin/agents/overview.md`
- `plan-plugin/skills/summaries/SKILL.md`
- `loom/src/planning/guidance/text/summary_codex.md`
- `doc/loom/summaries/overview/**`
- `README.md` (only the lines that describe the engineering overview, about line 485)

## Entry points (find each with `loom map --find-all` / `rg -n`)

- `store.rs`: `overview_parts`, which accepts "engineering"; `engineering_members`;
  `expected_ids`, which pushes `overview/<f>/engineering` (about lines 218 to 277).
- `evaluate/cite.rs` `overview_inputs`; `hashes.rs` about line 83; `brief.rs` about line 156;
  `cost/estimate.rs` about line 36; `format/front.rs` about lines 98 and 148.
- `plan_server/summaries/rebuilds/panel.rs` about line 118; `spawner.rs` about line 191.
- Tests: `store/tests.rs`, `engine/tests.rs`, `cost/feature_tests.rs`, `seal/*tests.rs`,
  `hashes/tests.rs`, `brief/tests.rs`, `evaluate/fixtures.rs` (the `ENGINEERING` const),
  `summaries/rebuilds/tests_ordering.rs`, `rebuilds/panel/tests.rs`, and the two `loom/tests/`
  files.
- `rg -n 'engineering' loom/src/planning/summary loom/src/plan_server/summaries` lists the rest.
  Keep every hit that is about the per-plan `Scope::Engineering` pages.
- Committed data: delete `doc/loom/summaries/overview/feat-planner/engineering.md` and
  `doc/loom/summaries/overview/optim-token/engineering.md`. Before deleting, `rg` for any index,
  manifest or hash file under `doc/loom/summaries/` that lists them, and remove those entries too.
- Prompts: the overview agent and the summaries skill describe two overviews. Make them describe
  only the product overview.

## Traps

- `Scope::Engineering` is also the scope of plan summary pages (`engineering/:doc`). Only the
  overview of that scope goes.
- The concept page layer (`loom/src/plan_server/concept/**`) is W2's, in the next wave. It may stop
  compiling once you remove a store function it calls. That is expected: leave it alone and list,
  in your report, every concept symbol that referenced what you removed.
- Tests that only built the engineering overview are deleted. Tests that build both overviews keep
  the product half.

## Done

`cargo check --manifest-path loom/Cargo.toml --all-targets` passes, except for errors inside
`loom/src/plan_server/concept/**`, which you list. `rg -n 'overview/[^/"]+/engineering'` finds
nothing in your files. Report the files changed and every concept-layer reference you found.
