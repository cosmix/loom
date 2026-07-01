---
name: loom-background-jobs
description: Background job processing patterns including job queues, scheduled jobs, worker pools, retry strategies, and delivery guarantees. Use when implementing async processing, ETL pipelines, ML training jobs, cron schedules, or dead letter queues.
triggers:
  - async processing
  - job queue
  - job queues
  - workers
  - task queue
  - task queues
  - async tasks
  - delayed jobs
  - recurring jobs
  - scheduled tasks
  - ETL pipelines
  - data processing
  - ML training jobs
  - Celery
  - Bull
  - BullMQ
  - Sidekiq
  - Resque
  - SQS
  - cron jobs
  - retry logic
  - dead letter queues
  - DLQ
  - at-least-once delivery
  - exactly-once delivery
  - visibility timeout
  - job monitoring
  - worker management
---

# Background Jobs

## Overview

Reliable async task execution: enqueue work, process it in workers decoupled from the request cycle, and survive crashes/retries without corrupting state. This file covers job queues, retries/backoff, DLQs, scheduling, worker pools, and delivery guarantees.

For pub/sub, event sourcing, CQRS, sagas, and streaming brokers (Kafka/Pulsar), see **loom-event-driven** — don't reimplement those here.

## The two invariants everything hangs off

1. **At-least-once is the default. Design every handler to be idempotent.** Redis-backed queues (Sidekiq, BullMQ, Celery+Redis), SQS standard, and any queue that retries on crash *will* deliver a job more than once. "Exactly-once delivery" does not exist over an unreliable network; the achievable goal is exactly-once *effect* = at-least-once delivery + idempotent handler.
2. **Ack after success, never before.** The job must stay owned by the worker until the side effect is durably committed. Ack-then-process = at-most-once = silent data loss on crash. Process-then-ack = at-least-once = duplicates you dedup away. Always choose the latter.

### Idempotency: the non-negotiable pattern

Derive a stable key from the job's *business identity* (not a random UUID per enqueue), and gate the side effect on it.

```python
def handle(job):
    key = job["idempotency_key"]          # e.g. f"charge:{order_id}"
    if store.setnx(f"done:{key}", "1", ex=7*86400):   # first time → claim
        do_side_effect(job)               # charge card, send email, insert row
    # else: already done → no-op, safe to return success
```

- ⚠ **Claim *before* the effect, but only mark "done" after it commits** — if you set the flag first and then crash, the retry sees "done" and skips a side effect that never happened. Best: idempotent write at the sink (unique constraint / upsert / conditional put) rather than a separate flag. The DB unique index is the most reliable dedup store.
- The processed-marker TTL must exceed max retry window + queue retention, or a late redelivery re-runs the job.
- Idempotency covers **duplicate delivery**; it does not cover **concurrent** duplicate execution (two workers at once). For that you need a lock or an atomic conditional write — see visibility-timeout races below.

## Delivery guarantees & broker selection

| Broker / lib | Model | Reliability caveat | Use when |
| --- | --- | --- | --- |
| **SQS standard** | at-least-once, unordered | Visibility timeout only; no ordering | Cloud-native, want managed durability + native DLQ (redrive) |
| **SQS FIFO** | exactly-once *delivery* within 5-min dedup window, ordered per group | 300 msg/s (3000 batched) cap; ordering serializes a group | Ordering/dedup matters more than throughput |
| **Sidekiq (OSS)** | at-least-once | Basic fetch (`BRPOP`) **loses in-flight jobs if the worker is killed** — job left Redis when fetched. Pro `super_fetch` (RPOPLPUSH) recovers them | Ruby, Redis already present, jobs are idempotent |
| **BullMQ (Node)** | at-least-once | Uses a `lockDuration`; a stalled job (lock expired) is re-added — long jobs get double-run unless they renew the lock | Node, Redis, need rate limiting/flows |
| **Celery + Redis** | at-least-once | **`visibility_timeout` (default 3600s)**: a task running longer than it is redelivered to another worker → concurrent double-run. No true broker ack | Python, small/medium scale |
| **Celery + RabbitMQ** | at-least-once | Real broker acks; `task_acks_late` needed for crash-safety | Python, want a durable AMQP broker |
| **Kafka / Pulsar** | at-least-once (log) | Consumer-group rebalance re-delivers; offset commit = ack | High-throughput streaming → see loom-event-driven |

**Rule of thumb:** for durability without running infra, prefer a managed broker (SQS) or a real AMQP/log broker. Redis-list queues are fast and simple but the queue lives in one Redis instance — plan for `AOF`/replication and accept the crash-window caveats above.

## Visibility timeout / lock races (the #1 duplicate-execution bug)

A worker leases a job for `T` (SQS visibility timeout / Celery `visibility_timeout` / BullMQ `lockDuration`). If processing exceeds `T`, the queue assumes the worker died and hands the *same job to a second worker* — now it runs twice **concurrently**, and idempotency-by-marker won't save a non-atomic side effect.

Mitigations, in order of preference:

1. **Keep handlers well under `T`.** Size the timeout to p99 duration × safety factor.
2. **Heartbeat / extend the lease** for legitimately long jobs: SQS `ChangeMessageVisibility`, BullMQ auto-renews while the processor runs (older Bull needs care), Celery — split the work or raise `visibility_timeout` (but that also delays recovery of genuinely-dead jobs).
3. **Make the sink atomic** (conditional put / `UPDATE ... WHERE status='pending'`) so a concurrent second run is a no-op.

```text
❌ visibility_timeout = 60s, job takes 90s  → guaranteed double-processing
✅ heartbeat every 30s, or checkpoint & keep each unit < timeout
```

## Retries, backoff, and retry storms

**Exponential backoff with FULL JITTER.** Fixed or un-jittered backoff synchronizes all failed jobs to retry at the same instant → a thundering herd that re-topples a recovering dependency.

```text
delay = random_between(0, min(cap, base * 2 ** attempt))   # full jitter (AWS)
```

- **Full jitter** `rand(0, backoff)` beats "equal jitter" and beats no jitter for spreading load — use it by default.
- **Cap the delay** (e.g. 10 min) and **cap attempts**. Unbounded retries = zombie jobs clogging the queue forever.
- **Classify errors before retrying.** Retry only *transient* faults (timeouts, 429, 503, connection reset). Do **not** retry deterministic failures (validation error, 400, `NotFound`) — they'll fail identically N times, waste capacity, and delay the DLQ. Fail fast to DLQ instead.
- **Circuit breaker** in front of a flapping dependency: after M consecutive failures, open the breaker and fast-fail (or pause the queue) for a cooldown so you stop hammering it and stop burning retry budget. See loom-error-handling for breaker internals.
- **Retry budget:** total attempts × concurrency must not exceed the downstream's capacity, or retries themselves become the outage.

### Framework retry config (canonical)

```python
# Celery — autoretry with jittered backoff + late acks
class Base(Task):
    autoretry_for = (ConnectionError, TimeoutError)   # transient ONLY
    retry_backoff = True        # exponential
    retry_backoff_max = 600     # cap 10 min
    retry_jitter = True         # ON (default) — keep it
    retry_kwargs = {"max_retries": 5}

app.conf.update(task_acks_late=True, task_reject_on_worker_lost=True)
```

```typescript
// BullMQ — exponential backoff, bounded attempts, retention
new Queue("email", { defaultJobOptions: {
  attempts: 5,
  backoff: { type: "exponential", delay: 2000 }, // BullMQ jitters internally
  removeOnComplete: 1000,     // cap completed set or Redis grows unbounded
  removeOnFail: 5000,
}});
```

```ruby
# Sidekiq — default 25 retries (~21 days) w/ built-in jittered backoff
sidekiq_options queue: :default, retry: 10, dead: true
sidekiq_retries_exhausted { |msg, ex| DeadJobNotifier.notify(msg, ex) }
```

## Dead letter queues & poison pills

A **poison pill** is a job that fails every attempt (bad data, unhandled shape). Without a ceiling it retries forever and can head-of-line-block the queue.

**Policy:** max attempts → move to DLQ → **alert** → keep the payload + error + attempt count → provide **redrive** tooling to replay after a fix.

- SQS: set `RedrivePolicy` with `maxReceiveCount`; DLQ is a normal queue you can redrive from (console "start DLQ redrive" or re-enqueue).
- Give the **DLQ its own retention and monitoring** — a silently-filling DLQ is an outage you haven't noticed. Alert on `DLQ depth > 0`.
- **Redrive after the fix, not blindly** — replaying poison into the same broken handler just refills the DLQ. Fix root cause, then redrive in controlled batches.
- Store enough context to reproduce: original payload, failing worker version, stack, first/last-failed timestamps.

```typescript
// Minimal DLQ hand-off (BullMQ): route exhausted jobs, keep forensics
worker.on("failed", async (job, err) => {
  if (job.attemptsMade >= job.opts.attempts!) {
    await dlq.add("dead", {
      original: job.data, error: err.message,
      attempts: job.attemptsMade, failedAt: Date.now(),
    });
    metrics.increment("dlq.parked", { queue: job.queueName });
  }
});
```

## Idempotent enqueue (producer-side dedup)

Duplicates start at the *producer* too: a retried HTTP request or an at-least-once upstream enqueues the same job twice. Dedup on enqueue with a deterministic job id.

- BullMQ / Bull: pass `jobId` — a job with an existing id is ignored while present.
- SQS FIFO: `MessageDeduplicationId` (5-minute dedup window) or content-based dedup.
- Celery: no native enqueue dedup — use a `SETNX` guard or a unique DB row keyed on the business id.

⚠ `jobId` dedup only holds while the job is **still in the queue/known set**. Once completed and evicted (see `removeOnComplete`), the same id can be enqueued again — so producer dedup complements, never replaces, handler idempotency.

## Claim-check: don't put big payloads on the queue

Store the large blob (file, image, dataset row batch) in object storage / DB and enqueue only a **reference + integrity hash**. Queues (SQS 256KB limit, Redis memory) are for coordination, not bulk data.

```json
{ "job": "transcode", "s3_key": "uploads/abc.mov", "sha256": "…", "size": 734003200 }
```

Benefits: small fast messages, no broker bloat, payload survives independently of retries, and re-delivery re-reads the source of truth rather than a stale copy.

## Scheduled / cron jobs

Distinct from queues: a **scheduler** decides *when*, then enqueues a normal job. Celery Beat, BullMQ repeatable jobs, Sidekiq-cron, or a DB-backed scheduler.

```python
app.conf.beat_schedule = {
    "nightly-report": {"task": "reports.daily", "schedule": crontab(hour=9, minute=0)},
    "health": {"task": "ops.health", "schedule": 300.0},  # every 5 min
}
```

```typescript
await queue.add("cleanup", {}, { repeat: { pattern: "0 * * * *" }, jobId: "cleanup-hourly" });
```

### Scheduling gotchas (each has bitten someone in prod)

- **Overlap / double-fire.** Beat, repeatable jobs, or a cron on *N replicas* will each fire the tick → the same run enqueued N times, or the previous run still executing when the next starts. **Prevent with a leader lock**: `SET lock:job NX PX <ttl>` (SETNX) around the run; only the holder proceeds. Run exactly one Beat process, or use a distributed lock for the scheduler itself.

  ```python
  if redis.set(f"cron:{name}", worker_id, nx=True, px=ttl_ms):
      try: run()
      finally: redis.delete(f"cron:{name}")   # or let TTL expire; TTL > max runtime
  ```

- **Always schedule in UTC.** Local-time cron double-runs (fall-back) or skips (spring-forward) an hour on DST transitions; a job at `02:30` local may run twice or never. Store/evaluate schedules in UTC and convert for display only.
- **Missed runs after downtime.** If the scheduler was down at fire time, the tick is *gone* — most schedulers do **not** back-fill. Decide per job: **catch-up** (run once for the missed window — good for reports/aggregations that must not skip a period) vs **skip** (only run the latest — good for "sync current state" jobs where stale runs are useless). Don't naively replay every missed tick, or a 6-hour outage triggers 72 five-minute jobs at once.
- **Cron drift & long ticks.** If the job takes longer than the interval, ticks pile up. Guard with overlap prevention (above) or a "skip if previous still running" flag.
- **Timezone of `crontab(hour=…)`** follows `CELERY_TIMEZONE`/app tz — verify it; a silently-local Beat is a classic 1am/2am incident.

## Worker pools, concurrency & sizing

- **CPU-bound** (image/video, ML): concurrency ≈ number of cores; more than that just thrashes context switches. Use **processes** (Celery `--concurrency`, separate workers), not threads, to sidestep the GIL in Python.
- **IO-bound** (HTTP calls, DB): concurrency can far exceed cores; use async/greenlets (Celery `-P gevent/eventlet`, Node's single-loop concurrency) and size against the *downstream's* connection/rate limit, not CPU.
- **Recycle workers** to bound leaks: Celery `worker_max_tasks_per_child`, `worker_max_memory_per_child`.
- **`prefetch_multiplier`:** high prefetch starves other workers of queued jobs and holds them past a crash. For **long/uneven** tasks set `worker_prefetch_multiplier=1` (fair dispatch); for many **short** tasks a higher prefetch cuts round-trips.
- **Separate queues per workload class.** One slow bulk queue on the same worker as latency-sensitive jobs = head-of-line blocking. Isolate `email`, `images`, `compute` onto dedicated queues + workers so a backlog in one never starves another. Route with `task_routes` (Celery) / distinct `Queue`s (BullMQ).

## Priority & fairness (starvation)

Priority queues let critical jobs jump ahead — but **strict priority starves low-priority work** if high-priority never drains. Mitigate with weighted/round-robin consumption or **aging** (bump a job's priority the longer it waits). A tenant flooding a shared queue also starves others → **per-tenant queues** or a fair scheduler that caps any one tenant's share of worker capacity.

## Graceful shutdown (SIGTERM draining)

A deploy/scale-down sends `SIGTERM`. If you exit immediately, in-flight jobs are killed mid-run → at-least-once saves you *only if* the job wasn't acked yet. Drain instead:

```typescript
process.on("SIGTERM", async () => {
  await Promise.all(workers.map(w => w.pause()));      // stop pulling new jobs
  await Promise.all(workers.map(w => w.close()));      // wait for in-flight to finish
  process.exit(0);
});
```

- Give the orchestrator a **grace period ≥ longest job** (K8s `terminationGracePeriodSeconds`, else SIGKILL truncates the drain). For jobs longer than any sane grace period, rely on **checkpointing** (below) so a kill just resumes.
- Celery warm shutdown = one `TERM` (finish current, stop taking new); a second `TERM`/`QUIT` = cold (interrupt). Configure the deployment to send one and wait.
- With late acks + drain, a killed job is simply redelivered — which again requires an idempotent handler. Everything routes back to invariant #1.

## Long-running jobs: checkpoint & resume

Any job that can exceed the visibility timeout or a deploy grace period must be **resumable**, not restart-from-zero. Checkpoint progress durably; on retry, resume from the last checkpoint.

```python
@app.task(bind=True, acks_late=True)
def train(self, run_id, resume_from=None):
    state = load_ckpt(resume_from) if resume_from else fresh_state()
    for epoch in range(state.epoch, TOTAL):
        step(state)
        if epoch % CKPT_EVERY == 0:
            save_ckpt(run_id, epoch, state)     # durable → survives crash/redeliver
            self.update_state(state="PROGRESS", meta={"epoch": epoch})
    return finalize(state)
```

- Checkpoint to durable storage (S3/DB), not local disk a rescheduled worker can't see.
- Make each checkpoint write idempotent/versioned so a duplicated final step doesn't double-commit results.
- Break a huge job into **many small enqueued units** (per-batch jobs) when possible — smaller units mean cheaper retries, better parallelism, and no visibility-timeout fights. Fan-out then aggregate (see loom-event-driven for saga/chord orchestration).

## Backpressure & queue-depth monitoring

The queue depth is your leading indicator. **Monitor it and act on it** — a queue growing faster than it drains is an unbounded-latency outage in slow motion.

- **Alert on:** queue depth (waiting), **oldest-message age** (the truest "am I keeping up?" signal), processing latency p95/p99, error rate, DLQ depth > 0, active worker count.
- **Backpressure at the producer:** when depth exceeds a threshold, shed load, reject/429 new work, or slow enqueue — don't let an unbounded queue defer the failure into a memory/retention blowup.
- **Autoscale workers on depth or age**, not CPU alone (a queue can back up while workers idle on IO).
- Cap retained completed/failed sets (`removeOnComplete`/`removeOnFail`, SQS retention) so bookkeeping doesn't exhaust the broker.

```text
Golden signals for a queue:
  waiting_depth · oldest_age · in_flight · completed/min · failed/min · dlq_depth · p99_latency
```

## Framework quick reference

**Celery (Python)** — crash-safe defaults:

```python
app.conf.update(
    task_acks_late=True,               # ack after success (crash-safe)
    task_reject_on_worker_lost=True,   # requeue if worker dies
    worker_prefetch_multiplier=1,      # fair dispatch for long tasks
    task_time_limit=300, task_soft_time_limit=240,   # hard + catchable soft limit
    broker_transport_options={"visibility_timeout": 3600},  # ⚠ tune to > p99 runtime
)
# Composition: chain (sequential), group (parallel), chord (parallel + callback)
```

**BullMQ (Node)** — successor to `bull`; prefer it for new work. `Worker`/`Queue`/`QueueEvents` split; `FlowProducer` for parent/child DAGs; built-in rate `limiter`. Watch `lockDuration` vs job length (stalled-job re-run).

**Sidekiq (Ruby)** — at-least-once, idempotent handlers mandatory. OSS basic fetch loses in-flight jobs on hard kill; **Pro `super_fetch`** recovers them. `sidekiq_options retry:`, `dead:`; `death_handlers` for DLQ hooks.

**SQS** — managed, durable, native DLQ via `RedrivePolicy`. Standard = at-least-once/unordered; FIFO = ordered + dedup. Tune visibility timeout to job length; 256KB payload cap → claim-check.

## Anti-patterns

- **Non-idempotent handler on an at-least-once queue** → duplicate charges/emails/rows. The default and most common prod incident.
- **Acking before the work is done** → silent job loss on crash.
- **Fixed / un-jittered backoff** → synchronized retry storms.
- **Retrying non-transient errors** → wasted capacity, delayed DLQ, masked bugs.
- **Unbounded retries / no DLQ** → poison pills clog the queue forever.
- **No overlap lock on cron** → same scheduled run fires on every replica.
- **Local-time schedules** → DST double/skip.
- **Big payloads in the queue** → broker bloat, 256KB SQS rejections; use claim-check.
- **Shared queue for fast + slow work** → head-of-line blocking; separate by workload class.
- **`git add`-ing the whole `.work`** — n/a here, but analogously: don't retain unbounded completed/failed sets; they exhaust Redis.
- **Storing job state only in worker memory** → lost on restart; persist checkpoints.

## Checklists

**Before shipping a job handler:**

- [ ] Handler is idempotent — dedup key derived from business identity, side effect gated by a unique constraint or `SETNX`, marker TTL > retry window + retention
- [ ] Ack/commit happens **after** the side effect (late ack), not before
- [ ] Errors classified: transient → retry, deterministic → fail fast to DLQ
- [ ] Backoff is exponential **with full jitter**, capped delay, bounded attempts
- [ ] Max-attempts → DLQ with payload + error + attempt count; DLQ depth alerts
- [ ] Job runtime < visibility timeout / lock duration, OR heartbeat/extend, OR checkpoint & resume
- [ ] Large payloads passed by reference (claim-check), not inline
- [ ] Producer-side dedup where retries can double-enqueue (`jobId` / FIFO dedup id)

**Before shipping a scheduled job:**

- [ ] Overlap prevention: leader lock / `SET NX PX` so only one runs across replicas
- [ ] Schedule stored/evaluated in **UTC**; DST verified
- [ ] Missed-run policy chosen (catch-up vs skip) and implemented — no naive replay of every missed tick
- [ ] "Skip if previous still running" guard for jobs that can exceed their interval

**Before shipping worker infra:**

- [ ] Concurrency sized to workload (CPU≈cores/processes; IO≫cores/async), against downstream limits
- [ ] SIGTERM drains in-flight jobs; orchestrator grace period ≥ longest job
- [ ] Separate queues/workers per workload class; no fast+slow mixing
- [ ] Priority scheme can't starve low-priority (weighting/aging); per-tenant fairness if multi-tenant
- [ ] Monitoring: waiting depth, oldest-message age, in-flight, throughput, error rate, DLQ depth, p99 latency
- [ ] Backpressure/autoscale keyed on depth or age; retained job sets bounded
- [ ] Workers recycled to bound memory leaks (`max_tasks_per_child` / `max_memory_per_child`)
