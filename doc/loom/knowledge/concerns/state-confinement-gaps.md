# State Confinement Gaps

> Confinement gaps: all closed by the 2026-09-14 merge except shared caches

## Open Gaps (2026-09-13)

Tracked by `doc/plans/PLAN-loom-state-confinement.md` (branch `state-confinement`, merged 2026-09-14). One accepted risk remains:

- **Shared package caches** (cargo registry, npm, bun, pnpm, uv, go and pip caches) stay session-writable and are executed by the operator's own builds. Private per-session caches are a follow-up plan.

## Closed by the Confinement Merge (2026-09-14)

Each bullet below was open on 2026-09-13. The mechanisms are described in [Security and Isolation](../architecture/security-and-isolation.md) and [Execution Containment](../architecture/execution-containment.md).

- **Checkout-rooted sessions writing loom state:** every capsule denies `.loom`, `.worktrees` and `.claude` for its location in both the sandbox and `Edit` layers, and sessions send requests through the relay inbox instead of writing `.loom/work`. The srt confinement e2e exercises the knowledge capsule.
- **Loom's git calls running repository hooks:** every git command loom runs passes `-c core.hooksPath=/dev/null`, and the merge gate holds a branch that touches the in-repo hooks directory for review.
- **Package grants reaching executables:** `~/.rustup/toolchains` and `~/.local/share/uv` are no longer granted; pnpm is narrowed to its store.
- **The codex lane's `~/.codex` grant:** `~/.codex/hooks`, `hooks.json` and `config.toml` are denied in every capsule; the srt e2e checks it, including a `hooks.json` that does not exist yet.
- **The shared `.claude/settings.local.json`:** loom no longer writes it. Approvals propagate through `W/permissions/approved.json`, and `loom repair --fix` strips the keys loom used to write.
