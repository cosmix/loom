# Architecture

> High-level component relationships, data flow, and module dependencies.
> This file is append-only - agents add discoveries, never delete.

(Add architecture diagrams and component relationships as you discover them)

## Module Moves (Mega-Cleanup Refactoring)

### Relocated Modules

- `compute_all_levels` moved from `commands/status/` to `plan/graph/levels.rs` (correct architectural layer)
- `MergeLock` moved from `orchestrator/progressive_merge/` to `git/merge/lock.rs` (correct layer)
- `truncate`/`truncate_for_display` moved from `commands/common/` to `utils.rs` (shared utility, used by 5+ layers)

### Removed Modules

- `verify/gates.rs` — zero callers, dead quality gate code
- `orchestrator/continuation/yaml_parse.rs` — removed
- Escalation system removed from `monitor/failure_tracking.rs` (RecoveryInitiated, StageEscalated, ContextRefreshNeeded, RecoveryType, should_escalate, mark_escalated)
- Dead methods removed: `short_label` from `models/failure.rs`, `get_state`/`heartbeat_watcher` from monitor

### Split Modules

- `commands/knowledge.rs` → `commands/knowledge/{mod,check,gc}.rs`
- `fs/knowledge.rs` → `fs/knowledge/{mod,types,dir,gc}.rs`

### Remaining Known Layering Violations

- `handoff/mod.rs` re-exports from `orchestrator/continuation/` (workaround for spawner dependencies)
- `models/stage/types.rs` imports TruthCheck, WiringTest, etc. from `plan/schema/types.rs` (circular dep)
- `plan/schema/types.rs` re-exports StageType, ExecutionMode, WiringCheck from models (circular dep)

## CLI Simplification (2026-03-05)

The Handoff System section above references `loom handoff create` — this is now just `loom handoff` (subcommands removed). The underlying handler at `commands/handoff/create.rs` is unchanged. The `HandoffCommands` enum was removed; args are on `Commands::Handoff` directly.

Similarly, `sandbox` and `hooks` CLI commands were removed. Their functionality is now in `loom repair --fix`.

## CLAUDE.loom.md Separation (2026-03-18)

Loom orchestration rules now live in `~/.claude/CLAUDE.loom.md` instead of directly in `~/.claude/CLAUDE.md`.

- CLAUDE.md is a thin pointer file with `@import CLAUDE.loom.md`
- This allows users to keep their own content in CLAUDE.md alongside loom rules
- install.sh creates both files; `loom repair --fix` migrates old installations
- init/execute.rs warns if CLAUDE.loom.md is missing
