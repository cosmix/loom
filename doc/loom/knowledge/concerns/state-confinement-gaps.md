# State Confinement Gaps

> Security gaps found on 2026-09-13 that stay open until the `.loom` confinement plan merges: session write access to loom state and to what runs outside the sandbox.

## Open Gaps (2026-09-13)

Tracked by `doc/plans/PLAN-loom-state-confinement.md` (branch `state-confinement`). Until that plan merges:

- **Checkout-rooted sessions can write loom state.** Knowledge, merge and adjudication sessions run in the main checkout with the whole repository writable, `.loom/work` included. They can also write any `.worktrees/<id>/.loom/*-spool.jsonl`, which the daemon attributes to that stage, and any `.worktrees/<id>/.claude/settings.json`, which that stage's session loads.
- **Loom's own git calls run repository hooks.** `core.hooksPath` is `loom/.githooks`, a tracked directory, and loom's git runner does not disable hooks, so a hook committed by a stage runs unsandboxed in the daemon's later git operations and in the operator's own commits.
- **Package grants reach executables on PATH.** `~/.rustup/toolchains` and `~/.local/share/uv` are session-writable (`sandbox/package_caches.rs`); `cargo` and uv-installed tools on PATH execute from them.
- **The codex lane grants all of `~/.codex`** (`codex.rs`), its hooks and config included.
- **The main checkout's `.claude/settings.local.json` is shared** with the operator's interactive sessions and carried a stage's `allow_write` list and `env.LOOM_WORK_DIR` (see [Security and Isolation](../architecture/security-and-isolation.md)).
- **Shared package caches** (cargo registry, npm, bun, pnpm, uv, go, pip) stay session-writable and are executed by the operator's own builds. Private per-session caches are a follow-up plan; this one is an accepted risk.

Remove each bullet when its fix merges.
