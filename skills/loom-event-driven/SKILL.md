---
name: loom-event-driven
description: Event-driven architecture patterns including message queues, pub/sub, event sourcing, CQRS, and sagas. Use for async messaging, distributed transactions, event stores, domain/integration events, data streaming, choreography/orchestration, delivery guarantees, or integrating with Kafka, RabbitMQ, Pulsar, SQS/SNS, or NATS.
triggers:
  - event
  - message
  - messaging
  - pub/sub
  - pubsub
  - publish/subscribe
  - kafka
  - rabbitmq
  - sqs
  - sns
  - nats
  - pulsar
  - event sourcing
  - CQRS
  - saga
  - choreography
  - orchestration
  - outbox
  - event store
  - domain event
  - integration event
  - message queue
  - message broker
  - event bus
  - data streaming
  - stream processing
  - event-driven
  - exactly-once
  - idempotent consumer
---

# Event-Driven Architecture

## Overview

Patterns for decoupling services via events instead of synchronous calls: message queues, pub/sub, event sourcing, CQRS, sagas, and streaming. Scope of THIS skill = the architecture and its distributed-systems traps (delivery semantics, ordering, outbox, schema evolution). For job-queue *mechanics* — worker pools, scheduling/cron, retry/backoff internals, generic DLQ plumbing — see `loom-background-jobs`; cross-reference rather than duplicate.

**The one law that governs everything below:** networked delivery is *at-least-once*; therefore **every consumer must be idempotent.** Read the Delivery Semantics section first — most EDA bugs are a violation of it.

Code samples are TypeScript for concreteness; the patterns are language-agnostic. Assume `crypto.randomUUID()`, a broker client, and a datastore are in scope.

---

## Delivery Semantics — "Exactly-once" Is (Mostly) a Lie

Two-Generals: across an unreliable network you cannot guarantee a message is delivered *exactly* once. You get to pick a *failure mode*:

| Semantic | Mechanism | Failure mode | Use when |
| --- | --- | --- | --- |
| At-most-once | fire-and-forget, ack before processing | **loses** messages on crash | metrics/telemetry where loss is OK |
| At-least-once | ack *after* processing, redeliver on no-ack | **duplicates** on retry/redelivery | **default for everything that matters** |
| "Exactly-once" | at-least-once transport **+ idempotent consumer + dedup** | none observable | any state-changing handler |

**Exactly-once is an *effect* you engineer at the consumer, not a transport guarantee you buy.** The real recipe:

1. At-least-once delivery (durable broker, ack after commit).
2. Idempotent consumer: processing the same message twice == processing it once.
3. A dedup/idempotency store keyed by a stable message id or business key.

⚠️ **Kafka "exactly-once semantics (EOS)" is real but scoped to Kafka.** Idempotent producer + transactions give exactly-once for **read-topic → process → write-topic-and-offsets** *inside Kafka*. It does **not** cover external side effects — a DB write, an email, a charge. The moment a handler touches the outside world, you are back to at-least-once and must be idempotent. Never tell a stakeholder Kafka gives you exactly-once for a payment.

### Idempotent consumer (the load-bearing pattern)

```typescript
// Dedup by stable id (producer-assigned messageId or a business key).
// Store the result so a duplicate returns the SAME answer, not a re-execution.
async function handleIdempotent<T>(messageId: string, work: () => Promise<T>): Promise<T> {
  const key = `dedup:${messageId}`;
  const cached = await redis.get(key);
  if (cached) return JSON.parse(cached) as T; // already processed → no-op replay

  const result = await work();                 // side effects happen here
  // SETNX + TTL: window must exceed max redelivery lag (broker retention + retry horizon)
  await redis.set(key, JSON.stringify(result), "EX", 24 * 3600, "NX");
  return result;
}
```

**Gotchas.**

- ⚠ **Dedup window vs. redelivery horizon.** If the TTL expires before the broker could still redeliver (retention + max retry delay + DLQ replay), a late duplicate re-executes. Size the window to the *worst-case* redelivery age, not the happy path.
- ⚠ **The "check then act" race.** Two workers both miss the key and both execute. Either make the side effect itself idempotent (DB `INSERT ... ON CONFLICT DO NOTHING` on a unique business key — the strongest option, atomic with the write), or gate with a lock. A Redis dedup key is best-effort unless it shares the transaction with the side effect.
- ⚠ **Prefer a natural idempotency key** (orderId, paymentIntentId) over the broker's message id. Redelivery may carry a *new* transport id for the same business fact, and a producer retry can emit two transport messages for one intent.

---

## The Dual-Write Problem & the Transactional Outbox

The single most common EDA correctness bug: **updating the database and publishing an event as two separate operations.**

```typescript
// ❌ BROKEN: two systems, no atomicity
await db.orders.insert(order);         // commits
await broker.publish("order.created"); // crash here → DB has order, world never hears about it
```

Reorder it and you get the opposite: publish succeeds, DB write fails, phantom event for an order that doesn't exist. There is **no ordering of these two lines that is safe** — you cannot atomically commit across a DB and a broker without distributed transactions (which brokers don't support and you don't want; see 2PC below).

**Fix: Transactional Outbox.** Write the event into an `outbox` table **in the same local DB transaction** as the state change. A separate *relay* reads the outbox and publishes, marking rows sent. One atomic commit; publishing becomes at-least-once (relay may crash after publish, before marking) → consumers idempotent.

```typescript
// Producer: one transaction, two writes to the SAME db
await db.tx(async (t) => {
  await t.orders.insert(order);
  await t.outbox.insert({
    id: crypto.randomUUID(),
    aggregate_id: order.id,
    type: "order.created",
    payload: JSON.stringify(order),
    created_at: new Date(),
    published_at: null,           // relay flips this
  });
});
```

**Two ways to run the relay:**

- **Polling publisher** — `SELECT ... WHERE published_at IS NULL ORDER BY created_at` (add `FOR UPDATE SKIP LOCKED` so multiple relays don't double-publish), publish, set `published_at`. Simple, portable; adds latency and DB load.
- **CDC / log-tailing (Debezium, etc.)** — a connector tails the DB WAL/binlog and streams outbox inserts to the broker. No polling, near-real-time, no query load — but operational weight (Kafka Connect) and DB-specific. Debezium has a dedicated **Outbox Event Router**.

**Gotchas.**

- ⚠ Outbox is **at-least-once**, never exactly-once — relay can publish then die before marking sent. Non-negotiable: idempotent consumers.
- ⚠ **Ordering:** to preserve per-aggregate order, publish with the aggregate id as the partition key and have the relay process a given aggregate's rows in `created_at`/sequence order.
- ⚠ **Outbox bloat:** prune `published_at IS NOT NULL` rows on a schedule; an unbounded outbox degrades the polling query.
- **Inbox pattern** is the mirror on the consumer: record processed message ids in an inbox table inside the same transaction as the write — atomic dedup without an external store.

---

## Ordering — Only Per-Partition/Per-Key, Never Global

**Global total order across a topic does not scale and is not offered.** Kafka guarantees order **only within a single partition**; RabbitMQ only within a single queue with a single consumer; SQS FIFO only within a `MessageGroupId`.

**Consequences you must design around:**

- **Choose the partition key = the entity whose events must stay ordered** (orderId, userId, accountId). All events for one entity land on one partition → ordered. Cross-entity order is *not* guaranteed and you must not depend on it.
- **Hot-partition skew.** A skewed key ("region=US", or one whale tenant) piles most traffic on one partition → that consumer lags while others idle. Pick a high-cardinality, evenly-distributed key; composite keys or salting for known-hot entities.
- **Consumer parallelism is capped by partition count** per consumer group. 6 partitions → at most 6 useful consumers. Repartitioning later is disruptive (rehashes keys, breaks in-flight order). Over-provision partitions modestly up front.
- **Retries reorder.** Reprocessing message N after N+1 already went breaks order. Kafka's idempotent producer preserves send order with up to `max.in.flight.requests.per.connection=5`; *without* idempotence, `>1` in-flight can reorder on retry — set it to 1 or enable idempotence.
- ⚠ If handlers must be ordered per key, **do not fan a single partition out to a worker pool** — that reintroduces reordering. Parallelize across partitions/keys, serialize within a key.

---

## Fat vs. Thin Events (the coupling / staleness trade-off)

How much state does an event carry? Two poles:

| | Thin / notification | Fat / event-carried state transfer |
| --- | --- | --- |
| Payload | ids only (`{orderId}`) | full snapshot (`{orderId, items, total, status,...}`) |
| Consumer action | **call back / refetch** source | read straight from the event |
| Coupling | temporal (source must be up *now*) + API coupling | schema coupling to the event shape |
| Autonomy | low | high (consumer needs nothing else) |
| PII/size | small | larger; **spreads PII** to every subscriber |

⚠ **The thin-event stale-read race:** consumer gets `order.updated {id}`, refetches the order — but events are delivered async and can arrive *before* the source's own read replica is consistent, or after a *newer* update. You can read a **stale or wrong version**. Mitigations: include a **version/sequence number** in the thin event and refetch-then-check `version >=`, or make the source's read strongly consistent for this path, or just send a fat event.

**Default to event-carried state transfer** (fat) for integration events between services — it removes temporal coupling and the refetch storm. Use thin events when payloads are huge, PII-sensitive, or consumers legitimately need the freshest value at handling time. Always version either way.

---

## Broker Selection

| Broker | Model | Ordering | Delivery | Retention/replay | Reach for it when |
| --- | --- | --- | --- | --- | --- |
| **Kafka** | partitioned log, pull | per-partition | at-least-once; EOS *within Kafka* | long, offset-based replay | high-throughput streaming, event sourcing, replayable log, many independent consumer groups |
| **RabbitMQ** | queues + exchanges, push | per-queue (single consumer) | at-least-once | consumed msgs gone (no replay) | complex routing (topic/headers/fanout), per-message TTL, RPC, priority queues |
| **Pulsar** | log, segmented storage | per-partition/key-shared | at-least-once; effectively-once dedup | tiered (offload to S3), long | Kafka-like + multi-tenancy, geo-replication, unified queue+stream, decoupled compute/storage |
| **SQS + SNS** | managed queue (+ fanout) | FIFO: per-MessageGroupId; Standard: none | at-least-once (Std); FIFO exactly-once *in-queue* | up to 14 days; no arbitrary replay | AWS-native, zero-ops, native DLQ redrive; SNS/EventBridge fan-out |
| **NATS JetStream** | log/stream, pull | per-subject-sequence | at-least-once (+ msg-id dedup window) | limits/interest/workqueue, replay | low-latency, lightweight ops, edge/IoT, request-reply + streams |
| **Redis Streams** | log | per-stream | at-least-once (consumer groups + PEL) | capped (MAXLEN) | already-have-Redis, modest scale, simple durable queue |

**Selection heuristics.**

- Need **replay / rebuild projections / event sourcing** → log-based (Kafka/Pulsar). Queue brokers discard consumed messages.
- Need **rich routing / per-message priority / TTL** → RabbitMQ.
- **On AWS and don't want to run brokers** → SQS/SNS/EventBridge; native DLQ redrive is a real ergonomics win.
- Extreme throughput or multi-consumer-group replay → Kafka. Kafka is overkill (and heavy ops) for a simple work queue — don't Kafka a to-do list.
- ⚠ Don't conflate SNS (fanout pub/sub, no per-subscriber durability by itself) with SQS (durable queue). The durable fanout pattern is **SNS → SQS per consumer**.

---

## Pub/Sub — Kafka (canonical producer/consumer)

```typescript
import { Kafka, logLevel } from "kafkajs";

const kafka = new Kafka({ clientId: "order-service", brokers: ["localhost:9092"] });

// Producer: idempotent → no duplicates from producer-side retries, preserves order.
const producer = kafka.producer({ idempotent: true, maxInFlightRequests: 5 });
await producer.connect();

await producer.send({
  topic: "orders",
  messages: [{
    key: order.id,                                    // partition key = ordering key
    value: JSON.stringify({ type: "order.created", data: order, v: 1 }),
    headers: { "event-type": "order.created", "correlation-id": corrId },
  }],
});

// Consumer: one group per logically-distinct subscriber; each group gets all messages.
const consumer = kafka.consumer({ groupId: "inventory-service" });
await consumer.subscribe({ topic: "orders", fromBeginning: false });
await consumer.run({
  eachMessage: async ({ message }) => {
    const evt = JSON.parse(message.value!.toString());
    await handleIdempotent(message.key!.toString() + ":" + evt.v, () => react(evt));
    // throwing here does NOT auto-DLQ in Kafka — offset isn't committed, msg redelivers.
    // Bound retries yourself, then produce to an error topic (see Poison Pills).
  },
});
```

**Kafka gotchas.**

- ⚠ **A crashing handler blocks the partition.** No auto-DLQ: the offset isn't committed, so the same message redelivers forever (poison pill) and everything behind it stalls. You must implement bounded-retry-then-error-topic.
- **Consumer group == subscription.** Two services that both need every event use two *different* group ids. Two instances that should *share* the load use the *same* group id (partitions split among them).
- **`fromBeginning`** only matters the first time a group has no committed offset; afterwards it resumes from the committed offset.
- Manual offset commit after successful processing = at-least-once. Auto-commit before processing = at-most-once (silent loss on crash).

**RabbitMQ** (routing-first): declare queue with `x-dead-letter-exchange` + `x-message-ttl`, publish `persistent: true`, `prefetch(n)` for backpressure, `ack` after success / `nack(requeue=false)` to route to the DLX. `nack(requeue=true)` in a tight loop is a poison-pill amplifier — track a retry-count header and DLX after N.

**NATS JetStream** (lightweight, durable): create a stream (`subjects: ["events.*"]`, retention `limits`), a **durable** pull consumer with `ack_policy: explicit` and `max_deliver: N` (built-in redelivery cap), publish with `Nats-Msg-Id` for the server-side dedup window. `msg.ack()` / `msg.nak()` / `msg.term()` (term = don't redeliver, straight to poison handling).

---

## Poison Pills & Dead-Letter Policy

A **poison pill** is a message that *always* fails — malformed payload, unparseable schema, a referenced entity that will never exist. In-band infinite retry blocks the partition/queue behind it and can pin CPU. Policy:

1. **Bounded retries with backoff** (exponential + jitter), retry count carried in a header/attribute.
2. On exhaustion, **park it in a DLQ** (never drop silently) with metadata: original topic/queue, error, stack, retry count, first-seen time, correlation id.
3. **Alert** on DLQ arrivals and on DLQ size/age crossing a threshold — a growing DLQ is an incident, not a metric.
4. **Replay tooling**: inspect, edit/fix, selectively redrive, and purge. A DLQ you can't replay from is a graveyard.

**Broker specifics.**

- **SQS:** native `RedrivePolicy` (`maxReceiveCount` → DLQ) and console/SDK **redrive back** to source. Prefer it over hand-rolling.
- **Kafka:** no native DLQ. Kafka Connect sinks have `errors.deadletterqueue.topic.name`; app consumers publish failures to an `<topic>.DLT` error topic yourself.
- **RabbitMQ:** DLX + a retry queue with per-message TTL that dead-letters *back* to the work queue implements delayed retry.

⚠ **Retry vs. DLQ classification:** only retry *transient* failures (timeouts, 5xx, lock contention). A deserialization error or a 4xx/validation failure is deterministic — retrying wastes time and delays the DLQ; fast-fail those straight to the DLQ. Distinguish retryable from non-retryable in the handler.

See `loom-background-jobs` for generic retry/backoff and worker-pool mechanics.

---

## Message Queues (work distribution)

Competing-consumers: N workers pull from one queue, each message handled once (per successful ack). Contrast with pub/sub (every subscriber group gets every message).

```typescript
// SQS competing-consumers loop. Delete = ack; not-deleting = redelivery after visibility timeout.
while (running) {
  const { Messages = [] } = await sqs.receiveMessage({
    QueueUrl: url, MaxNumberOfMessages: 10, WaitTimeSeconds: 20,        // long-poll
    AttributeNames: ["ApproximateReceiveCount"],
  });
  await Promise.all(Messages.map(async (m) => {
    try {
      await handleIdempotent(m.MessageId!, () => process(JSON.parse(m.Body!)));
      await sqs.deleteMessage({ QueueUrl: url, ReceiptHandle: m.ReceiptHandle! });
    } catch (e) {
      // Do NOT delete → SQS redelivers after visibility timeout; RedrivePolicy routes to DLQ
      // once ApproximateReceiveCount > maxReceiveCount. No manual DLQ code needed.
    }
  }));
}
```

**Gotchas.**

- ⚠ **Visibility timeout must exceed worst-case processing time**, or SQS redelivers a message you're still working on → duplicate processing. For long jobs, extend visibility with `ChangeMessageVisibility` heartbeats.
- ⚠ **SQS Standard is at-least-once AND unordered.** `ApproximateReceiveCount` is *approximate*. FIFO gives ordering per `MessageGroupId` + in-queue dedup via `MessageDeduplicationId`, at lower throughput.
- **Long polling (`WaitTimeSeconds=20`)** — always set it; short polling burns API calls and money and returns empty on sparsely-populated queues.
- **`prefetch`/batch size** is your backpressure knob: too high and a slow consumer hoards messages it can't process before the visibility timeout.

---

## Event Sourcing

Store **the sequence of state-changing events** as the source of truth; derive current state by replaying them. Not for every domain — reach for it when you need a full audit trail, temporal ("what did it look like at T?") queries, or event-driven integration by construction. It's overkill for CRUD.

```typescript
// Append with OPTIMISTIC CONCURRENCY: expected version guards against lost updates.
// A UNIQUE(aggregate_id, version) constraint is the real enforcement — the SELECT is advisory.
async function append(events: DomainEvent[]): Promise<void> {
  await db.tx(async (t) => {
    for (const e of events) {
      const { max } = await t.one(
        "SELECT COALESCE(MAX(version),0) AS max FROM events WHERE aggregate_id=$1", [e.aggregateId]);
      if (e.version !== max + 1) throw new ConcurrencyError(`expected ${max + 1}, got ${e.version}`);
      await t.none(
        `INSERT INTO events(id,aggregate_id,type,version,data,metadata,ts)
         VALUES($1,$2,$3,$4,$5,$6,$7)`,                 // UNIQUE(aggregate_id,version) catches races
        [e.id, e.aggregateId, e.type, e.version, e.data, e.metadata, e.ts]);
    }
    // ⚠ Do NOT publish to the broker here (dual-write). Write to an OUTBOX in this same tx,
    // OR let a projector/relay tail the events table (which IS your outbox in ES).
  });
}
```

Aggregate replay is a fold: `apply(command)` validates invariants and emits an event; `when(event)` mutates in-memory state; `loadFromHistory(events)` replays `when` over the stream to rebuild. Uncommitted events are appended, then cleared after a successful `append`.

### Event sourcing traps

- ⚠ **Events are immutable facts. Never edit or delete an event.** A mistake is corrected by appending a *compensating* event (`OrderCorrected`), not by mutating history. Editing history silently corrupts every projection that already consumed it.
- **Rebuild cost is O(stream length).** Long-lived aggregates get slow to load → **snapshots**: persist state every N events, load latest snapshot + events after it. Snapshots are a cache, never the source of truth (you must be able to delete all snapshots and rebuild).
- **Schema evolution via upcasting**, never rewriting stored events. On read, transform old event versions forward to the current shape (`v1 → v2 → v3`). Old versions live in the log forever, so upcasters live forever too.
- **Projections are eventually consistent.** The write side commits before read models catch up — **don't read-your-own-write from a projection** right after a command. Return the new version/state from the command, or subscribe/poll for the version to appear.
- ⚠ **Immutable log vs. GDPR / right-to-erasure.** You can't delete a person's events without breaking the chain. **Crypto-shredding:** encrypt PII with a per-subject key held outside the log; to "erase", delete the *key* — the ciphertext in the events becomes permanently unreadable while the log stays intact. Design this in from day one; retrofitting is brutal.
- **Not everything is an aggregate.** Cross-aggregate invariants can't be enforced in one atomic append — use a saga/process manager and accept eventual consistency, or reconsider the boundaries.

---

## CQRS (Command Query Responsibility Segregation)

Split the **write model** (commands → validate invariants → emit events) from the **read model** (denormalized projections optimized for queries). A command bus routes commands to handlers; projections subscribe to events and maintain query-shaped views.

```typescript
// Projection: subscribe to domain events, maintain a denormalized read row.
// Enrichment (join in customer/product names) happens HERE, so reads are single-fetch.
class OrderProjection {
  async onOrderCreated(e: DomainEvent) {
    const d = e.data as OrderCreated;
    await this.read.orders.upsert({
      id: e.aggregateId, status: "pending", total: d.total,
      customerName: (await this.customers.get(d.customerId)).name,   // denormalize at write-time
      version: e.version, updatedAt: e.ts,
    });
  }
  async onOrderConfirmed(e: DomainEvent) {
    // ⚠ Idempotent + ordered: guard with version so a replayed/out-of-order event can't regress state.
    await this.read.orders.updateIf(e.aggregateId, { version_lt: e.version },
      { status: "confirmed", version: e.version, updatedAt: e.ts });
  }
}
```

**Gotchas.**

- ⚠ **Read-model lag is inherent** — the write commits, the projection updates milliseconds-to-seconds later. Design UX for it: optimistic UI, return the new state from the command, or expose "processing". Never assume a query immediately reflects a just-issued command.
- **CQRS ≠ event sourcing.** You can do CQRS with plain read replicas or maintained materialized views; ES is one way to feed projections, not a prerequisite.
- **Projections are disposable and rebuildable** — that's the point. Version the projection code; to change a read model's shape, rebuild it from the event log rather than migrating in place.
- ⚠ **Idempotent, order-tolerant projection updates.** At-least-once + possible reordering means a projection handler must be safe to run twice and must not regress on a stale event — guard writes by `version`/sequence.
- Don't apply CQRS to simple CRUD; the operational cost (two models, sync, eventual consistency) only pays off for read/write asymmetry or complex domains.

---

## Sagas — Distributed Transactions Without 2PC

A business transaction spanning services can't hold one ACID transaction. A **saga** is a sequence of local transactions; if step *k* fails, run **compensating** transactions for steps *k-1…1* in reverse.

### Why not two-phase commit (2PC/XA)

- **Blocking + locks held across services** for the whole transaction → terrible availability and throughput.
- **Coordinator is a SPOF**; if it dies mid-commit, participants are stuck holding locks (in-doubt).
- Poor fit for the modern stack — most brokers and many datastores don't support XA; it doesn't scale.
- Sagas trade **atomicity for availability**: you accept a window of visible intermediate state, and converge via compensation. That trade is almost always correct for microservices.

### Choreography vs. orchestration

| | Choreography (events) | Orchestration (central coordinator) |
| --- | --- | --- |
| Control | each service reacts to events, no central brain | orchestrator issues commands, awaits replies |
| Coupling | decentralized; new step = new subscriber | coordinator knows all steps |
| Visibility | ⚠ emergent — hard to see the whole flow | explicit, easy to monitor/trace |
| Failure logic | compensation is distributed across services | compensation centralized in coordinator |
| Best for | 2–4 steps, simple flows | many steps, complex branching, need auditability |
| Risk | cyclic event dependencies, "who does what?" at scale | coordinator is another service to run |

```typescript
// Orchestrated saga: linear steps, reverse-order compensation. Persist state after EVERY step
// so a crash resumes/compensates instead of losing the transaction.
async function runSaga(steps: SagaStep[], data: SagaData, store: SagaStore) {
  const done: SagaStep[] = [];
  const inst = await store.start(data);
  for (const step of steps) {
    try {
      await step.execute(data);                 // must be idempotent (may re-run after crash)
      done.push(step);
      await store.advance(inst, step.name, data);
    } catch (err) {
      for (const s of done.reverse()) {         // compensate in reverse
        try { await s.compensate(data); }       // compensations MUST be idempotent + retryable
        catch (ce) { await store.flagCompensationFailure(inst, s.name, ce); /* alert; keep going */ }
      }
      await store.fail(inst, err);
      return;
    }
  }
  await store.complete(inst);
}
```

**Saga gotchas.**

- ⚠ **Compensation is semantic undo, not rollback.** You can't un-charge a card — you *refund*. Un-send an email — you send a correction. Design compensations as forward business actions.
- **Some steps aren't compensatable** (email sent, physical shipment). Order steps so **retriable-only** actions come *after* the last **compensatable** one — the "pivot transaction". Everything before the pivot can be undone; everything after must be driven forward with retries.
- **Compensations must be idempotent and retryable** — they run during failure handling, exactly when the network is flaky. A failed compensation is a manual-intervention incident; alert, don't swallow.
- **Persist saga state after each step.** A saga is a state machine; on crash it must resume forward or resume compensating. In-memory-only sagas lose transactions on restart.
- **Counter-compensation / semantic locks:** during a saga, a resource is in a provisional state (`PENDING`). Concurrent readers must know not to treat it as final; use a status flag rather than assuming isolation you don't have.

---

## Streaming (aggregations, joins, windows)

For stateful stream processing — windowed aggregations, stream-stream/stream-table joins — **prefer a real stream-processing engine (Kafka Streams, ksqlDB, Flink, Spark Structured Streaming) over hand-rolling in a consumer.** Hand-rolled windowing with `setTimeout` + in-memory `Map` (as older versions of this skill showed) has fatal gaps: state is lost on restart (not checkpointed), doesn't survive rebalance, no watermarks/late-data handling, and doesn't scale past one process. Roll your own only for trivial, loss-tolerant cases.

What the engines give you that a raw consumer doesn't:

- **State stores** backed by changelog topics → survive crashes/rebalances.
- **Event-time windows + watermarks** → correct results with out-of-order/late data (tumbling/hopping/session windows).
- **Repartitioning (`groupByKey`)** so aggregation keys land co-partitioned.
- **Exactly-once processing** *within the Kafka boundary* (`processing.guarantee=exactly_once_v2`).

⚠ **Event-time vs. processing-time.** Windowing on wall-clock (processing time) miscounts when events are delayed or replayed. Window on the **embedded event timestamp** and configure allowed lateness; otherwise a backfill or a lagging consumer silently corrupts aggregates.

---

## Schema Evolution & Versioning

Events, once published, are consumed by code you don't control and (in event sourcing) stored forever. Schema is a permanent contract.

- **Use a schema registry** (Confluent Schema Registry with Avro/Protobuf/JSON Schema) to enforce compatibility at publish time. Compatibility modes: **BACKWARD** (new consumer reads old events — the usual default), **FORWARD** (old consumer reads new events), **FULL** (both).
- **Only make additive, optional changes**: add fields with defaults. **Never** remove/rename a field, change a type, or repurpose semantics in place — that breaks existing consumers and stored history.
- **Tolerant reader:** consumers ignore unknown fields and tolerate missing optional ones, so producers can evolve without lock-step deploys.
- **Version explicitly** — carry a schema version in the event (`v`/`schemaVersion`). In event sourcing, **upcast** old versions on read (see traps). For breaking changes, publish a *new event type* (`OrderCreatedV2`) and run both until consumers migrate.
- ⚠ **Deploy order matters:** with BACKWARD compat, roll out **consumers before producers**; with FORWARD, producers first. Ship the wrong order and you break in prod during the deploy window.

---

## End-to-End Reference Flow (outbox + CQRS + saga)

```typescript
// 1) HTTP → command. Return 202: this is async; the resource isn't queryable yet (read-model lag).
app.post("/orders", async (req, res) => {
  await commandBus.dispatch({ type: "CreateOrder", payload: req.body,
    metadata: { userId: req.user.id, correlationId: req.headers["x-correlation-id"] } });
  res.status(202).json({ status: "accepted" });        // NOT 200 with the order body
});

// 2) Handler: aggregate emits events; events + state committed in ONE tx via outbox (no dual write).
class CreateOrderHandler {
  async handle(cmd: CreateOrderCommand) {
    const order = Order.create(crypto.randomUUID(), cmd.payload.customerId, cmd.payload.items);
    await this.store.appendWithOutbox(order.getUncommittedEvents()); // events table + outbox, atomic
  }
}
// 3) Relay/CDC publishes outbox → Kafka (at-least-once).
// 4) Projections (idempotent, version-guarded) build read models; queries hit those.
// 5) Orchestrated saga runs reserve-inventory → charge → ship → confirm, compensating on failure.
//    Every consumer above is idempotent because delivery is at-least-once.
```

---

## Best Practices (dense)

**Events**

- Past-tense facts (`OrderCreated`, not `CreateOrder`); immutable; one event = one business fact.
- Include `id`, `type`, `version`/schema-version, `timestamp` (event-time), `correlationId`, `causationId`, aggregate id/type.
- Prefer event-carried state transfer (fat) for integration events; version everything (see Fat vs. Thin).

**Correctness**

- At-least-once is the floor → **every handler idempotent** (dedup key or naturally idempotent side effect).
- Never dual-write DB + broker → **transactional outbox** (relay or CDC).
- Ordering only per partition/key → choose the key deliberately; watch hot partitions.

**Reliability**

- Bounded retries + backoff/jitter → DLQ; alert on DLQ arrivals and size/age; build replay tooling.
- Classify transient (retry) vs. deterministic (fast-fail to DLQ) failures.
- Visibility timeout / ack deadline > worst-case processing time; heartbeat-extend long jobs.

**Observability**

- Track **consumer lag** (Kafka offset lag, SQS `ApproximateAgeOfOldestMessage`) — the single most important EDA health metric; alert on sustained growth.
- Propagate `correlationId`/trace context through every hop (distributed tracing); it's the only way to reconstruct an async flow.
- Monitor processing latency, error rate, DLQ depth, redelivery counts, event-store/topic growth.

**Architecture**

- Sagas over 2PC; pick choreography (few steps) vs. orchestration (complex/auditable) deliberately.
- Event sourcing/CQRS only where audit/temporal/read-write-asymmetry justify the cost; not for CRUD.
- Don't over-broker: Kafka for streaming/replay, queue brokers for work distribution, managed (SQS/SNS) to avoid ops.

---

## Verification Checklists

**Before shipping any consumer:**

- [ ] Handler is **idempotent** — reprocessing the same message twice yields the same state (dedup key or `ON CONFLICT`/naturally-idempotent write).
- [ ] Dedup/idempotency window ≥ worst-case redelivery age (retention + retry horizon + DLQ replay).
- [ ] Failures are **classified**: transient → bounded retry+backoff; deterministic → straight to DLQ.
- [ ] There **is** a DLQ, it alerts on arrival + size/age, and you can replay from it.
- [ ] Poison pills can't block the partition/queue forever (retry cap enforced).
- [ ] Ack/delete happens **after** successful processing (at-least-once), not before.
- [ ] Visibility/ack timeout exceeds max processing time (or heartbeat-extended).
- [ ] Correlation/trace id is read from and propagated onward.

**Before shipping any producer / write path:**

- [ ] **No dual write** — DB change and event publish are atomic (outbox in same tx, or CDC).
- [ ] Partition/message key chosen so per-entity order holds; hot-partition risk assessed.
- [ ] Kafka producer `idempotent: true` (or equivalent) to avoid producer-retry duplicates.
- [ ] Event includes version/schema-version, correlation id, event-time timestamp.
- [ ] Schema change is additive + registry-compatible; deploy order (consumer-vs-producer-first) is correct.

**Event sourcing / CQRS specific:**

- [ ] Optimistic concurrency enforced by a `UNIQUE(aggregate_id, version)` constraint, not just a SELECT.
- [ ] No code path edits or deletes stored events; corrections are compensating events.
- [ ] Snapshot strategy exists for long streams; system still correct with all snapshots deleted.
- [ ] Old event versions are upcast on read; upcasters covered by tests.
- [ ] PII erasure strategy (crypto-shredding) designed in, not retrofitted.
- [ ] Projections are idempotent + version-guarded and rebuildable from the log.
- [ ] Callers don't read-your-own-write from a lagging projection.

**Saga specific:**

- [ ] Saga state persisted after every step; crash resumes forward or compensating.
- [ ] Every compensation is a semantic forward action, idempotent, and retryable.
- [ ] Non-compensatable steps ordered after the pivot (retriable-only) transaction.
- [ ] Compensation failures alert for manual intervention (not swallowed).
- [ ] Provisional resource states are visible to concurrent readers (status flags / semantic locks).
