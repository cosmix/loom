# Live State Pollution

> Tests wrote live state

## A Stage's Test Run Rewrote the Live State Directory (2026-09-13)

**What happened:** knowledge-bootstrap (a knowledge stage, so it runs in the main checkout) ran its own gate script with `TMPDIR=<repo>/loom/target/token-optimization-checks` and `cargo test --all-targets`. Every `TempDir` was created inside the live checkout, and `WorkDir::new(tempdir)` (`loom/src/fs/work_dir.rs:211-227`) walked up to the nearest `.git` and adopted the live `.loom/work`. Within 30 seconds the tests wrote the `terminal-backend-fallback` marker (`backend_flag_tests` in `loom/src/commands/run/mod.rs` write the literal `fell back`), flipped `[terminal] backend` to native, filed a false crash report that marked the live, still-running knowledge-bootstrap session `crashed`, and left a dispute for the fixture stage `build-api` (`daemon/server/tests/self_service_client.rs:103`). The marker then forced every later spawn onto the native lane, although both the project and the user config said `backend = "tmux"`. 290 tests failed and nothing connected the failures to live-state writes.

**Why:** three independent defects combined. (1) Tests relied on an unstated precondition: that their tempdir sits outside any repository holding a workspace. The upward walk turns a TMPDIR nested in a checkout into the live workspace. (2) Main-checkout stages get the whole repository as sandbox write scope, `.loom/` included, so nothing stopped the writes. (3) A sticky state file outranked the configuration chain. It was added with the tmux backend (e6ff6445, 2026-08-07) without any plan or spec asking for it.

**Prevention:**

- Loom stage sessions never write `.loom/` directly; stage-driven state changes go through daemon-mediated channels.
- If the configuration system can express a behavior, no marker or state file may alter that behavior. When the configured behavior cannot run, fail loudly with the cause.
- A test that resolves a workspace creates it inside its own tempdir; workspace discovery never walks above the OS temp dir.
- Detection: fixture names (`build-api`, `test-plan`, `stage-a`, `.tmpXXXXXX`) under a live `.loom/work` mean a test adopted the live workspace.
- Do not add fallback machinery a feature's spec did not ask for; a spec-less sticky marker is unrequested scope.

**Fix (2026-09-13):** the `.loom/work/terminal-backend-fallback` marker and the `.loom/work/remote_control-unsupported` marker are both removed, along with their writers, readers, and the native retry the terminal-backend marker guarded. A tmux spawn failure is now an `Err` that blocks the stage (`FailureType::InfrastructureError`); `loom run` refuses to start when the effective backend is tmux and tmux is not on PATH (`loom init`'s equivalent check stays advisory). Remote control's fast-fail path now calls `remote_control::disable_for_this_process(reason)`, an in-memory, process-lifetime flag logged to the daemon's stderr — nothing persisted, so a daemon restart tries remote control again. `WorkDir::new`'s upward walk (`walk_up`, `fs/work_dir/discovery.rs`) is now bounded at the OS temp root: it never inspects `std::env::temp_dir()` (canonicalized) or anything above it when the base path is inside it. `commands/subagents/render.rs` tests were isolated from the ambient cwd. Write-denying `.loom/**` for every stage session is still pending — design in progress.

## A Live-State Probe Also Sees the Operator's Own Sessions — That Is Not Automatically Test Pollution (2026-09-13)

**What happened:** while checking live `.loom/work` for pollution during the state-confinement gate,
fresh entries under `context/_local/<scope>/session-retrieval/*` and `telemetry/events.jsonl`
looked like a repeat of this file's core bug.

**Why:** the prompt-brief hook writes both of those on every prompt, for every session, including
the operator's own interactive ones running against the same live state directory. Their presence
alone is not evidence a test adopted the workspace.

**Prevention:** tell an operator session's writes apart from a polluting test run by the telemetry
entry's `kind` and `session_id`, not by the mere existence of the files, before calling something
test pollution.

## `cargo test` Rewrote the Real HOME and Broke a Live Daemon (2026-09-25)

**What happened:** a full `cargo test` on the development machine installed the working tree's hook scripts into `~/.claude/hooks/loom` and `~/.codex/hooks/loom`, rewrote `~/.codex/hooks.json`, and added a `hasTrustDialogAccepted` entry to `~/.claude.json` for every temp worktree it created. 3,216 of 3,879 project entries in `~/.claude.json` were `/tmp/.tmp*` leftovers. The working tree's `commit-guard.sh` differed from the running daemon's build, so the daemon's hook integrity preflight (`sandbox/config/preflight.rs` `hook_script_refusals`) refused the next spawn and blocked a live stage: "loom hook scripts in ~/.claude/hooks/loom are missing or differ from this loom build: commit-guard.sh".

**Why:** `install_loom_hooks()`, `install_codex_hooks()` and `trust_worktree` resolve `dirs::home_dir()` with no override. Tests reached them through `repair_workspace` (`commands/repair/workspace/tests.rs`, `commands/init/repair_tests.rs` via `startup_repairs`) and through in-process `loom::git::create_worktree` (`tests/integration/dependency_*.rs`). The repair tests only wrote when the real HOME's hooks were stale, so they looked clean in isolation and wrote only in a whole-binary run.

**Prevention:** a test that can reach a HOME-resolving writer runs under a scratch HOME: `#[serial]` with an env guard, `startup_repairs_isolated`, or `helpers::create_worktree_isolated` in integration tests. To detect a leak, run `HOME=$(mktemp -d) CARGO_HOME=~/.cargo RUSTUP_HOME=~/.rustup cargo test --no-fail-fast` and list the scratch HOME afterwards; it must stay empty. `tests/integration/home_isolation.rs` pins the redirect.

**Fix:** the tests above now run under a scratch HOME; `home_isolation.rs` asserts that `loom repair --fix` with `HOME` redirected installs into the scratch HOME. The existing `/tmp/.tmp*` entries in an operator's `~/.claude.json` are not removed automatically.

## Credential-Lookup Tests Read the Host Keychain and Print the Real Token (2026-10-15)

**What happened:** the `quota::credentials` tests are not hermetic on a logged-in Mac. `access_token` asks the macOS keychain (`loom/src/quota/credentials.rs:41-43`) before the temp `HOME`, so the five `access_token` tests failed and `a_token_present_in_the_credentials_file_is_returned` printed the host's live OAuth token in its `assert_eq!` output, into every captured cargo test log. Integration-verify deleted the leaked log and injected the keychain source in the tests.

**Why:** the tests stubbed `HOME` only, and the lookup has a second host-global source.

**Prevention:** a test of a credential lookup stubs every host secret source (keychain, environment, file), never only `HOME`. Treat a cargo test log captured on a macOS host as secret-bearing: delete it, never commit or quote it, and never assert with `assert_eq!` on a value that can be a host secret.

## Native window tests must not probe the desktop

**What happened:** Unit tests for nonexistent terminal windows called the real AppleScript window backend and could probe six GUI terminal applications.

**Why:** Negative window-existence smoke tests relied on the operator desktop instead of injected command outcomes. The observed AppleEvents calls do not establish a TCC Input Monitoring cause.

**Prevention:** Keep real desktop commands unreachable in unit tests; record argv and inject outputs for parser, fallback and escaping assertions. Make accidental use of the real test backend panic.

**Fix:** Isolate native window command execution behind a test-only seam while preserving production execution, deadlines and child retirement. Do not add speculative permission polling or caches when no corresponding input-monitoring caller is found.

## Impostor repository tests must own their ancestor directory

**What happened:** The impostor-Git-directory fixture wrote and removed `.git` and `.loom/work` directly under `std::env::temp_dir()`, sharing those paths with unrelated processes.

**Why:** The test used the OS temporary root itself as the repository ancestor rather than allocating an owned parent fixture.

**Prevention:** Every ancestor walk in a test must remain inside its own TempDir; never recursively delete paths beneath a shared OS root as fixture setup or teardown.

**Fix:** Allocate a TempDir parent containing the impostor `.git` and nested working directory, assert no `.loom/work` appears under that parent, and let TempDir remove it on drop.

## Isolate test homes at the harness boundary

**What happened:** Tests inherited the operator home, and a static integration TempDir deliberately survived process exit.

**Why:** Individual fixtures cannot consistently constrain ambient home and cache lookups across child commands.

**Prevention:** Use an invocation-scoped harness runner with an owned OS temporary root, private home/cache/temp paths, concrete toolchain paths, and no writable links to user caches. Keep the Cargo build driver separate so it can read its dependency cache. Track owned process identities with native start times; sampled write counters are lower bounds, not exact aggregate writes.

**Fix:** Add a bounded runner and preserve per-test child guards. Reserve a trusted sibling outside the session-writable temp directory for confinement fixtures; do not weaken confinement to move fixtures. Review moved documentation links against their new module scope before verification.

## Isolated test environments must preserve fixture contracts

**What happened:** The first runner draft set provider-home and Cargo-target overrides that could defeat HOME-only fixtures and expected local build paths; its long Darwin temporary path exceeded Unix socket limits.

**Why:** Environment isolation changes inputs used by real compiler and confinement controls, and AF_UNIX path limits include every ancestor.

**Prevention:** Clear ambient provider/output overrides, redirect HOME, and use a short owned OS temporary parent outside canonical /tmp for trusted fixtures. Keep writable test temp and trusted fixtures as siblings. Grant only the installed toolchain read root within the offline compiler fixture through its existing RUSTUP_HOME input, restoring the variable on drop.

**Fix:** Use the short /var/tmp test parent and explicit test-owned paths. Preserve production confinement checks, real subprocess lifecycle tests, and target-path assertions. Do not copy or mutate installed toolchains.

Review also caught that a proposed /private/tmp root would violate trusted-fixture constraints; use /var/tmp instead. Formatting and new CI path filters exposed existing file-size ceilings, so compact comments before the gate. Rejected editing patches changed no files; refresh exact context before retrying a patch.

The workflow topic also contained stale advice opposing job throttling. Keep the current-state replacement factual: job limits control concurrency and the watchdog controls command lifetime; both apply. The discarded advice and why it failed belong in this mistakes entry, not a historical paragraph in conventions.

## A listening socket is not a deterministic stale-socket fixture

**What happened:** The bounded full run failed the stale-daemon-socket test after connect succeeded and reading the response reached EOF.

**Why:** The fixture created a live UnixListener, ignored shutdown return status, and dropped it while other test threads could fork and inherit its descriptor. Shutdown did not establish the claimed stale state.

**Prevention:** Create a real AF_UNIX stream socket inode by binding without listening. Preserve the production NotListening assertion and real socket semantics; never accept Unreachable or an exchange error as equivalent.

**Fix:** Reuse the bind-only stale-socket helper already present in daemon server tests. Review the fixture independently, then verify fresh final sources. The first full run also exposed an obsolete maintainability baseline entry for the now-split window_ops.rs; delete that stale debt entry rather than increasing limits.

## Preserve explicit verification strictness through test isolation

**What happened:** The isolated runner cleared the optional LOOM_TEST_REQUIRE_SANDBOX_FREE flag while preserving the tmux requirement flag.

**Why:** Broad Loom environment cleanup treated a caller-requested verification policy like ambient session identity.

**Prevention:** Allowlist explicit test strictness flags separately from stage, session, credential and application-location overrides.

**Fix:** Preserve both strictness flags in generated harness environments and check the actual wrapper environment. The completed full run required tmux but did not request strict sandbox-free mode; do not claim it did.

The environment check initially tried to decode the entire combined stdout/stderr log as JSON. Preserve the complete log, select the explicit JSON evidence line, and validate its full schema/value set; unrelated runtime diagnostics are not JSON data.
