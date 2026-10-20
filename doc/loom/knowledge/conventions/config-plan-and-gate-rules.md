# Config Plan And Gate Rules

> Config, plan-only, gate rules

## Configuration Is the Only Authority for Configurable Behavior

If the configuration chain (project `.loom/work/config.toml`, then `~/.loom/config.toml`, then the built-in default) can express a behavior, nothing else may change it: no marker file, cache, or state file under the work dir. When the configured behavior cannot run, fail with the cause instead of switching mode. Background: [Live State Pollution](../mistakes/live-state-pollution.md).

## Work That Needs a Loom Plan Goes Entirely Into the Plan

When a set of related changes needs a loom plan for any part of it, the whole set goes into the plan. No part is done directly beforehand: work split across a direct session and a plan loses the plan's review gate, integrity checks and knowledge distillation for the direct part. Owner direction.

## Language-Agnostic Gates

A completion, verification or cleanup rule never hardcodes one ecosystem's directory names or tools (`node_modules`, `__pycache__`, `.venv`, `target/`). Loom runs Java, Elixir, C++, Go and any other project the same way. Derive the rule from a project-owned, reviewable source (committed `.gitignore` files, the plan's declarations, the source graph, a base-commit comparison) or from loom's own state. A failure loom cannot attribute to the stage gets a deterministic self-check, then a dispute; a person is reached only through the dispute cap or a need only a person can meet.

## README Covers Current Behavior Only

The README documents what loom does now. A fixed bug gets no warning box, mechanism write-up, or workaround recipe there; that history belongs in `mistakes/` and the commit log. A cost or limit that was expected but never materialized is deleted, not hedged: the `claude -p` billing warning was removed after the owner confirmed `-p` usage is not charged separately.
