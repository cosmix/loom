# Entry Points

> Key files agents should read first to understand the codebase.
> This file is append-only - agents add discoveries, never delete.

(Add entry points as you discover them)

## Key Utility Locations (Updated)

- `src/utils.rs` — Shared utilities (truncation, formatting, terminal cleanup, color helpers)
- `src/commands/common/mod.rs` — Command-layer utilities (find_work_dir, detect_session, detect_stage_id) + re-exports of utils
- `src/plan/graph/levels.rs` — Generic DAG level computation (compute_all_levels)
- `src/git/merge/lock.rs` — MergeLock for atomic merge operations
- `src/verify/transitions/persistence.rs` — Canonical stage loading (load_stage)

## CLI Simplification Corrections (2026-03-05)

The following entry-points have changed due to CLI simplification:

- `loom handoff create` → now just `loom handoff` (no subcommand). Handler still at `commands/handoff/create.rs`.
- `HandoffCommands` enum removed — handoff args are now directly on the `Commands::Handoff` variant in `cli/types.rs`
- `ReviewCommands`, `GraphCommands` enums removed — flattened to direct variants
- `loom graph show` → `loom graph`, `loom graph edit` → `loom graph --edit`
- `loom review generate` → `loom review`
- `loom verify <stage-id>` → `loom check <stage-id>`
- `loom stage check-acceptance` → `loom stage verify --dry-run`
- `sandbox` and `hooks` CLI commands removed entirely; functionality absorbed into `loom repair`
- `signals/format/sections.rs` - Budget warnings now reference `loom handoff` (not `handoff create`)
