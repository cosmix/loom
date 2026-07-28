# Remote Control

> Capability detection, preflight and resolution for driving external agent binaries.

## Remote Control Module (loom/src/remote_control.rs)

Claude Code's `--remote-control` flag lets the loom orchestrator drive Claude sessions programmatically. It exits non-zero when prerequisites are unmet, so it must be gated by a preflight check before use.

**Key types:**

- `RemoteControlMode` (`auto` | `off`) — operator-facing switch persisted in `.work/config.toml [remote_control]`.
- `RemoteControlConfig` — the persisted config struct (single `mode` field).
- `RemoteControlStatus` (`Enabled` | `Disabled { reason }`) — preflight result.

**Key functions:**

| Function | Purpose |
|----------|---------|
| `preflight(claude_path)` | Combines version probe + auth-eligibility heuristic |
| `claude_supports_remote_control(path)` | Version gate only (>= 2.1.51) |
| `remote_control_eligible()` | Auth heuristic: no disqualifying env var + `~/.claude/.credentials.json` present |
| `resolve(work_dir)` | Per-spawn gate: checks mode, marker, and memoized preflight |
| `run_startup_preflight(path, work_dir)` | Advisory startup warning if disabled |
| `write_unsupported_marker(work_dir)` | Writes `.work/remote_control-unsupported` |

**Resolution model (in order):**

1. `mode == off` → false (skip)
2. `.work/remote_control-unsupported` marker exists → false
3. Memoized `preflight()` (runs `claude --version` once per process) → true/false

**Fallback / fast-fail path (crash_handler.rs):**

If a native session crashes within 15 seconds of creation while `resolve()` is true, the crash handler writes `.work/remote_control-unsupported` and logs a warning. The existing retry/backoff then respawns the session; on the retry, `resolve()` returns false (marker present) so `--remote-control` is omitted.

**Config persistence:**

`fs/work_dir.rs` exposes `read_remote_control_config()` / `write_remote_control_config()` using the `[remote_control]` section of `.work/config.toml`. Pattern mirrors `read_plan_sandbox` / `write_plan_sandbox`.

**Auth disqualifying env vars (Remote Control requires claude.ai login):**

`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `CLAUDE_CODE_USE_FOUNDRY`
