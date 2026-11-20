# Brief: macos-profile (native Codex boundary on macOS)

Tier: `loom-senior-software-engineer` (opus, effort medium). Security-sensitive. Read `doc/plans/briefs/open-github-issues/common.md` first, then plan Decisions items 8-9.

## Owns

`loom/src/native_worker/boundary/seatbelt_profile.rs` (new), `seatbelt_profile_tests.rs` (new), `boundary/macos.rs` (new), `boundary/mod.rs`, `boundary/environment.rs`, `boundary/launch.rs`, `loom/src/native_worker/protocol/wire.rs`, `loom/src/verify/criteria/confine_read_roots.rs`.

Frozen, read-only: `loom/src/native_worker/boundary/seatbelt_profile_contract_tests.rs`.

Read only: `boundary/linux.rs` (172-218 argv, 321-328 binds), `boundary/mounts.rs` (122-234: which paths are writable, read-only, hidden, and the Codex home layout), `boundary/tmpdir.rs`, `native_worker/bridge.rs` (48-119), `native_worker/proxy/`, `verify/criteria/confine_seatbelt.rs` (quoting, `seatbelt_path_rules`, `sccache_socket_rule` 153-163 as the unix-socket precedent), `verify/criteria/confine_ancestors.rs::seatbelt_spellings`, `verify/criteria/confine_read_roots.rs:92-125` (`installation_roots`, macOS entries).

## Why the Linux boundary cannot run here

Bind mounts, a network namespace and a pid namespace (bubblewrap) do not exist on macOS, and macOS refuses a sandbox started inside another one, so Codex's own per-command Seatbelt cannot run inside loom's profile.

## Change

1. `seatbelt_profile.rs` (compiled on every platform): `SeatbeltBoundarySpec` and `profile()` exactly as the stage PUBLIC SURFACE states. Reuse the Seatbelt quoting from `confine_seatbelt.rs`; if it is private there, copy the quoting function into this module with a comment naming the original (both refuse control characters). Every path in both `/var` and `/private/var` spellings where it starts with either.
2. `macos.rs` (`cfg(target_os = "macos")`): `spawn` for the macOS boundary: create `<stage cache>/native/<invocation>/{tmp,codex-home/sessions}` (0700, refuse symlinks), copy only the Codex auth files and `AGENTS.md` into `codex-home` (the same file list `mounts.rs` binds), build the spec (read roots from `installation_roots` plus the loom binary's canonical `current_exe()` and the Codex binary), run `/usr/bin/sandbox-exec -p <profile> <codex app-server ...>` in a new session (`setsid` in `pre_exec`), with the environment from `environment.rs`: `TMPDIR=<tmp>`, `CODEX_HOME=<codex-home>`, `LOOM_NATIVE_BOUNDARY=<invocation id>`, the bridge proxy variables, `PATH` with the loom binary's directory first. The bridge runs inside the sandbox and reaches the proxy through its unix socket.
3. `boundary/mod.rs`: route `spawn` to `linux` or `macos` by `cfg`; other platforms keep bailing. Declare the new modules, unconditionally: `pub mod seatbelt_profile;`, `pub mod quiescence_scan;` (written by `macos-quiescence`, which does not own this file), `#[cfg(target_os = "macos")] mod macos;`, and `#[cfg(test)] mod seatbelt_profile_tests;` and `#[cfg(test)] mod quiescence_scan_tests;`.
4. `launch.rs`: identity on macOS from the spawned child's pid plus `process::identity` (`bsd_info`) and `process::boot_id` (`kern.bootsessionuuid`); no bubblewrap info fd.
5. `protocol/wire.rs:85-88`: on macOS the turn's sandbox policy is `dangerFullAccess` (loom's profile is the only boundary); Linux keeps `workspaceWrite` with `networkAccess: false` and `excludeSlashTmp: true`. Comment why.
6. `confine_read_roots.rs:35-36`: make `read_roots` available on macOS if the boundary needs it.
7. Kernel tests in `seatbelt_profile_tests.rs` named `native_macos_*`, behind the confined-commands probe: a payload under the profile cannot write `<worktree>/.git/x` or `$HOME/x`, can write `<worktree>/src/x` and `<tmp>/x`, cannot connect to `1.1.1.1:443`, can connect to the proxy socket. They skip inside the stage; the operator runs them from a plain shell.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib native_worker::` passes with the frozen contracts; clippy clean on macOS; `cargo build` still succeeds for the Linux cfg paths (`cargo check --manifest-path loom/Cargo.toml --target x86_64-unknown-linux-gnu` only if that target is installed; otherwise state it was not checked).

## Traps

- Seatbelt rule order: a later rule wins for the same operation and path, so protected denies come after the worktree allow.
- `.loom/work` inside a worktree is a symlink to shared state: hide it with denies on both the link path and its canonical target, except the map store roots (`mounts.rs:239-258`).
- `macos-quiescence` owns `quiescence.rs`, `teardown.rs` and `supervisor/mod.rs`; `macos-qualify` owns `probe.rs`. Pinned for you: the supervisor calls `boundary::spawn` as on Linux; `macos-qualify` calls `seatbelt_profile::profile` for its payload probe.
