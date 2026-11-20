# Brief: macos-quiescence (prove a finished macOS worker left no process)

Tier: `loom-senior-software-engineer` (opus, effort medium). Read `doc/plans/briefs/open-github-issues/common.md` first, then plan Decisions item 9.

## Owns

`loom/src/native_worker/boundary/quiescence_scan.rs` (new), `quiescence_scan_tests.rs` (new), `loom/src/native_worker/boundary/quiescence.rs`, `loom/src/process/macos_procs.rs` (new), `loom/src/process/mod.rs`, `loom/src/native_worker/supervisor/teardown.rs`, `loom/src/native_worker/supervisor/mod.rs`.

Frozen, read-only: `loom/src/native_worker/boundary/quiescence_scan_contract_tests.rs`.

`boundary/mod.rs` belongs to `macos-profile`, which declares `pub mod quiescence_scan;` and `#[cfg(test)] mod quiescence_scan_tests;` for you; do not edit it.

## Facts (verified)

- `verify_quiescent` (`quiescence.rs:34-42`) compares the boot id, then scans `/proc/<pid>/ns/pid` (`verify/criteria/pid_namespace.rs:10-60`); off Linux it returns `Unverified("pid namespaces need Linux")` (:44-47), so ownership never releases and the stage gates stay refused. `kill_namespace` (76-119) uses pidfds (Linux).
- `identity_for` (:7-19) already works on macOS (`process/identity.rs:121-135`, `process/boot_id.rs:54-100`).
- Teardown gates: `supervisor/teardown.rs:47-55` and `supervisor/mod.rs:341` run the namespace kill and scratch retention on Linux only. `codex-lane-lifetime` changed `supervisor/mod.rs` earlier (startup deadline); keep away from its code.
- The macOS worker runs in its own session (`setsid`) with `LOOM_NATIVE_BOUNDARY=<invocation id>` in its environment (pinned with `macos-profile`).

## Change

1. `quiescence_scan.rs` (every platform): `ProcEntry`, `BoundaryMark`, `survivors()` exactly as the stage PUBLIC SURFACE states (match sid, pgid or marker, then add every descendant by ppid; sorted, deduplicated).
2. `process/macos_procs.rs` (`cfg(target_os = "macos")`): read the process table with `sysctl` `KERN_PROC_ALL` (`kinfo_proc`: pid, ppid, pgid, `e_sid` through `getsid`), and the environment marker per pid with `KERN_PROCARGS2` (skip pids it cannot read; a process it cannot read is not treated as quiescent proof: count it as unknown only when its sid or pgid matches). Use the `libc` crate already in the tree; every `unsafe` block carries a SAFETY comment.
3. `quiescence.rs`: on macOS `verify_quiescent` compares the boot id, then `survivors(scan, mark)`; empty -> quiescent; otherwise kill the process group (`killpg` SIGTERM, then SIGKILL after 5 s, treating EPERM from a group of zombies as empty, as `pressure/codex_run.rs:83-95` does), rescan until empty or 30 s, then `Unverified` naming the pids.
4. `teardown.rs`, `supervisor/mod.rs:341`: run the macOS kill and scan where Linux runs the namespace kill; scratch retention keyed on the invocation id instead of the pid-namespace inode on macOS.

## Tests

`quiescence_scan_tests.rs`: descendants of a matched pid, an unrelated process, duplicates. On macOS a kernel test named `native_macos_quiescence_*` spawns `sh -c 'sleep 30 & sleep 30'` in a new session with the marker and proves the scan finds both and the kill empties it (runs unconfined; skip when `sysctl` reading is denied).

## Done

`cargo test --manifest-path loom/Cargo.toml --lib native_worker::boundary::` and `--lib process::` pass with the frozen contract; clippy clean.

## Traps

- A process that calls `setsid` and rewrites its environment escapes the scan (Decisions item 9). Say so in the module docs; it cannot leave the profile's write limits.
- Never signal a pid outside the matched set; re-check the boot id before killing.
