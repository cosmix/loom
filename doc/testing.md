# Run the Rust test suite

Run from a checkout with the Rust toolchain installed. On macOS, use the system
Python so the watchdog can inspect processes through libproc without launching
`ps` or another executable on each sample:

```bash
/usr/bin/python3 scripts/run-rust-tests.py
```

This runs one real `cargo test --all-targets --no-fail-fast` invocation from the
`loom/` crate directory, with an explicit libtest thread count. Cargo
selects and runs the targets; the launcher does not replay an artifact list or
filter out failures. Documentation tests are a separate invocation:

```bash
/usr/bin/python3 scripts/run-rust-tests.py --doc
```

The root `.cargo/config.toml` limits concurrent Cargo build jobs to four and
supplies `RUST_TEST_THREADS=4` for ordinary Cargo
commands. An existing environment value overrides that default; libtest's
explicit `--test-threads` option overrides its environment. The launcher defaults
to four workers for all targets and one for doctests, honors `RUST_TEST_THREADS`,
and accepts `--test-threads N` as an explicit override. Doctest mode uses
`--doc --workspace --all-features`. The libtest limit does not bound runtime worker pools or threads created by
individual tests.

## Temporary files and home directories

The launcher owns one `r-*` directory under the OS temporary location
`/var/tmp/loom-tests-<uid>/` (canonicalized on macOS). The short path accommodates
Unix-domain socket limits and keeps trusted fixtures outside the ordinary `/tmp`
write grant. The parent is private to the current user.
Every harness receives `TMPDIR`, `TMP`, and `TEMP` inside the run directory, so
`tempfile::TempDir` and `std::env::temp_dir()` use that location. Fixtures should
retain their `TempDir` guard until all files and child processes are finished.
`LOOM_TEST_ROOT` names the owned root; its `trusted/` sibling directory is outside
the `t/` subtree for fixtures that exercise write restrictions. A platform's
broader temporary-directory grants can still affect those confinement tests.

The invocation-scoped Cargo runner also places `HOME`, XDG locations,
Cargo/Rustup homes inside that root. Git's global configuration follows HOME. It clears
inherited Loom/session, Codex, Claude, Git, and Cargo target-directory overrides.
Explicit `LOOM_E2E_REQUIRE_TMUX` and `LOOM_TEST_REQUIRE_SANDBOX_FREE` controls are
preserved when supplied by the caller.
Loom, Codex, and Claude then use their defaults under the isolated HOME; a
fixture can redirect HOME again without an inherited application-home override.
Nested Cargo output follows its fixture's working directory. The doctest mode
applies the same environment to rustdoc.
Both wrappers `exec` the real executable. No global runner is installed, and
ordinary `cargo run` retains its usual environment.

The outer Cargo build uses its normal registry and target caches. Test processes
use concrete installed toolchain executables with an empty, isolated Cargo home;
the launcher does not copy registries or link writable caches into the real
home. Dependency-free nested Cargo fixtures continue to work. A fixture requiring
dependencies must supply an explicit fixture-local dependency source.

The parent removes the aggregate root after process retirement, including files
retained by process-static fixtures. If retirement fails or processes survive,
the root is preserved with cleanup evidence. Logs remain under `loom/target/test-runs/`.
This is environment isolation, not an OS filesystem sandbox: fixtures must not
write to hard-coded absolute paths, the real home, or cloud-synced directories.
Calling Cargo directly only supplies the four-thread default; use the launcher
for aggregate temporary-root ownership and isolated homes.

## Bounds, exits, and retained evidence

The default limits are 3,600 seconds total and 300 seconds without new command
output. Override them explicitly when a known workload needs more time:

```bash
/usr/bin/python3 scripts/run-rust-tests.py --wall-seconds 5400 --idle-seconds 600
```

The supervisor forks before execution, monitors native PID/start-time identities,
and retires its private process group plus observed detached descendants with
TERM followed by KILL. It also checks for surviving children after normal exit.
A process that detaches and loses its ancestry between samples can escape this
observation; each test still owns and waits for every child it starts. No global
name-based process killing occurs. SIGKILL of the supervisor or a host crash
prevents its cleanup and can leave a run directory behind.

Each retained run directory contains `cargo.log` (combined command output and
`/usr/bin/time -l` output on macOS, `-v` on Linux), `summary.json`, and bounded
toolchain-discovery logs. A `*.cleanup.json` journal records owned identities and
the root before signaling, then records remaining processes. The summary preserves
the actual command return code,
test-result totals, timeout reason, cleanup PIDs, and whether the root was removed.
Successful Cargo output reporting zero executed tests fails the launcher.
Counts aggregate libtest summaries in the combined log; nested summaries printed
inside failure diagnostics can inflate them. Inspect Cargo's target results in
the retained log when diagnosing an incomplete or failed run.

| Launcher exit | Meaning |
| --- | --- |
| Cargo's exit | Normal execution, including test/build failure |
| 98 | Cargo succeeded but no executed tests were reported |
| 124 | Wall or output-idle limit reached |
| 125 | Launcher/monitor failure or leftover processes after Cargo exit |
| 128 + signal | Launcher or command interrupted by a signal |

`peak_threads_sampled` is the largest observed sum across the command tree,
including Cargo, compiler, harness, and child processes. On macOS,
`disk_bytes_written_sampled` sums the maximum observed per-process
`ri_diskio_byteswritten` counters; Linux uses `/proc/<pid>/io` `write_bytes`.
These are sampled lower bounds, not exact totals: 250 ms sampling can miss short
processes and final writes, and kernel disk accounting differs from bytes passed
to `write()`. The supervisor's own work and processes not observed in the tree
are excluded. `/usr/bin/time` block-output counts are not byte counts. Compare
runs with the same cache warmth and selection; a stopped run is incomplete.

## Optional macOS indexing and backup exclusions

This opt-in script creates `loom/target/.metadata_never_index` and asks Time
Machine to exclude Cargo output and the stable temporary parent:

```bash
sh scripts/exclude-test-output-macos.sh
```

It accepts an explicit Cargo output directory and temporary parent as its first
and second arguments; its default temporary parent matches the launcher.
The script does not run automatically, does not invoke `sudo`,
and does not change TCC, Gatekeeper, or global Spotlight settings. The marker is
an indexing opt-out hint; this script does not verify an indexer's behavior.
