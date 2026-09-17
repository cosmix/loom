# State Confinement Gaps

> Shared package-manager caches stay session-writable.

## Open Gap (2026-09-13)

Tracked by `doc/plans/PLAN-loom-state-confinement.md` (branch `state-confinement`, merged 2026-09-14). One accepted risk remains:

- **Shared package caches** (cargo registry, npm, bun, pnpm, uv, go and pip caches) stay session-writable and are executed by the operator's own builds. Private per-session caches are a follow-up plan.
