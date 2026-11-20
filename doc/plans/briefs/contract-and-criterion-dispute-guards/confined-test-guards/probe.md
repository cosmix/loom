# Brief: probe (stage confined-test-guards)

## Goal

Make `confined_commands_runnable()` tell the truth on macOS, and make CI fail when a confined-launch test skips.

## Files you own

- `loom/src/process/sandbox_probe.rs`
- `.github/workflows/ci.yml`

The stage's frozen contract files (your orchestrator names them) are read-only.

## Facts

- `confined_command_blocker()` (`sandbox_probe.rs:180`) returns `Ok(())` off Linux (`:181-183`), so 57 tests guarded by `confined_commands_runnable()` (`:157`) run on macOS and fail inside a host job: the host job wraps `cargo test` in Seatbelt, and macOS refuses a nested profile that differs from the outer one (`sandbox_apply: Operation not permitted`, exit 71).
- On Linux it runs a bare `bwrap ... -- true` (`:184-198`). Leave that probe's arguments unchanged.
- `skip_unless` (`:218-227`) panics when `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`.
- `confined_commands_blocker` (`:170`, memoized) has a production caller: `loom/src/commands/plan/verify/dry_run_exec.rs:55`. After your change a dry run inside a sandbox on macOS reports the blocker once; that is intended. Do not edit that file.
- The pattern to follow: `seatbelt_unavailable()` in `loom/tests/planning_summary_seatbelt.rs:22-37`.

## Change

1. Add `pub fn start_blocker(program: &Path, args: &[&str]) -> Option<String>`: run `program args..` with a null stdin and wait; `None` on exit 0; otherwise `Some(reason)` holding the exit status and the first non-empty stderr line; `Some` when the spawn fails. No platform `cfg`.
2. `confined_command_blocker()`: Linux calls `start_blocker` with the existing bwrap arguments; macOS calls it with `/usr/bin/sandbox-exec`, `-p`, `(version 1)(allow default)`, `/usr/bin/true`, and reports a missing launcher file as a blocker; other platforms keep `Ok(())`.
3. Under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` on macOS, a guard that probes a Linux-only tool must not panic. Run `rg -n 'skip_unless\(' loom/src loom/tests`; for each call whose condition is Linux-only (bwrap, `srt`, user namespaces) confirm the test or its module is `#[cfg(target_os = "linux")]`. Report every call that is not; do not edit files outside your list (the orchestrator routes them).
4. `.github/workflows/ci.yml`: add `LOOM_TEST_REQUIRE_SANDBOX_FREE: "1"` to the `env` of the full-suite `cargo test --all-targets` step in the `test` job and in the `test-macos` job. Find the steps by their command, not by line number; if a step already sets it, leave it.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib process::sandbox_probe` passes.
- `cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings` is clean for your files.
- Report: the calls found in step 3, and whether the probe returned a blocker in your own shell (it should when your shell is sandboxed).
