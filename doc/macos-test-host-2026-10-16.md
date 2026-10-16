# macOS test-host diagnosis

No direct Input Monitoring or accessibility API caller was found in Loom or
its 339 resolved macOS dependencies. The reported WindowServer wait on
`com.apple.tcc.preflight.kTCCServiceListenEvent` remains unexplained. This work
removes demonstrated incidental desktop activity and reduces test-host load;
it does not establish the watchdog root cause.

## Audit findings

The audit covered the entire crate, Cargo.lock, and the local source trees
selected by `cargo tree --offline --locked --target aarch64-apple-darwin
-e normal,build,dev` and `cargo metadata --offline --locked --filter-platform
aarch64-apple-darwin --format-version 1`. Searches included CGEventTap,
CGEventSourceKeyState, IOHIDManager/IOHIDCheckAccess, AXIsProcessTrusted,
AXUIElement, NSEvent monitors, framework linkage, dynamic loading, and the
requested wrapper crates. None of those callers or wrapper dependencies exists.
The 14 matching dependency strings describe other frameworks or examples in
objc2 documentation; they are not input-monitoring calls.

| Reachable operation | Source and reachability |
| --- | --- |
| Terminal window AppleEvents | `loom/src/orchestrator/terminal/native/window_ops/macos.rs:14`; native session teardown and PID-less liveness checks can reach it. Three former unit tests could issue 18–21 real `osascript` probes. Unit tests now use scripted command results. |
| Terminal launch AppleEvents | `loom/src/orchestrator/terminal/emulator.rs:202` and `:220`; production Terminal/iTerm launch scripts. Command construction tests inspect arguments without executing scripts. Ghostty uses `open -na` at `:247`. |
| Desktop notification | `loom/src/orchestrator/notify.rs:38`; production human-review notification, bounded to three seconds. Unit-test builds return before execution. |
| Claude capability/auth probes | `loom/src/remote_control.rs:118`, `:136`, `:162`; CLI version/help and cached authentication, not desktop control. Tests use missing paths, disabled mode, or pure values. Version/help production probes remain unbounded; no evidence connects them to ListenEvent. |
| Keychain credential lookup | `loom/src/quota/credentials.rs:23`, `:56`; daemon quota polling invokes `security find-generic-password`. Credential tests inject lookup results. Keychain access is separate from Input Monitoring. |
| Terminal keyboard input | crossterm's Unix backend reads a terminal file descriptor. Loom callers are `commands/config/tui/mod.rs:107` and `status/ui/tui/app.rs:246`; no global event monitor. |
| Native framework links | Dependencies use Foundation/CoreFoundation, Security for TLS certificate verification, and SystemConfiguration for proxy discovery. Loom links `sandbox` at `daemon/inherited/kernel.rs:8`. No ApplicationServices/CoreGraphics/IOKit input linkage was found. |

objc2 is reached through ctrlc → dispatch2/block2 for dispatch semaphores and
Unix signal handling. Its CoreFoundation dynamic lookup reads OS version data.
The optional Security framework `job_bless` feature is not enabled.

The source audit cannot identify APIs inside external terminal applications,
Claude, or macOS services. A TCC requesting/responsible-process attribution is
still needed to identify the ListenEvent requester. No security settings,
signatures, TCC state, system services, or permission databases were changed.

## Changes

- Window unit tests use a thread-local command seam with deterministic parsing,
  escaping, fallback, and success assertions. Calling the real backend in a
  unit-test build panics (`window_ops.rs:51`). Production behavior is unchanged.
- Census defaults to 32 physical files while sending over 2 MiB of repeated
  path input through real Git attribute parsing (`context/census/tests.rs:306`).
  The original 3,000-file stress test remains explicitly ignored at `:332`.
- Doctrine fixtures call the doctrine installer directly, avoiding eight full
  hook/skill/index installations (`assets/tests/doctrine.rs:11`). Existing
  backup, content, and symlink assertions remain.
- The impostor-repository regression owns its ancestor `.git` and nested cwd
  instead of deleting shared OS-temp `.git` and `.loom/work` paths. HOME guards
  in init/trust tests restore the previous value during panic unwinding.
- Two quota parser cases run in process. Successful exchanges, EPIPE, deadline,
  chatty-reader, descendant-retirement, confinement, executable-byte mutation,
  and zero-test controls continue to exercise their real production paths.
- A stale-socket RPC fixture now shares the existing bind-without-listen helper
  at `daemon/test_support.rs:3`. A concurrent fork cannot inherit a listener
  that accepts a connection; the real `NotListening` assertion remains.
- The bounded launcher owns one short `/var/tmp` tree. Harness HOME, provider
  defaults, caches, and ordinary TempDirs live inside it; trusted confinement
  and short socket fixtures use owned siblings. The outer Cargo build retains
  its normal cache. A fixture-local read grant lets the confined compiler test
  read the installed toolchain through the existing confinement policy.
- Cargo defaults to four build jobs and four test workers. The launcher records
  logs, exits, `/usr/bin/time -l`, sampled process-tree metrics, and cleanup
  evidence. Doctests run separately with one worker. See [testing.md](testing.md).
- The launcher preserves an explicitly requested strict sandbox flag. The helper
  unit fixture now asserts the actual default skip or exact strict-mode panic
  without environment mutation; the production helper remains unchanged.

The token checkpoint fixtures already use owned temporary roots. Their
size-limit and durable-write assertions remain: removing fsync or reducing data
below the tested threshold would conceal behavior. The asset fixtures pass
explicit temporary destinations; no real `~/.codex` or `~/.loom` write was
found in the investigated paths. Launcher HOME isolation also covers ambient
lookups. This is source/environment evidence, not a system-wide filesystem trace.

The subprocess inventory found 435 explicit Command constructors, including
305 in test-named files. This is an inventory, not proof of every child's
lifetime. Real Git, shell hooks, sandboxes, tmux, and lifecycle regressions remain.
The aggregate watchdog bounds their run; existing per-test guards remain
necessary because sampling can miss a child that detaches between observations.
Direct Cargo invocation supplies the concurrency default but does not own the
aggregate temp tree; its static integration scratch-home fixture still outlives
the harness. The launcher removes that leftover after process retirement.

## Verification and metrics

The first bounded invocation completed all 47 targets, exit 101: 8,507 passed,
2 failed, 7 ignored. Library time was 137.82 s; total `/usr/bin/time -l` wall
time was 262.18 s. It exposed the stale-socket fixture and obsolete window-module
maintainability entry described above. Both are corrected; this failed run is
retained as pre-fix evidence, not a final-source pass. Its observed process-tree
peak was 77 threads and sampled writes were 2,965,942,272 bytes. No owned
process survived and the aggregate temporary root was removed.

The earlier successful invocation completed all 47 targets: 8,509 reported
passes, zero failed, seven ignored, exit 0; the library took 138.48 s. Its doctests
passed three executed and three compile-only examples with seven ignores in
3.96 s. Formatting (1.78 s), Clippy (11.71 s), and warnings-denied rustdoc (4.84 s)
also passed. These runs preceded the last optional-flag handling edit in the
launcher, so a fresh complete invocation was required.

The fresh strict attempt set `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`: **8,502 passed,
7 failed, 7 ignored** across all 47 targets, exit 101, 223.35 s wall time
(223.586008042 s supervised). Six failures deliberately refused existing
Linux-only runtime guards on this Mac; the seventh exposed the helper fixture's
unconditional skip expectation. The repaired fixture passed its strict targeted
regression, then the whole native suite ran with the optional strict flag absent.
The strict attempt peaked at 77 sampled threads and 2,205,704,192 sampled written
bytes, with no survivors and its root removed. The real macOS sibling read/write/
network canary and offline Cargo cache test passed in strict mode as well.

The latest native invocation completed all 47 targets: **8,509 reported passes,
zero failed, seven explicit ignores**, exit 0. The library contributed 7,814
passes and two ignores in 122.97 s. The pass count includes the six Linux-only
runtime returns listed in the [verification continuation](astra-verification-2026-10-16.md#strict-attempt-and-runtime-exclusions);
their Linux bodies did not execute. Main and all 45 integration targets
completed. No owned processes survived; the root was removed.

| Metric | Earlier passing run | Latest native run |
| --- | ---: | ---: |
| `/usr/bin/time -l` wall time | 252.62 s | 220.07 s |
| Supervisor elapsed, including cleanup | 252.90 s | 220.115204584 s |
| Peak process-tree threads, sampled | 60 | 77 |
| Process-tree disk bytes written, sampled | 2,748,194,816 (2.75 GB) | 1,233,932,288 (1.23 GB) |
| `/usr/bin/time -l` maximum resident set size | 4,258,578,432 bytes | 1,983,922,176 bytes |

The latest time output also reports 178.64 s user and 159.60 s system time.
Sampling observed 1,696 process identities over 861 samples, with no missing
usage samples, sampling errors, cleanup signals, or survivors. Thread and written-
byte samples are process-tree lower bounds; cache and process-tree differences
prevent a direct causal comparison between these runs.

The seven explicit all-target ignores comprise six existing terminal/orchestrator
cases and the user-authorized 3,000-file census stress case. The default 32-file
case still exercises real >2 MiB Git stdin. Latest doctests registered 13 examples:
**three executed and three compile-only passed, seven existing ignores**, exit 0,
3.28 s wall time (3.371828709 s supervised), with no survivors and root removed.
Latest formatting (1.82 s), warnings-denied Clippy (9.38 s), and warnings-denied
rustdoc (4.43 s) exited 0; these are supervisor times. Formatting initially
returned 1 solely for wrapping the new fixture's expected panic string; that
failure is retained. The full and doctest runs precede this whitespace-only
wrap. Independent review confirmed unchanged implementation by reconstructing
the tested helper hash; `format-equivalence.json` retains that proof.

Earlier Python parsing, exclusion-shell syntax, and the watchdog's controlled
success/timeout exercise passed; its child tree had no survivors. The exclusion
script's actions were never executed. The latest knowledge sync/check both
exited 0; check reported zero issues and 335 review notes, the same totals as the
earlier run. These checks do not establish corpus-wide semantic freshness.

Exact launcher commands from the repository root:

```bash
env -u LOOM_TEST_REQUIRE_SANDBOX_FREE LOOM_E2E_REQUIRE_TMUX=1 /usr/bin/python3 scripts/run-rust-tests.py --test-threads 4 --wall-seconds 1200 --idle-seconds 120
env -u LOOM_TEST_REQUIRE_SANDBOX_FREE /usr/bin/python3 scripts/run-rust-tests.py --doc --test-threads 1 --wall-seconds 300 --idle-seconds 120
```

The launcher executes Cargo from `loom/`, using the installed toolchain,
`/usr/bin/time -l`, `--no-fail-fast --color never`, the all-target runner override,
and explicit four-worker or one-worker libtest arguments. The doctest invocation
includes `--doc --workspace --all-features`. Static checks from `loom/`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
```

Complete logs and exit/cleanup metadata remain in `loom/target/test-runs/`:

- `20261016-174559-17u35o4v`: initial bounded pre-fix failure.
- `20261016-175226-x4kq92ve`, `20261016-175643-tzz5l5e4`, `final-checks-20261016`:
  earlier passing all-target, doctest, and static runs, dependency tree, Cargo
  metadata, and 47-target accounting.
- `20261016-184548-kojx3fvc`: failed strict whole-suite attempt.
- `20261016-185107-1i2x6xjr`: latest complete native run, source diff, and verified
  wrapper exports (`LOOM_E2E_REQUIRE_TMUX=1`; optional strict flag absent).
- `20261016-185513-3lguk83x`: latest complete doctests.
- `main-final-checks-20261016`: strict-helper regression, failed pre-wrap
  formatting, passing static gates, format-equivalence proof, and knowledge
  sync/check results.

No repeated flake stress loop or new dashboard/installer suite was run.
Independent source and evidence review preserved the resolver Completed
assertion, Go successful execution and semantic rejection, and real
confinement/retirement controls, and approved the strict-helper repair.

The incident baseline supplied by the operator is approximately 96 threads and
8.6 GB written by the test binary over 42 minutes, plus 2.1 GB written by rustc
over 16 minutes. It is not a completed-suite measurement. New process-tree
samples include compilers and children and are lower bounds; they must not be
presented as an exact, directly comparable reduction.

## Remaining suspects and manual action

1. Executable-startup/security-service contention has the strongest incident
   evidence: syspolicyd CPU use, pre-entry launch stalls, and dead shell children.
   The causal connection to TCC/WindowServer is unproven.
2. Filesystem pressure is demonstrated by reported writes and fixture setup.
   Reduced fixtures and concurrency address avoidable load; Spotlight and backup
   involvement does not establish causality.
3. Incidental AppleEvents were reachable from unit tests and are now isolated.
   AppleEvents alone do not explain a ListenEvent permission request.
4. Direct Loom input monitoring is unsupported by the source/dependency audit.

The optional `sh scripts/exclude-test-output-macos.sh` creates the target
Spotlight opt-out marker and requests Time Machine exclusions for build output
and the stable test-temp parent. **It has not been run.** It uses no sudo and
does not change global Spotlight, Gatekeeper, or TCC settings. If freezes recur,
retain the incident report and TCC responsible-process attribution for an OS
diagnosis; do not infer the requester from the presence of Cargo alone.

## Commits

The changes developed on `fix/macos-test-host-load` are included on local `main`
by fast-forward through `cdf26982`, with normal hooks and no push. The landed
host fixes and the additional strict-helper fixture repair are covered by the
latest recorded runs; unrelated working-tree files remain unchanged.

| Commit | Change |
| --- | --- |
| `19441ba4` | Isolate terminal window probes and remove obsolete file-size debt. |
| `bff898bd` | Reduce fixture filesystem/process work and contain temporary state. |
| `9c6067b5` | Share the bind-only stale-socket fixture. |
| `0b635718` | Bound full-suite execution, isolate homes/fixtures, cap concurrency, and provide exclusion tooling. |
| `9fba8088` | Make the probe helper fixture assert its selected strict/default policy; production behavior is unchanged. |

The latest full-run snapshot records `cdf26982` plus the reviewed helper test
diff, before the documented formatting-only wrap. It does not claim byte
identity after that wrap. Normal earlier commit hooks passed all eight
maintainability checks and warnings-denied rustdoc where triggered. The unrelated
`doc/RETRIEVAL-AB-METHOD.md` remains untouched.
