# Remote Control

> The detect-capability, preflight, resolve-invocation shape for external agent binaries.

## Remote Control Capability/Preflight/Resolve Pattern (2026-05-14)

`--remote-control` requires claude >= 2.1.51 AND claude.ai login auth (no disqualifying env var, `~/.claude/.credentials.json` present). Because the flag exits non-zero on failure, it must never be passed unconditionally.

**Three-function split:**

| Function | What it does | When to call |
|----------|-------------|--------------|
| `preflight(path)` | Runs `claude --version` + auth eligibility check | Startup advisory only |
| `resolve(work_dir)` | Per-spawn gate (mode + marker + memoized preflight) | Called at every spawn site |
| `write_unsupported_marker(work_dir)` | Writes `.work/remote_control-unsupported` | Called by crash_handler on fast-fail |

**`resolve()` check order (all cheap):**

1. `[remote_control] mode = off` in `.work/config.toml` → false (operator opted out)
2. `.work/remote_control-unsupported` marker exists → false (mid-run fast-fail)
3. Memoized `preflight()` via `OnceLock` (runs `claude --version` at most once per process) → true/false

**Fast-fail fallback (crash_handler.rs):**

- Session crashes within 15 seconds of creation while `resolve()` is true → write unsupported marker → retry with `--remote-control` omitted.
- No new retry code path: the existing exponential-backoff retry handles it; `resolve()` returning false is the only change.

**`build_claude_command()` helper (native/mod.rs):**

Pure function shared by all four spawn sites (`spawn_session`, `spawn_merge_session`, `spawn_base_conflict_session`, `spawn_knowledge_session`). Signature:

```rust
fn build_claude_command(
    claude_path: &str,
    model: &str,
    effort: &str,
    remote_control_enabled: bool,
    escaped_prompt: &str,
) -> String
```

Appends `--remote-control` before the prompt positional only when `remote_control_enabled` is true. Call `resolve(work_dir)` to compute the flag, then pass the bool into this helper.

**OnceLock memoization note:**

`cached_preflight_enabled()` uses a process-lifetime `OnceLock<bool>`. This is intentional: `claude --version` output is invariant for the lifetime of a daemon process. Config (`mode`) and the marker file are re-read on every `resolve()` call (both cheap) so operator changes or crash-handler writes take effect immediately without restarting the daemon.
