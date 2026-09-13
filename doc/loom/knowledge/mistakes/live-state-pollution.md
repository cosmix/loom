# Live State Pollution

> How a stage's test run rewrote live `.loom/work` state: a TMPDIR nested in the checkout, `WorkDir::new`'s upward walk, a stage sandbox that could write `.loom/`, and a sticky marker that outranked config.

## A Stage's Test Run Rewrote the Live State Directory (2026-09-13)

**What happened:** knowledge-bootstrap (a knowledge stage, so it runs in the main checkout) ran its own gate script with `TMPDIR=<repo>/loom/target/token-optimization-checks` and `cargo test --all-targets`. Every `TempDir` was created inside the live checkout, and `WorkDir::new(tempdir)` (`loom/src/fs/work_dir.rs:211-227`) walked up to the nearest `.git` and adopted the live `.loom/work`. Within 30 seconds the tests wrote the `terminal-backend-fallback` marker (`backend_flag_tests` in `loom/src/commands/run/mod.rs` write the literal `fell back`), flipped `[terminal] backend` to native, filed a false crash report that marked the live, still-running knowledge-bootstrap session `crashed`, and left a dispute for the fixture stage `build-api` (`daemon/server/tests/self_service_client.rs:103`). The marker then forced every later spawn onto the native lane, although both the project and the user config said `backend = "tmux"`. 290 tests failed and nothing connected the failures to live-state writes.

**Why:** three independent defects combined. (1) Tests relied on an unstated precondition: that their tempdir sits outside any repository holding a workspace. The upward walk turns a TMPDIR nested in a checkout into the live workspace. (2) Main-checkout stages get the whole repository as sandbox write scope, `.loom/` included, so nothing stopped the writes. (3) A sticky state file outranked the configuration chain. It was added with the tmux backend (e6ff6445, 2026-08-07) without any plan or spec asking for it.

**Prevention:**

- Loom stage sessions never write `.loom/` directly; stage-driven state changes go through daemon-mediated channels.
- If the configuration system can express a behavior, no marker or state file may alter that behavior. When the configured behavior cannot run, fail loudly with the cause.
- A test that resolves a workspace creates it inside its own tempdir; workspace discovery never walks above the OS temp dir.
- Detection: fixture names (`build-api`, `test-plan`, `stage-a`, `.tmpXXXXXX`) under a live `.loom/work` mean a test adopted the live workspace.
- Do not add fallback machinery a feature's spec did not ask for; a spec-less sticky marker is unrequested scope.

**Fix (2026-09-13):** the `.work/terminal-backend-fallback` marker and the `.work/remote_control-unsupported` marker are both removed, along with their writers, readers, and the native retry the terminal-backend marker guarded. A tmux spawn failure is now an `Err` that blocks the stage (`FailureType::InfrastructureError`); `loom run` refuses to start when the effective backend is tmux and tmux is not on PATH (`loom init`'s equivalent check stays advisory). Remote control's fast-fail path now calls `remote_control::disable_for_this_process(reason)`, an in-memory, process-lifetime flag logged to the daemon's stderr — nothing persisted, so a daemon restart tries remote control again. `WorkDir::new`'s upward walk (`walk_up`, `fs/work_dir/discovery.rs`) is now bounded at the OS temp root: it never inspects `std::env::temp_dir()` (canonicalized) or anything above it when the base path is inside it. `commands/subagents/render.rs` tests were isolated from the ambient cwd. Write-denying `.loom/**` for every stage session is still pending — design in progress.

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
