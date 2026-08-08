# Sessions And Liveness

> Session identity, liveness routing, spawn-site coverage, and the blast radius of adding a session field.

## Session Identity: Backend Metadata Must Be Persisted

**Mistake:** Relying on transient session state to route kill/liveness calls after a daemon restart.

**Why:** Sessions are reconstructed from `.work/sessions/<id>.md` on daemon restart. Any field not in the session file is lost.

**Prevention:** Add `#[serde(default)]` to backend-related session fields and ensure they are set before the session is written to disk.

## Liveness: Monitor Must Route Through LivenessService

**Mistake:** Monitoring thread reads the PID from the session file and calls `kill -0 <pid>` directly.

**Prevention:** Always route session liveness through `LivenessService::is_alive(session)`. Never `kill -0` directly in the monitor.

**Fix:** `LivenessService` added in `orchestrator/liveness.rs`, wrapping `Arc<NativeBackend>`. The monitor thread holds the `LivenessService`, not a raw backend handle.

## Run-Path Coverage: All Spawn Sites Must Use the Shared Backend

**Mistake:** Wiring a session-spawning change into the main orchestrator loop but forgetting the other spawn paths: foreground mode, daemon startup, merge resolver spawner, continuation (handoff) spawner, auto-merge spawner.

**Why:** Sessions are spawned from multiple entry points beyond the main orchestrator. Each missed path drifts from the shared `Arc<NativeBackend>` the orchestrator holds.

**Prevention:** When changing session spawning, `rg` for all `spawn_session\|spawn_merge_session\|spawn_knowledge_session` call sites before considering the work done. Typically 5+ sites: orchestrator main loop, foreground spawner, merge_handler, continuation, auto_merge.

## Session Liveness: Use tracking_key, Not stage_id

**What happened:** `kill_session` and `is_session_alive` in `orchestrator/terminal/native/mod.rs` used `format!("loom-{stage_id}")` for window titles and bare `stage_id` for PID key lookups. This worked for standard stages but silently missed merge sessions, knowledge sessions, and base-conflict sessions whose spawns use prefixed tracking keys.

**Why:** Standard stages dominate the mental model; their PID key and stage_id happen to align. But `Session.tracking_key` is the canonical OS-level resource identifier — it encodes the prefix/suffix needed for non-standard session types.

**Prevention:** Any OS-resource lookup keyed on a session (window title, PID file, process name) MUST use `session.tracking_key`, not `stage_id` or `format!("loom-{stage_id}")`. Verify by running a merge-resolver or knowledge session and checking that kill/liveness correctly targets it.

**Fix:** `native/mod.rs` updated to use `session.tracking_key` in all OS lookups.

## Adding Session Fields: ~15-20 Struct Literal Breakages

**Mistake:** Adding a field to `Session` struct and expecting `cargo build` to guide you to all the breakages. Test files in `tests/` are not compiled by default and may not show breakages until `cargo test`.

**Why:** Rust requires all struct fields to be initialized in struct literals (unless `..Default::default()` spread is used). `Session` is constructed explicitly in ~15-20 locations across `src/` and `tests/`.

**Prevention:** Use `..Session::default()` spread in all struct literals. When adding fields to Session/Stage/LoomConfig, run `cargo test --all` (not just `cargo build`) to catch `tests/` breakages. Alternatively, write a context-aware patch script.

## Timing: Missing Accumulation on Exit Transitions

**Mistake:** `accumulate_attempt_time` not called on `NeedsHandoff`/`BudgetExceeded`, permanently losing execution time.
**Fix:** Call `accumulate_attempt_time` on ALL exit transitions, not just `Completed`.

## Test Environment Race Condition

**Mistake:** `test_loom_terminal_env_var_takes_precedence` uses `std::env::set_var` without `serial_test`.
**Fix:** Use `#[serial]` attribute on tests that modify environment variables.

## CORRECTION (2026-08-08): "Adding Session Fields: ~15-20 Struct Literal Breakages" Is Now Wrong

**Supersedes the "~15-20 Struct Literal Breakages" section above — treat that count as obsolete.**

Adding `Session.backend` during the tmux-backend plan broke exactly **3** struct-literal sites, not
15-20: `src/commands/handoff/create.rs:98`, and `src/commands/stage/tests/session.rs` at `:52` and
`:146`. `Session` construction has migrated almost entirely to `Session::new()` / `new_merge()` /
`new_knowledge()` plus field mutation, so bare struct literals are now rare.

**What still holds, and is the part worth keeping:** `cargo build` alone does not surface breakages in
`tests/` — use `cargo test --all-targets --no-run`.

**Meta-lesson:** a knowledge entry that carries a *count* is a decaying asset. Two stages sized their
work off this number before anyone checked it. Blast-radius numbers should be re-measured, not
inherited — and when you measure one, correct the entry in the same stage.
