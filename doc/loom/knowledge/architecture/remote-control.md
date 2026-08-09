# Remote Control

> Capability detection, preflight, resolution, and per-kind session naming for driving external agent binaries.

## Remote Control Module (loom/src/remote_control.rs)

Claude Code's `--remote-control` flag lets the loom orchestrator drive Claude sessions programmatically. It exits non-zero when prerequisites are unmet, so it must be gated by a preflight check before use. `--remote-control [name]` also takes an _optional_ name argument (verified against claude 2.1.226 `--help`) — loom names every spawned session after its stage.

**Key types:**

- `RemoteControlMode` (`auto` | `off`) — operator-facing switch persisted in `.work/config.toml [remote_control]`.
- `RemoteControlConfig` — the persisted config struct (single `mode` field).
- `RemoteControlStatus` (`Enabled` | `Disabled { reason }`) — preflight result.
- `RemoteControlInvocation` (`Disabled` | `Bare` | `Named(String)`) — the concrete per-spawn decision, returned by `resolve_invocation`.

**Key functions:**

| Function                                     | Purpose                                                                                                                                    |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `preflight(claude_path)`                     | Combines version probe + auth-eligibility heuristic                                                                                        |
| `claude_supports_remote_control(path)`       | Version gate only (>= 2.1.51)                                                                                                              |
| `remote_control_eligible()`                  | Auth heuristic: no disqualifying env var + `~/.claude/.credentials.json` present                                                           |
| `resolve(work_dir)`                          | Mode/marker/preflight gate — unchanged `bool` contract, now called ONLY by the crash handler's fast-fail check                             |
| `resolve_invocation(work_dir, session_name)` | **The real per-spawn gate.** Layers a memoized `--help` capability probe over `resolve()`; returns `Disabled`/`Bare`/`Named(session_name)` |
| `run_startup_preflight(path, work_dir)`      | Advisory startup warning if disabled                                                                                                       |
| `write_unsupported_marker(work_dir)`         | Writes `.work/remote_control-unsupported`                                                                                                  |

**`resolve_invocation` resolution model (in order):**

1. `!resolve(work_dir)` (mode off / marker present / preflight fails) → `Disabled`
2. `find_claude_path()` fails → `Disabled` (defensive; `resolve()` already failed closed on this)
3. Memoized `--help` capability probe (`cached_named_arg_supported`, separate `OnceLock` from the version-preflight cache) — does `claude --help` output contain the literal substring `--remote-control [name]`?
   - Yes → `Named(session_name)`
   - No (older claude that accepts the flag but not the optional argument) → `Bare`

**Session naming (`remote_control_session_name`, `orchestrator/terminal/native/launch.rs`):**

Derived in `prepare_session_launch` — the shared funnel both native and tmux backends call — from `stage.name` (falls back to `stage.id` when empty after trim):

| `SessionType`  | Session name                  |
| -------------- | ----------------------------- |
| `Stage`        | `<stage.name>`                |
| `Merge`        | `Merge: <stage.name>`         |
| `BaseConflict` | `Base conflict: <stage.name>` |
| `Knowledge`    | `Knowledge: <stage.name>`     |

**Fallback / fast-fail path (crash_handler.rs):**

If a native session crashes within 15 seconds of creation while `resolve()` is true, the crash handler writes `.work/remote_control-unsupported` and logs a warning. The existing retry/backoff then respawns the session; on the retry, `resolve()` returns false (marker present), so `resolve_invocation` returns `Disabled` and `--remote-control` is omitted. `resolve()` itself was untouched by the session-naming work — only its callers changed (the crash handler still calls it directly; the spawn path now goes through `resolve_invocation`).

**Config persistence:**

`fs/work_dir.rs` exposes `read_remote_control_config()` / `write_remote_control_config()` using the `[remote_control]` section of `.work/config.toml`. Pattern mirrors `read_plan_sandbox` / `write_plan_sandbox`.

**Auth disqualifying env vars (Remote Control requires claude.ai login):**

`ANTHROPIC_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX`, `CLAUDE_CODE_USE_FOUNDRY`

**Known limitations (2026-08-08):**

- `cached_named_arg_supported` (named-arg support) and `cached_preflight_enabled` (version+auth) each memoize on a `OnceLock<bool>` keyed by nothing, ignoring the `claude_path` argument they accept — consistent with each other, but means a `claude_path` that changes mid-process (unlikely) would not be re-probed. Two/three `which::which` lookups still happen per spawn (`resolve()` → `find_claude_path()`, then `resolve_invocation` calls it again) — accepted as consistent with existing precedent, not fixed.
- The wrapper script also exports `CLAUDE_REMOTE_CONTROL_SESSION_NAME_PREFIX=loom` (`native/pid_tracking.rs`) for auto-generated names; claude's own `--help` documents this prefix as applying only to auto-generated names, not an explicit `--remote-control=<name>`. Corroborated by the `--help` text but NOT confirmed against a live claude.ai Remote Control connection — not smoke-testable in this sandbox (no TTY/login flow for an interactive RC session).
- A stage name beginning with `-` is joined to the flag via `=` (`--remote-control=<name>`), not a space, specifically because `--remote-control` takes an _optional_ argument and a space-separated value starting with `-` risks being reparsed as a separate CLI flag by claude's own parser. `shell_escape` alone does not neutralize this: `-` is in its safe, returned-unquoted-verbatim charset.
- The `Bare` vs `Named` branch of `resolve_invocation` cannot be exercised both ways in one `cargo test` binary (shared process-global `OnceLock`). Verified once via an isolated `cargo test --lib <test> -- --exact --nocapture` run against this machine's real claude install (confirmed the full chain: derived name → `Named` → `--remote-control='Merge: ...'` in the generated wrapper script's `exec` line) rather than as a committed test.
