# Remote Control

> The detect-capability, preflight, resolve-invocation shape for external agent binaries.

## Remote Control Capability/Preflight/Resolve Pattern (2026-05-14, extended 2026-08-08)

`--remote-control` requires claude >= 2.1.51 AND claude.ai login auth (no disqualifying env var, `~/.claude/.credentials.json` present). Because the flag exits non-zero on failure, it must never be passed unconditionally. `--remote-control [name]` also takes an optional name argument on newer claude versions — a second, independently-memoized capability probe decides whether to pass it.

**Function split:**

| Function | What it does | When to call |
|----------|-------------|--------------|
| `preflight(path)` | Runs `claude --version` + auth eligibility check | Startup advisory only |
| `resolve(work_dir)` | Mode/marker/preflight gate (unchanged `bool` contract) | Called ONLY by the crash handler's fast-fail check |
| `resolve_invocation(work_dir, session_name)` | Per-spawn gate: layers the `--help` named-arg probe over `resolve()` | Called at every spawn site (via `prepare_session_launch`) |
| `write_unsupported_marker(work_dir)` | Writes `.work/remote_control-unsupported` | Called by crash_handler on fast-fail |

**`resolve_invocation()` check order (all cheap except the two probes, each memoized):**

1. `resolve(work_dir)` false (mode off / marker present / version-or-auth preflight fails) → `Disabled`
2. `find_claude_path()` fails → `Disabled` (defensive — `resolve()` already required this to succeed)
3. Memoized `--help` probe (`cached_named_arg_supported`, its OWN `OnceLock`, separate from the version-preflight cache) — does `claude --help` contain the literal substring `--remote-control [name]`?
   - Yes → `Named(session_name)`
   - No → `Bare`

**Fast-fail fallback (crash_handler.rs):**

- Session crashes within 15 seconds of creation while `resolve()` is true → write unsupported marker → retry with `--remote-control` omitted (via `resolve_invocation` short-circuiting to `Disabled` once the marker exists).
- No new retry code path: the existing exponential-backoff retry handles it; `resolve()` returning false is the only change, same as before this pattern was extended for naming.

**`build_claude_command()` helper (native/mod.rs) — CURRENT signature:**

```rust
pub(crate) fn build_claude_command(
    claude_path: &str,
    model: &str,
    effort: &str,
    permission_mode: &str,
    remote_control: &RemoteControlInvocation,
    escaped_prompt: &str,
) -> String
```

Appends the flag AFTER the prompt positional (required — `--remote-control [name]` is an optional argument, so before the prompt it would swallow the prompt as the name). `Disabled` omits it; `Bare` appends ` --remote-control`; `Named(name)` appends ` --remote-control={escaped name}` — joined with `=`, not a space, so a name beginning with `-` cannot be reparsed as a separate flag by claude's own CLI parser (shell-escaping alone doesn't neutralize a leading `-`, since it's in `shell_escape`'s safe/unquoted charset).

**OnceLock memoization note:**

Both `cached_preflight_enabled()` (version+auth) and `cached_named_arg_supported()` (named-arg support) use process-lifetime `OnceLock<bool>` caches, each keyed by nothing. This is intentional: both probes' outputs (`claude --version` / `claude --help`) are invariant for the lifetime of a daemon process. Config (`mode`) and the marker file are re-read on every `resolve()` call (both cheap) so operator changes or crash-handler writes take effect immediately without restarting the daemon.

**Testing consequence:** because the caches are process-global, the `Bare` vs `Named` branch of `resolve_invocation` cannot be exercised both ways within one `cargo test` binary — the first test in execution order to reach the real probe pins the result for every other test sharing that binary. Don't add a committed test that asserts a specific branch there; it will be order-dependent/flaky. Verify that branch via a one-off isolated run instead (`cargo test --lib <test_name> -- --exact --nocapture`, written temporarily and removed before commit) or manual inspection of a generated wrapper script.
