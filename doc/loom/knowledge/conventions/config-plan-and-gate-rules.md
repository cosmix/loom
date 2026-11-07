# Config Plan And Gate Rules

>

## Configuration Is the Only Authority for Configurable Behavior

If the configuration chain (project `.loom/work/config.toml`, then `~/.loom/config.toml`, then the built-in default) can express a behavior, nothing else may change it: no marker file, cache, or state file under the work dir. When the configured behavior cannot run, fail with the cause instead of switching mode. Background: [Live State Pollution](../mistakes/live-state-pollution.md).

## Work That Needs a Loom Plan Goes Entirely Into the Plan

When a set of related changes needs a loom plan for any part of it, the whole set goes into the plan. No part is done directly beforehand: work split across a direct session and a plan loses the plan's review gate, integrity checks and knowledge distillation for the direct part. Owner direction.

## Language-Agnostic Gates

A completion, verification or cleanup rule never hardcodes one ecosystem's directory names or tools (`node_modules`, `__pycache__`, `.venv`, `target/`). Loom runs Java, Elixir, C++, Go and any other project the same way. Derive the rule from a project-owned, reviewable source (committed `.gitignore` files, the plan's declarations, the source graph, a base-commit comparison) or from loom's own state. A failure loom cannot attribute to the stage gets a deterministic self-check, then a dispute; a person is reached only through the dispute cap or a need only a person can meet.

## README Covers Current Behavior Only

The README documents what loom does now. A fixed bug gets no warning box, mechanism write-up, or workaround recipe there; that history belongs in `mistakes/` and the commit log. A cost or limit that was expected but never materialized is deleted, not hedged: the owner confirmed `-p` usage is not charged separately, so the README carries no `claude -p` billing warning.

## A Completed, Merged Stage Is Final

A stage that is Completed with `merged: true`, or completed by the operator with `--force-unsafe --assume-merged` (`merge_assumed: true`), is final. Loom never re-verifies its merge, never reverts `merged`, and never moves it to MergeBlocked, MergeConflict or any other status, whatever happens to the target branch afterwards (a move outside loom, a target hold, a changed HEAD). `--assume-merged` is the operator's statement that the work landed, and loom takes it as given. Owner direction.

## Prove Reach With `reachable`; `wiring` Is for Seams the Graph Cannot See

A stage proves its new unit is used with `reachable` (a symbol and an entry point, checked on the source graph). `wiring` exists for seams the graph cannot follow: registration by string, configuration, templates, macros and trait objects. A `wiring` pattern never points at a definition (`fn run`, `mod x`, `class Y`): it matches only where the name is declared, so it passes while nothing calls the unit, and `loom plan verify` rejects the shape (`v2_lints/wiring_definition.rs`). A stage whose consumer comes in a later stage or outside the plan declares `consumer_deferred: "<reason>"` and writes no invented wiring check; the reason reaches the stage signal and integration-verify's reviewer brief, and it waives nothing else. Details: [plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md#wiring-patterns-shaped-like-a-definition-consumer_deferred-and-the-leaf-rule). Run `loom plan verify --strict --dry-run` before handing a plan over.
