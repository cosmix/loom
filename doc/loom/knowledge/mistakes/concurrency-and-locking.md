---
---

# Concurrency And Locking

>

## File Locking: Writing to Locked Handles

**Mistake:** `fs::write()` opens a NEW handle that ignores locks held by other handles.
**Fix:** Write to the locked handle: `file.set_len(0)`, `file.seek(Start(0))`, `file.write_all()`.

## Stage-File Lost Updates: Whole-Stage Save Reverts Concurrent Writers (A-5, 2026-06-09)

**What happened:** `locked_read`/`locked_write` make individual reads/writes atomic, but load → mutate → whole-record save releases the lock between read and write. Three writer classes race on the same stage file — the orchestrator main loop, daemon IPC handlers, and agent-run CLI commands. A writer that loaded a stage minutes earlier can therefore revert status, counters, close reason, session identity, or amended verification policy written in the gap.

**Misleading signal:** patterns.md claimed a "daemon single-writer model" and "no explicit file locking." Both were false — `fs/locking.rs` exists, and the daemon, its dispute thread, and CLI agents all write stage files concurrently. "Each save is locked" hid that locked atomic saves of a STALE whole object still lose updates.

**Why:** per-operation locking serializes the WRITE but not the read→write transaction. Two transactions that both `load(); mutate_field_A_or_B(); save_whole_stage()` interleave as load-A, load-B, save-A, save-B → B's save reverts A's field.

**Prevention — use `verify::transitions::update_stage(id, work_dir, |s| …)` for every existing-stage mutation, never load + mutate + whole-record save.** It re-reads under the stages-directory lock, applies the closure, and writes in one critical section. Mutate only operation-owned fields. Run slow Git, terminal, network, and verification work outside the lock, then apply a short delta. Whole-record persistence is for actual creation only; the orchestrator loop is not exempt because daemon and CLI writers remain concurrent.

**Detection rule:** any `load_stage(); …; save_stage()` pair where the `…` can run while the daemon/dispute-thread/another-CLI is live is a lost-update candidate. Especially when `…` contains a multi-minute step (acceptance, verification, git merge) or increments a counter read from the in-memory stage (`fix_attempts += 1`, `dispute_count += 1`).

## Inbox Ledger Has Two Rows Per Request Id — a Lookup Must Take the Latest (2026-09-13)

**What happened:** the relay inbox drain appends an `applying` row when it starts processing a
request, then an outcome row when it finishes, both under the same request id. A lookup that
matches by id alone finds the `applying` row.

**Why:** the ledger is append-only and `applying` is written first, so an id-only lookup returning
the first match reads the in-progress state instead of the outcome.

**Prevention:** any reader of the inbox ledger must take the LATEST row for an id, never the first
match.

**Fix:** `fs/inbox/status.rs` uses `.rev().find(...)` — walk from the end.

## `save_stage` Creates and Never Overwrites, So a Test Mutating a Stage Must Use `update_stage` (2026-10-06)

**What happened:** two new tests loaded a stage, mutated it and called `save_stage`, which panicked with
`Stage already exists`.

**Why:** `save_stage` is an alias of `create_stage` and refuses to overwrite
(`verify/transitions/persistence.rs`).

**Prevention:** change an existing stage record in a test with `update_stage(id, dir, |stage| { ...; Ok(()) })`.

## A Session Read Inside `update_stage` Deadlocked the Daemon on Its Own flock (2026-10-30)

**What happened:** commit `d3503aa2` made `record_status_change` (`loom/src/verify/transitions/persistence.rs`) look up the stage session's type with `load_session_exact`. The daemon froze at the first host-job completion after the install: `complete_stage` (`loom/src/host_verify/apply.rs`) holds the exclusive `sessions/` directory lock across `update_stage`, and `load_session_exact` → `locked_read` takes a shared lock on that same directory. The poll thread blocked forever right after writing the stage `completed` (the log shows `host job ended` with no `host job applied`, and `stage-events.jsonl` has no completed row). `loom status`, the web dashboard and the session's `loom stage complete` waiter hung behind the held lock until `killall loom`; the card-payload stage sat unmerged for 11 minutes.

**Why:** `lock_parent_dir` (`loom/src/fs/locking.rs`) opens a new descriptor per call, and `flock` locks taken through separate open file descriptions conflict even inside one process, so a nested locked read of a directory the caller already holds exclusively never returns. The new `update_stage` tests call it with no caller-held lock. The one existing test that runs it under the sessions lock, `host_verify::tests_apply::a_passing_job_completes_the_stage_and_records_the_receipt`, hangs instead of failing, so only a full `cargo test` would have shown it, as a stuck run.

**Prevention:** what `update_stage` runs while it holds the stages lock (the modify closure and `record_status_change`) takes no `locked_*` lock of its own: a caller may already hold that directory exclusively, and a stages→sessions order inverts `complete_stage`'s sessions→stages order. A change to what runs under a lock gets a test that holds the caller's lock and asserts the call returns within a timeout, so a deadlock fails the suite instead of hanging it.

**Fix:** `record_status_change` reads the session type without the directory lock; session files are replaced by rename, so an unlocked read sees one whole version.
