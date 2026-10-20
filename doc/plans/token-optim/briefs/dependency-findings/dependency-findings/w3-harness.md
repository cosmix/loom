# W3: the offline harness over three recorded pairs

Tier: opus (`loom-senior-software-engineer`, effort high). Wave 4: you start after W2 returned, because the harness renders the section W2 built and measures the change set W1 built. Read `doc/plans/token-optim/briefs/dependency-findings/dependency-findings/common.md` first. Paths are relative to `loom/` unless they start with `doc/`.

The harness answers one question before this plan is accepted, with no model calls: on recorded successor stages, do the new packs carry the dependency files those sessions read, by more than the findings section costs? It is evidence class 2, step 1, of the metaplan (`doc/plans/optim-token-metaplan.md`, "How a saving is shown"). Step 2, the frozen-successor replay, is a post-merge operator check and not yours.

## What you own (write only these)

- `tests/fixtures/dependency_findings/guard-core--guard-integration.json`
- `tests/fixtures/dependency_findings/token-core--tui-tokens.json`
- `tests/fixtures/dependency_findings/token-core--web-tokens.json`
- `tests/fixtures/dependency_findings/guard-core.md`
- `tests/fixtures/dependency_findings/token-core.md`
- `src/orchestrator/signals/tests_dependency_harness.rs` (new)
- `src/orchestrator/signals/tests_dependency_harness/` (new helper modules, only if the main file would pass 400 lines)
- `src/orchestrator/signals/mod.rs` (one module declaration)

## Step 1: copy the fixture

Copy the five files byte for byte from `doc/plans/token-optim/briefs/dependency-findings/dependency-findings/fixtures/` (at the worktree root) into `loom/tests/fixtures/dependency_findings/` with `cp`. Do not copy `capture.py`; it stays beside them as provenance. Never edit the copies: they are measurements, captured on 2026-10-18 at `6fee4ecd` from the archived plans (`doc/plans/archive/DONE-PLAN-target-ref-guard.md`, `DONE-PLAN-token-flow.md`), git history, the archived journals (`.loom/memory/archive/PLAN-target-ref-guard-20261003T180511Z/memory/guard-core.md`, `.loom/memory/archive/PLAN-token-flow-20261014T220227Z/memory/token-core.md`) and the successors' transcripts, none of which a stage worktree can reach. `capture.py`'s docstring defines every field. `.markdownlintignore` already excludes `loom/tests/fixtures/`.

Each JSON holds: `dependency` (id, name, stage_type, working_dir, files, artifacts, `completed_commit`, `merge_commit`, `change_base`, `journal`), `successor` (id, name, stage_type, working_dir, description, files, artifacts, `wiring_descriptions`, dependencies, `base_commit`), `changed_paths` (the dependency's `git diff --name-only <change_base> <completed_commit>`: 28 paths for guard-core, 41 for token-core), `transcripts` (each with `id`, `role` of `main`, `contract` or `worker`, `api_calls`, `spawn` with `subagent_type`/`description`/`prompt` for workers, and `reads`: a list of objects `{path, calls, first_arrival_tokens, replay_weighted_tokens}`, `path` repository-relative), plus `pair` (for example `"guard-core->guard-integration"`), `method` (the capture definitions) and `totals`. Recorded totals: guard-integration 17 transcripts, 71,574 first-arrival and 2,395,454 replay-weighted tokens; tui-tokens 9, 5,909 and 79,954; web-tokens 11, 2,998 and 23,264. `stage_type` is a lowercase string (`"standard"`).

## Step 2: the harness

`src/orchestrator/signals/tests_dependency_harness.rs`, declared in `src/orchestrator/signals/mod.rs` beside `tests_size`, in the same style and for the same reason (a filter on `orchestrator::signals::tests_dependency_harness` must select it):

```rust
#[cfg(test)]
#[path = "tests_dependency_harness.rs"]
mod tests_dependency_harness;
```

Plan 3 added a `tests_doctrine_commands` declaration to that file (directly after `tests_doctrine_waiting`); plan 2 does not edit it. Re-read it and add only these lines.

Three tests, one per pair, each ignored by default because it clones the repository and builds a source graph, and each kept well under the 300 s an acceptance command may take:

```rust
#[test]
#[ignore = "offline measurement over recorded runs; run with --ignored --nocapture"]
fn guard_core_guard_integration_pays_for_itself()

#[test]
#[ignore = "offline measurement over recorded runs; run with --ignored --nocapture"]
fn token_core_tui_tokens_is_reported()

#[test]
#[ignore = "offline measurement over recorded runs; run with --ignored --nocapture"]
fn token_core_web_tokens_is_reported()
```

Read the fixtures through `env!("CARGO_MANIFEST_DIR")` with `serde_json` into structs local to the test module. Per pair:

1. **Scratch checkout.** In a `tempfile::TempDir`: `git clone --quiet --shared --no-checkout <toplevel> repo`, where `<toplevel>` is `git rev-parse --show-toplevel` run in `CARGO_MANIFEST_DIR` (the stage worktree root); then, in `repo`, `git checkout --quiet -B main <successor.base_commit>` and `git remote remove origin`. The last step matters: the clone's `refs/remotes/origin/HEAD` names the worktree's branch, and `git::branch::default_branch` would return it as the target, so `accepted_change_base` would look for a branch the clone does not have. Run every git call with the isolated config `git/merge/test_support.rs::isolated_git` sets (`GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`, `GIT_CONFIG_NOSYSTEM`). Never `git worktree add`, never touch the stage worktree's refs. `--shared` reads the source's objects, so both base commits (`e133f68c`, `51d2020e`, ancestors of this branch) resolve; a shared clone of a linked worktree reads the common object directory (checked on git 2.53 while planning).
2. **State.** Create `repo/.loom/work`; save the dependency's `Stage` (fixture fields; `status: Completed`, `merged: true`, `merge_assumed: false`, `completed_commit`, `stage_type` parsed from the fixture string) and the successor's `Stage` (`plan_id: Some("harness")`, `wiring` built from `wiring_descriptions` with empty `source` and `pattern`) with `crate::verify::transitions::save_stage`; copy the dependency's journal to `repo/.loom/work/memory/<dependency id>.md`.
3. **Graph.** `ContextStore::open(&WorkDir::new(repo)?)`, `GraphStore::new(store.root(), work_dir.root())` (as `tests_brief_e2e.rs::write_stage_overlay` does), then `crate::context::refresh::ensure_snapshot(&store, &graph_store, repo, SnapshotPolicy::BaseOnly)`; assert `outcome.state() == crate::context::freshness::GraphState::Current`, as `context/refresh/tests_states.rs` does. A cold build of this repository took 4.6 s on 2026-10-18.
4. **Arms.** Old paths: for each successor dependency whose record loads, its `files` then `artifacts` through `normalize_dependency_path`, deduplicated: what `StageQuery::dependency_paths` returned before this plan (retrieval.rs:100-123 at `6fee4ecd`), reimplemented here because production no longer has it. New paths: `StageQuery::dependency_paths(&work_dir, &successor)`. Assert the new set equals `changed_paths` as a set: W1's change set reproduces the recorded merge.
5. **Packs**, built the way production builds them, with each arm's paths:
   - stage pack (the `main` and `contract` roles): mirror `retrieval.rs::retrieve_stage_pack` (`StageQuery::new(work_dir, StageQuery::build_stage_query_text(&successor))`, `overlay = StageQuery::stage_overlay_scope(..)`, `stage_dependency_ids = crate::context::delivery::dependency_chunk_ids(..)`, `dependency_paths = <arm>`, `retrieve_for_stage(&query, RetrievalConfig::load(repo).stage_brief_budget_tokens)`);
   - worker pack (each `worker` transcript): mirror `commands/hook/worker_brief.rs::retrieve_pack` as plan 1 left it: query text `crate::commands::hook::worker_brief::worker_query_text(&successor, &spawn.description, &spawn.prompt)`, `.with_surface(Surface::Hook)`, the same overlay, dependency ids and arm paths. Re-read `retrieve_pack` first and mirror what it does at run time. Skip a worker whose `subagent_type` is `loom-codex-forwarder` or starts with `codex:`: after plan 1 those spawns get no brief.
   - Not mirrored, and why: `worker_brief.rs`'s private `declared_paths` (paths a prompt lists under a "Files owned" heading join the boost) and `scope_pack`. This plan does not edit `worker_brief.rs`. To keep the comparison fair, a worker read whose path, or that path relative to the successor's working_dir, appears verbatim in the spawn prompt counts as covered in both arms.
6. **Coverage.** A read is covered by an arm when that transcript's pack holds a source item (`item.source == Channel::Source`) whose `pointer.path`, through `normalize_dependency_path`, equals the read's path. Sum `first_arrival_tokens` and `replay_weighted_tokens` of covered reads per arm. The gain is new minus old.
7. **Section.** `super::generate::build_dependency_findings(&work_dir, &successor, Some(&new_stage_pack))`. Assert it is `Some`, starts with `## Dependency Findings`, and is at most 1,600 bytes (one dependency's headline always fits). Cost = `crate::context::schema::estimate_tokens(&section)` (bytes / 4) times the summed `api_calls` of the `main` transcripts (each main session carries the section on every call; contract signals do not get it).
8. **Report** with `println!` (read with `--nocapture`): per pair the changed-path count; per pack the item ids gained and lost and the knowledge items lost (the metaplan's risk: a large change set can crowd knowledge out of the 3,000-token brief); covered first-arrival and replay-weighted tokens per arm and per role; the section's bytes and tokens; the main calls; the cost; the gain; the verdict; the test's wall time.

**Stop rule**, asserted in `guard_core_guard_integration_pays_for_itself` only (the pair whose successor worked on its dependency's interfaces): the replay-weighted gain must be at least the section cost. On failure, panic with every number in the message, stating that the plan stops here. Never tune the boost, the cap or the fixture to make it pass: a failure is the measurement, and the stage's main agent reports it to the operator with `loom stage block`. The other two pairs are reported, not asserted beyond steps 3, 4 and 7.

Keep every function under 50 lines and each file under 400 (helpers in `tests_dependency_harness/` submodules declared with `#[path]` from the main file if needed). `ensure_snapshot`, `retrieve_for_stage`, `StageQuery`, `Surface`, `Channel`, `RetrievalConfig`, `WorkDir`, `ContextStore`, `GraphStore` and `SnapshotPolicy` are public (`crate::context::*`, `crate::fs::work_dir`); `super::retrieval` and `super::generate` are reachable from a child of `orchestrator::signals`.

## Proof (from `loom/`)

```bash
cargo test --lib orchestrator::signals::tests_dependency_harness::guard_core -- --ignored --nocapture
cargo test --lib orchestrator::signals::tests_dependency_harness::token_core_tui -- --ignored --nocapture
cargo test --lib orchestrator::signals::tests_dependency_harness::token_core_web -- --ignored --nocapture
cargo test --test maintainability
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Paste the three harness reports into your final message. No git commands except the read-only ones and the clone the harness runs inside its TempDir; no edits outside your list.

## Report

The harness numbers per pair, the verdict, each test's wall time, and anything the tree did differently from this brief. Record the verdict with `loom memory note "found: dependency-findings harness ..." --evidence src/orchestrator/signals/tests_dependency_harness.rs`.
