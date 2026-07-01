---
name: loom-concurrency
description: Concurrency and parallelism patterns for multi-threaded and async code. Use when implementing async/await, parallel processing, thread safety, worker pools, channels, or debugging race conditions and deadlocks across Rust (tokio), Python (asyncio), TypeScript, and Go.
triggers:
  - async
  - await
  - thread
  - mutex
  - lock
  - semaphore
  - channel
  - actor
  - parallel
  - concurrent
  - race condition
  - deadlock
  - livelock
  - atomic
  - memory ordering
  - futures
  - promises
  - tokio
  - asyncio
  - goroutine
  - spawn
  - Arc
  - Mutex
  - RwLock
  - mpsc
  - select
  - join
  - JoinSet
  - TaskGroup
  - errgroup
  - worker pool
  - backpressure
  - cancellation
  - structured concurrency
  - queue
  - synchronization
  - critical section
  - false sharing
---

# Concurrency

## Overview

Cross-language concurrency reference organized by concept, not by language. Each concept lists the shared principle plus per-language gotcha callouts for Rust (tokio), Python (asyncio), TypeScript (event loop), and Go. The hard part is never the happy-path API; it's the failure modes — data races, deadlocks, latent OOM from unbounded queues, futures dropped mid-flight by cancellation, and memory-ordering bugs that only surface under load on weakly-ordered CPUs.

## Agent Delegation

- **loom-senior-software-engineer** (Opus) — DEFAULT. Threading models, shared-state vs message-passing, race/TOCTOU analysis, lock ordering, memory ordering, distributed concurrency/consistency, saga patterns.
- **loom-software-engineer** (Sonnet) — ONLY boilerplate async handlers or unit tests following an established pattern.

## Mental Model: Pick the Right Executor

| Workload | Rust | Python | TS/JS | Go |
| --- | --- | --- | --- | --- |
| I/O-bound | tokio tasks | asyncio | async/Promises | goroutines |
| CPU-bound | rayon / `spawn_blocking` | `ProcessPoolExecutor` | Worker threads | goroutines (real parallelism) |
| Blocking call in async | `spawn_blocking` | `run_in_executor`/`to_thread` | Worker | fine (goroutines block cheaply) |

**Runtime model determines everything:**

- **Rust/tokio, Go** — genuinely multi-threaded; tasks run on a thread pool → shared mutable state needs real synchronization.
- **Python asyncio** — single OS thread; the GIL means asyncio gives you *concurrency, not parallelism*. Great for I/O, useless for CPU (one core). CPU work → `ProcessPoolExecutor`. A blocking call (sync DB driver, `time.sleep`, heavy compute) freezes the entire event loop.
- **JS/TS** — single-threaded event loop. Same rule: a long synchronous loop blocks all timers, I/O callbacks, and rendering. CPU work → Web/Worker threads. There are no data races on shared JS objects (no preemption between awaits within a microtask), but state *can* change across an `await`.

⚠ **The blocking-in-async footgun** (asyncio + JS): `await` only yields at await points. Synchronous CPU work or a blocking syscall between awaits stalls every other task. Never call a sync HTTP/DB client, `time.sleep`, or a tight compute loop directly in an async function — offload it.

## Data Races & Shared State

A data race = two threads touch the same memory, ≥1 writes, no synchronization. Rust makes most compile-errors via `Send`/`Sync`; Go/Python/JS do not.

**Principle: minimize shared mutable state.** Prefer message passing (channels) or immutable snapshots. When you must share:

```rust
// Rust: Arc<Mutex<T>> for writes, Arc<RwLock<T>> for read-heavy.
use std::sync::Arc;
use tokio::sync::Mutex;               // async Mutex; std::sync::Mutex for sync-only sections
let cache = Arc::new(Mutex::new(HashMap::<String, String>::new()));
let mut g = cache.lock().await;
g.insert(k, v);                        // guard drops at end of scope → keep scope tiny
```

Per-language gotchas:

- **Rust** — `RwLock` can starve writers under continuous readers; prefer `Mutex` unless reads vastly dominate. `std::sync::Mutex` is *not* poisoned-safe across await — use `tokio::sync::Mutex` only when the guard must cross `.await` (see below), otherwise the std mutex is faster.
- **Go** — the race detector (`go test -race`, `go run -race`) is your primary tool; run it in CI. Copying a `sync.Mutex` by value silently breaks it (`go vet` catches most). Map access is not goroutine-safe — a concurrent read+write *panics* ("concurrent map writes"); use `sync.RWMutex` or `sync.Map`.
- **Python** — even with the GIL, `x += 1` is *not* atomic (LOAD/ADD/STORE bytecodes can interleave on thread switch). Threaded code still needs `threading.Lock`. asyncio code needs `asyncio.Lock` only around state that spans an `await`.
- **JS** — no locks needed for sync sections, but "read state → await → write state" is a check-then-act race: the awaited gap lets other tasks mutate. Re-read after the await or guard with an async mutex.

## Locks, Lock Ordering & Deadlock

**Deadlock rule #1: acquire locks in a globally consistent order.** Two code paths taking A→B and B→A will eventually deadlock. Assign a total order (by address, id, or name) and always lock ascending.

```text
Thread 1: lock(A); lock(B)   ┐  both block forever
Thread 2: lock(B); lock(A)   ┘  → enforce A-before-B everywhere
```

Defenses, in order of preference:

1. **Don't hold two locks.** Restructure so critical sections don't nest.
2. **Consistent ordering** when you must nest.
3. **`try_lock` with timeout + backoff** as a last resort (detects, doesn't prevent).
4. **Keep critical sections tiny** — never do I/O, call user callbacks, or `.await` unrelated work while holding a lock.

⚠ **Holding a lock across `.await` (the async deadlock/`Send` trap):**

- **Rust** — holding `std::sync::MutexGuard` across `.await` makes the future `!Send`, so `tokio::spawn` won't compile. Fix: drop the guard before `.await`, or use `tokio::sync::Mutex` (designed to be held across await — but then two tasks awaiting the same lock on one thread can deadlock the *logical* flow). Best: copy the data out, drop the guard, then await.
- **Python/JS** — holding an `asyncio.Lock` / async mutex across an `await` that (transitively) tries to re-acquire the same lock is a self-deadlock. Async locks are not reentrant.
- **Go** — `sync.Mutex` is not reentrant either; a method holding the lock calling another method that locks the same mutex deadlocks.

**Re-entrancy:** none of these mutexes are reentrant. If a locked method calls another locked method on the same object → deadlock. Split into a public (locks) + private (assumes locked) method pair.

## Channels & Backpressure

Channels decouple producers from consumers. The single most important decision: **bounded vs unbounded.**

⚠ **Unbounded channels are latent OOM.** If producers outpace consumers, an unbounded queue grows without limit until the process is killed. Default to **bounded**; a full bounded channel applies *backpressure* — `send` blocks/awaits, throttling the producer to the consumer's rate. Only use unbounded when you can prove the producer is rate-limited by something else.

| | Bounded | Unbounded |
| --- | --- | --- |
| Rust tokio | `mpsc::channel(N)` — `send().await` | `mpsc::unbounded_channel()` — ⚠ OOM risk |
| Python | `asyncio.Queue(maxsize=N)` | `asyncio.Queue()` (maxsize=0) — ⚠ |
| Go | `make(chan T, N)` | `make(chan T)` (unbuffered = rendezvous, or `N` too small = stall) |

Per-language gotchas:

- **Rust** — `tokio::sync::mpsc::Receiver` is **not** clonable (single consumer). For fan-out to many workers, share one `Arc<Mutex<Receiver>>` or use `async_channel`/`flume` (MPMC). The old skill's "clone the receiver" pattern does not compile. Dropping all senders closes the channel; `recv()` then returns `None` — the clean shutdown signal.
- **Go** — **only the sender closes a channel**, and only once; sending on a closed channel panics, closing twice panics. Ranging over a channel (`for v := range ch`) exits when it's closed. Reading from a nil channel blocks forever (useful in `select` to disable a case). Leaked goroutines blocked on a channel nobody closes are a top Go bug.
- **Python/JS** — bound the queue and handle `QueueFull`/awaiting `put` as backpressure; an unbounded producer feeding a slow consumer is the classic memory leak.

## Structured Concurrency

**Principle: spawned tasks have a bounded lifetime tied to a scope; the scope waits for all children and propagates the first error, cancelling siblings.** This kills orphaned/leaked tasks and lost exceptions.

```python
# Python 3.11+: TaskGroup — cancels siblings on first exception, raises ExceptionGroup
async with asyncio.TaskGroup() as tg:
    tg.create_task(fetch(a))
    tg.create_task(fetch(b))
# exits only when all done; any failure cancels the rest
```

```rust
// Rust tokio: JoinSet — owns tasks, join_next() yields results as they finish;
// dropping the JoinSet aborts all remaining tasks.
let mut set = tokio::task::JoinSet::new();
for url in urls { set.spawn(fetch(url)); }
while let Some(res) = set.join_next().await {
    let _ = res?;   // JoinError on panic/abort
}
```

```go
// Go: errgroup — first non-nil error cancels the derived ctx; Wait returns it.
g, ctx := errgroup.WithContext(ctx)
for _, u := range urls {
    u := u                       // pre-1.22 loop-var capture!
    g.Go(func() error { return fetch(ctx, u) })
}
if err := g.Wait(); err != nil { return err }
```

Gotchas:

- **Python** — bare `asyncio.gather(*tasks)` does NOT cancel siblings on failure and, without `return_exceptions=True`, leaves other tasks running as it raises. `TaskGroup` (3.11+) is the correct default. A task created with `create_task` and never awaited can have its exception silently swallowed ("Task exception was never retrieved").
- **Rust** — `tokio::spawn` detaches: the task keeps running even if the handle is dropped, and its panic is isolated (only surfaced via the `JoinHandle`). Prefer `JoinSet` when you need lifetime + error propagation. `join!`/`try_join!` run futures on the *current* task (no parallelism across threads, but concurrent) and `try_join!` short-circuits on first error.
- **Go** — `sync.WaitGroup` gives you *waiting* but not error propagation or cancellation; `errgroup` adds both. Classic footgun: capturing the loop variable by reference (fixed in Go 1.22, still bites older code) — every goroutine sees the last value.
- **JS** — `Promise.all` rejects on first failure but does NOT cancel the other in-flight promises (JS promises aren't cancellable); they run to completion, their results discarded. Use `Promise.allSettled` when you need every outcome. `AbortController` is the cancellation mechanism for fetch/streams.

## Cancellation & Timeouts

**Cancellation must propagate.** A timeout at the top is useless if inner operations ignore it.

```rust
// Rust: tokio::time::timeout returns Err(Elapsed) and DROPS the future at the
// current await point → cancellation. The dropped future must be cancel-safe.
match tokio::time::timeout(Duration::from_secs(5), op()).await {
    Ok(v) => v?, Err(_) => return Err("timed out".into()),
}
```

```go
// Go: context is the cancellation currency. Thread ctx through every call;
// select on ctx.Done(). ALWAYS defer cancel() to avoid ctx leaks.
ctx, cancel := context.WithTimeout(parent, 5*time.Second)
defer cancel()
select {
case r := <-ch:   return r, nil
case <-ctx.Done(): return zero, ctx.Err()
}
```

⚠ **Cancellation safety** (Rust `select!`, Go `select`, any timeout): when a future is dropped mid-flight, work already in progress is lost. If that future had *taken* an item from a channel or half-written to a buffer, dropping it loses the item / corrupts state. In `tokio::select!`, only use futures whose partial execution is safe to discard; otherwise use `Semaphore::acquire_owned` / hold the value outside the `select!`, or use cancellation tokens. `mpsc::Receiver::recv` is cancel-safe; a hand-rolled read-modify-write across await usually is not.

- **Python** — cancellation raises `asyncio.CancelledError` inside the task at its next await. **Never swallow it** with a bare `except Exception` (in 3.8+ it derives from `BaseException`, so `except Exception` won't catch it — but `except:` will; don't). Cleanup goes in `finally`; if you must shield critical cleanup from cancellation, use `asyncio.shield`.
- **JS** — pass an `AbortSignal` to `fetch`/streams; a rejected/aborted promise still lets already-started microtasks finish. There is no forced cancellation of arbitrary async work.
- **Go** — a goroutine only stops if it *checks* `ctx.Done()`; there is no preemptive kill. Blocking on a channel without a ctx case = uncancellable = leak.

## Atomics & Memory Ordering

For a single counter/flag, an atomic beats a mutex. But **memory ordering is the trap.**

**Ordering, weakest→strongest:** `Relaxed` < `Acquire`/`Release` < `AcqRel` < `SeqCst`.

- **`Relaxed`** — atomicity only, NO ordering guarantee vs other memory. Safe for a standalone counter you only read at the end (e.g. metrics). NOT safe to publish data ("store flag, then reader sees the data") — the flag store can be reordered before the data writes.
- **`Acquire` (loads) / `Release` (stores)** — the workhorse. A `Release` store *publishes* all prior writes; an `Acquire` load that sees it *observes* those writes. This is what you want for lock-free publish/handoff (the "message passing" pattern). **Most code that isn't a plain counter wants Acquire/Release.**
- **`SeqCst`** — single global total order; simplest to reason about, slowest. Use only when multiple atomics must agree on one global order (rare). Reaching for `SeqCst` "to be safe" is a performance smell, but it's the correct default if you're unsure and can't prove Acquire/Release suffices.

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
COUNT.fetch_add(1, Ordering::Relaxed);          // pure counter: Relaxed is fine
DATA.store(payload, Ordering::Relaxed);
READY.store(true, Ordering::Release);           // publish: Release
// reader:
if READY.load(Ordering::Acquire) { use(DATA.load(Ordering::Relaxed)); }  // Acquire pairs with Release
```

Per-language:

- **Go** — `sync/atomic` operations are effectively sequentially consistent; you don't pick an ordering, but you still must use atomics (or a mutex) for *any* concurrent access — a plain `int` read/write across goroutines is a race even if "it looks atomic." Prefer `atomic.Int64` (Go 1.19+) typed wrappers over the free functions.
- **Python/JS** — no user-facing memory-ordering knobs (GIL / single event loop). JS `SharedArrayBuffer` + `Atomics` is the exception (shared memory across Workers) and there ordering matters again.

⚠ **ABA problem** (lock-free CAS): a value reads A, changes to B, back to A; a naive compare-and-swap succeeds though the world moved underneath it. Mitigate with tagged pointers / version counters (`AtomicU64` epoch) or hazard pointers. **When NOT to go lock-free:** unless you've measured lock contention as a real bottleneck, a `Mutex` is simpler, correct, and usually fast enough. Lock-free code is a maintenance and correctness liability — reserve it for proven hot paths.

⚠ **False sharing:** two unrelated atomics/fields on the same 64-byte cache line cause cores to ping-pong ownership, silently tanking throughput. Pad hot per-thread counters to a cache line (`#[repr(align(64))]` in Rust, `//go:align`/padding in Go) when profiling shows it.

## Worker Pools & Rate Limiting

Bounded concurrency = fixed workers draining a bounded channel, OR a semaphore capping in-flight tasks.

```rust
// Semaphore caps concurrency without spawning a fixed pool.
let sem = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
let tasks = urls.into_iter().map(|u| {
    let permit = sem.clone();
    tokio::spawn(async move { let _p = permit.acquire_owned().await.unwrap(); fetch(&u).await })
});
```

```python
sem = asyncio.Semaphore(max_concurrent)
async def limited(u):
    async with sem:            # acquire/release even on exception
        return await fetch(u)
await asyncio.gather(*(limited(u) for u in urls))
```

```go
// Worker pool: N goroutines range over a bounded jobs channel; close(jobs) to drain.
jobs := make(chan Job, 100)
var wg sync.WaitGroup
for i := 0; i < numWorkers; i++ {
    wg.Add(1)
    go func() { defer wg.Done(); for j := range jobs { process(j) } }()
}
// producer: for _, j := range work { jobs <- j }; close(jobs); wg.Wait()
```

- Fixed pool (channel + N workers) gives strict backpressure and bounded memory. Semaphore-per-task is simpler but each task still allocates until it acquires — bound the *spawn* rate too for huge inputs (spawn inside the semaphore, or chunk the input).
- Rate limiting (requests/sec) is orthogonal to concurrency (max in-flight): a token bucket / `time.Ticker` throttles *rate*; a semaphore caps *simultaneity*. Real clients usually need both.

## Gotchas Quick Reference

- ⚠ Unbounded channel/queue = latent OOM. Default bounded.
- ⚠ Blocking call (sync I/O, `sleep`, tight loop) in asyncio/JS freezes the whole loop.
- ⚠ Lock held across `.await`: Rust `!Send` (won't spawn) or async self-deadlock; copy-out then await.
- ⚠ Inconsistent lock order between two paths → deadlock. Total-order your locks.
- ⚠ Mutexes are non-reentrant (Rust/Go/asyncio) — nested same-lock acquire deadlocks.
- ⚠ `gather`/`Promise.all` don't cancel siblings; use `TaskGroup`/`JoinSet`/`errgroup`.
- ⚠ Cancelled/timed-out future dropped mid-flight loses in-progress work — verify cancel-safety.
- ⚠ Go: only sender closes a channel, once; send-on-closed and double-close panic; loop-var capture (<1.22).
- ⚠ Rust: `tokio::mpsc::Receiver` is single-consumer (not clonable); `spawn` detaches & isolates panics.
- ⚠ Python: `x += 1` isn't atomic even under the GIL; asyncio never gives CPU parallelism.
- ⚠ Atomics: `Relaxed` for pure counters; `Acquire`/`Release` to publish data; `SeqCst` only for cross-atomic global order.
- ⚠ Lock-free before profiling = premature; ABA and false sharing bite silently.

## Verification Checklist

Before declaring concurrent code done:

- [ ] Every channel/queue is bounded, or unboundedness is justified by an upstream rate limit
- [ ] No lock (sync mutex) is held across an `.await` / blocking call; critical sections are minimal
- [ ] Locks acquired in one consistent global order everywhere; no reentrant same-lock nesting
- [ ] Cancellation/timeout propagates to inner ops (ctx threaded / `AbortSignal` passed) and dropped futures are cancel-safe
- [ ] Spawned tasks are owned by a scope (`JoinSet`/`TaskGroup`/`errgroup`) — no detached, unawaited, or leaked tasks
- [ ] Errors from tasks propagate and cancel siblings where required (not swallowed by `gather`/`spawn`)
- [ ] Shared mutable state is guarded; atomic orderings justified (default Acquire/Release, not reflexive SeqCst/Relaxed)
- [ ] Go: `go test -race` green in CI; only the sender closes channels; every `WithCancel/Timeout` has a `defer cancel()`
- [ ] Python/JS: no blocking/CPU work on the event loop (offloaded to executor/Worker); `CancelledError` not swallowed
- [ ] Shutdown path drains queues, joins workers, and closes channels cleanly (no leaked goroutines/tasks)
- [ ] Stress-tested under load (many iterations / high concurrency), not just a single happy-path run
