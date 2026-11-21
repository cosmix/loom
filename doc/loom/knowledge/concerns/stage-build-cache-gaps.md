# Stage Build Cache Gaps

> Open reviewer suggestions and residuals of the stage build cache

## Open Reviewer Suggestions From the Stage Cache Plan

None of these blocked a stage; each names the file and what to change.

- **Provisioning keeps a blanket outbound allow.** `verify/criteria/confine_seatbelt.rs::provision_network_rules` allows `network-outbound` and denies Unix-socket connects under two prefixes (the stage-cache base and the session scratch base). The Docker, ssh-agent and loom daemon sockets and the operator's TCP sccache on `127.0.0.1:4226` stay reachable from a provisioning command. Allowing `(remote ip)` plus the `mDNSResponder` literal would close it. No test drives the connection: `confine_provision_tests.rs` only parses the rules; a macOS CI test that a connect to `<stage-cache>/x.sock` is denied and `h.sock` allowed is missing.
- **A stage-cache resolution error skips the before-stage checks.** `orchestrator/core/stage_preflight.rs` (`run_before_stage_checks`) makes the whole gate error when `stage_cache_for` fails, and the caller passes that error as advisory, so the checks never run.
- **A credential-bearing `GOPROXY` sits in the daemon's environment.** `daemon/server/environment.rs` keeps `GOPROXY` for the cache, so `Inherit` commands and the tmux server inherit it; filtering userinfo at capture would remove it.
- **`goproxy_has_userinfo` misreads a scheme-less entry.** It decides scheme-ness by `://` anywhere in the entry, so `user@host/a://b` passes. Testing for `://` before the first `/` would close it. The input is the operator's own environment, so the risk is low.
- **A process that already carries the stage environment resolves a nested root.** A session exports `XDG_CACHE_HOME=<root>/xdg`; `StageCache::resolve_from_process` called inside it (`verify/criteria/confine.rs`, `stage_cache/cleanup.rs`) would resolve `<root>/xdg/loom/stage-cache/<hash>`. The relay avoids it today; a guard or a comment would keep it so.
- **Duplicated helpers.** `stage_cache/roots.rs::push_unique` copies private helpers (`sandbox/control_surfaces.rs` has a generic one) and `HomeGuard` in `verify/criteria/confine_stage_cache_tests.rs` copies the one in `stage_cache/contract_tests.rs`.
- **Hard failure when `HOME` is unset.** Confined acceptance, provisioning and the other callers of `confined_stage_cache` fail without `HOME`. `Inherit` stages skip the cache, so only a confined stage under a daemon with no `HOME` is affected.
- **The worker locates its invocation directory by layout.** `native_worker/supervisor/mod.rs` uses `dir.ancestors().nth(3)`, which hard-codes `store::invocation_dir`'s depth; `LOOM_WORK_DIR` (set in `runner.rs`) or an inverse helper in `store.rs` would not.
- **`ensure()` runs twice for a Linux worker.** `native_worker/supervisor/mod.rs` ensures the root and `native_worker/boundary/linux.rs` ensures it again.
- **The no-follow removal depth is fixed.** `fs/no_follow_tree.rs::MAX_DEPTH` is 64, so an unusually deep `go-mod` or `node_modules` tree fails with `ELOOP` and the root leaks, with a warning.
- **The macOS session `PATH` is the daemon's.** `stage_cache/session.rs` exports the developer directories ahead of the host `PATH`, which is the daemon's; a daemon started with a minimal `PATH` strips tools from sessions that used to inherit the terminal's.
- **An unconfirmed cleanup assertion.** `git/cleanup/tests_stage_cache.rs` asserts an empty warnings list for a plain merged worktree; it was written without a run and needs confirming on the first test run.
- **A comment still narrates history.** `stage_cache/session_contract_tests.rs` says a grant "must no longer be granted"; a current-state sentence would do.

## A Host-Job Test Fails Only Inside the Completion Run

`host_verify::tests_wait::a_terminal_outcome_leaves_a_newer_request_the_other_waiter_wrote` (`host_verify/tests_wait.rs:375`) failed deterministically (2 of 2 runs, in a 647 s suite) inside the completion host job and passes alone in 1.2 s. The stage that met it changed neither the test nor the wait code. Cause unknown; run it alone first, and if it fails only in the full run, suspect suite timing or a shared-state interaction with a neighbouring test before touching the wait code.

## Confined Gradle Is Unsupported on macOS

Under Seatbelt a confined Gradle build fails even in-process (`GRADLE_OPTS=-Xmx1g`, `stage_cache/environment.rs::GRADLE_IN_PROCESS_OPTS`): every cache lock binds a UDP socket on the wildcard address (Gradle 8.10.2 `FileLockCommunicator.java:48`, unconditional), and the compiler and test workers connect back over loopback TCP. The profile grants no network. Allowing loopback was tried and reverted: on this repo's dev Mac (macOS 27) loopback rules worked, but on the CI runner (`macos-26-arm64`) Gradle's loopback connect was still refused and a port-specific deny had no effect, and Seatbelt cannot limit listen or receive to loopback. The Gradle probe (`stage_cache::toolchain_tests::jvm::gradle_resolves_from_the_read_only_cache_offline`) is Linux-only. Revisit with a Seatbelt network rule verified on every supported macOS version.
