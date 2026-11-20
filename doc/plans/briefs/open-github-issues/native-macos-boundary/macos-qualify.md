# Brief: macos-qualify (qualify the native lane on macOS)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/native_worker/boundary/probe.rs`, `loom/src/native_worker/boundary/probe_host.rs`, `loom/src/native_worker/boundary/probe_tests.rs`, `loom/src/native_worker/supervisor/preflight.rs`.

Frozen, read-only: `loom/src/native_worker/boundary/probe_contract_tests.rs`.

## Facts (verified)

- `native_backend_qualified` (`probe.rs:28-31`) caches `qualify()` in a `OnceLock`. Off Linux `qualify()` returns `Err("native Codex workers need Linux")` (:33-36). Linux runs `qualify_from(RealHost)` (:38-41, 70-83): bwrap `--size`, a payload boundary, a separate pid namespace, and `codex sandbox` inside it. Callers: `codex_backend.rs:69` (`codex_lane`) and `supervisor/preflight.rs:12`.
- `probe_host.rs` is Linux-only today (`which bwrap`, payload boundary, `codex sandbox`, `/proc/self/ns/pid`).

## Change

1. `probe.rs`: `MacHostProbes` and `qualify_macos_from` exactly as the stage PUBLIC SURFACE states; it runs the three probes in order and returns the first failure as `Err(reason)`. `qualify()` on macOS calls `qualify_macos_from(&RealMacHost)`; other non-Linux platforms keep the refusal (reworded: "native Codex workers need Linux or macOS").
2. `probe_host.rs`, `RealMacHost` (`cfg(target_os = "macos")`):
   - `sandbox_exec`: `/usr/bin/sandbox-exec` exists and `sandbox-exec -p "(version 1)(allow default)" /usr/bin/true` exits 0; exit 71 means loom itself runs inside a sandbox: report "loom runs inside a sandbox; start the daemon from a plain shell".
   - `payload_boundary`: `/usr/bin/true` under `seatbelt_profile::profile(..)` for a scratch spec (pinned by `macos-profile`) exits 0.
   - `codex_home_honoured`: run the Codex binary's version command with `CODEX_HOME` set to an empty scratch dir and confirm it does not read `~/.codex` (for example it reports the scratch home or creates files only there); report the Codex version in the reason on failure.
3. `preflight.rs:12`: unchanged call; check its message reads well for both platforms.

## Tests (`probe_tests.rs`)

Fake hosts: each probe failing yields its reason; all passing qualifies; the non-Linux, non-macOS refusal text. Linux `qualify_from` tests unchanged.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib native_worker::boundary::probe` and `--lib codex_backend` pass with the frozen contract.

## Traps

- The probe runs once per process (`OnceLock`); a probe that hangs blocks lane selection: bound every probe command at 10 s.
- `codex_tests.rs:183` drives `lane_from` with injected closures and must keep passing.
