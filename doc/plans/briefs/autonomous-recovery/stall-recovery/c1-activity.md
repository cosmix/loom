# stall-recovery C1: a thread-local record of what the current thread is waiting on (codex unit)

Lane: codex terra (`--model gpt-6.1-terra --effort xhigh`), foreground, Bash timeout 600000 ms.
Wave 1, in parallel with W2. Read `doc/plans/briefs/autonomous-recovery/common.md` first. Do
not run git or touch `.loom/`. Compile-only check: `cargo build --lib`.

## Files

Own: `src/process/activity.rs`, `src/process/activity_tests.rs`, and one line in
`src/process/mod.rs` (`pub mod activity;` beside the other module lines). Do not change
anything else in `src/process/mod.rs`: W1 hooks `run_bounded` into this module.

## Why

The scheduler stamps only its phase (`src/orchestrator/tick.rs`): plan 05's status showed "no
tick for 187 s, stuck in sync" and nothing said which operation blocked. This module is the
leaf both the process runner and the lock helpers report into, so the operation is named at
the primitive that waits, whoever calls it.

## Interface (pinned)

```rust
/// Called with the current stack (outermost first) after every enter and every exit.
pub type Recorder = Box<dyn Fn(&[String]) + 'static>;
/// Install `recorder` for the CURRENT thread, replacing any earlier one. Threads without a
/// recorder pay one thread-local lookup per enter and record nothing.
pub fn install(recorder: Recorder);
/// Remove the current thread's recorder and clear its stack.
pub fn uninstall();
/// Push `label` (control characters dropped, cut to 120 bytes on a char boundary) and return
/// a guard that pops it on drop, panics included. Each push and pop calls the recorder.
#[must_use]
pub fn enter(label: &str) -> ActivityGuard;
pub struct ActivityGuard { /* private */ }
/// The current thread's stack, outermost first.
pub fn current() -> Vec<String>;
```

Implement with `thread_local!` (`RefCell<Option<Recorder>>`, `RefCell<Vec<String>>`). A
recorder that calls `enter` itself must not panic on a re-entrant borrow: take the recorder out
of its cell while calling it and put it back after.

## Steps

1. Write the module (under 150 lines, functions under 50).
2. `activity_tests.rs` (declared `#[cfg(test)] #[path = "activity_tests.rs"] mod tests;`):
   nested enters record `["a"]`, `["a", "b"]`, `["a"]`, `[]`; a guard dropped by a panic
   (`std::panic::catch_unwind`) pops; a label with `\n` and `\x1b` is cleaned; another thread
   without a recorder records nothing.
3. `cargo build --lib`.
