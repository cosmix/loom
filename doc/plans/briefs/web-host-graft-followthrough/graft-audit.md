# Graft recommendation audit — 2026-09-13

The August report is a useful target architecture, but its “current Loom” baseline
is historical. Implement the remaining deterministic distillation prepass now.
Retain the shipped source graph and retrieval system; defer the larger durable
operation/semantic migration to its existing program owner.

This assessment compares `doc/REPORT-graft-inspired-loom-distillation.md` in full
with current source. It does not independently reproduce the pinned Graft
benchmarks or claim their savings for Loom. Old `hooks/` references now generally
mean `loom-hooks/`; old `.work/` references need the current `WorkDir` resolver.

## Comparison and decisions

| Report recommendation | Current implementation/evidence | Decision |
| --- | --- | --- |
| Local source wiring graph independent of model summaries | `context/extract`, `context/source_graph`, `context/graph_store`; native tree-sitter Rust/TypeScript/Python/Go extractors, conservative edges, coverage, cached snapshots | Shipped; preserve. No new Graft dependency or repeat parser spike needed for this follow-up. |
| H2 knowledge catalog and exact/lexical retrieval | `fs/knowledge/chunker.rs::chunk_sections`, `context/rank`, `rank_source`, `fuse`, `pack`; both channels reach one pack | Shipped. Do not build a second retrieval engine. |
| Cross-source ranking and bounded graph neighbors | `context/retrieve/channels.rs::rank_channels_cached`, `rank_source/expand.rs::expand_from_seeds`; exact-rung-first fusion then lexical RRF | Shipped with an intentional alternative to the report's generic graph rank. Keep exact matches authoritative within their coverage. |
| Source outline, find-all and impact tools | `loom map --outline/--find-all/--impact` return symbols, spans and explicit coverage | Shipped. Ranked retrieval still cannot prove exhaustive changes. |
| Immutable base and worktree overlays | `context/graph_store` and snapshot/reconcile lifecycle, overlay tombstones for deleted files | Shipped substantially. Avoid another shared mutable cache. |
| Briefs, pointer push, delivery deduplication, deterministic hooks | `orchestrator/signals/retrieval.rs`, `context/delivery`, `commands/hook`; current prompt already receives a bounded Knowledge Brief | Shipped substantially. No new model calls in hooks. |
| Knowledge lifecycle and evidence | `knowledge annotate`, frontmatter state/aliases/sources/verified; `catalog/evidence.rs::EvidenceCollector` reports changed or unavailable declared source evidence | Partial target. Useful today, but not per-claim source-body-hash semantic provenance. |
| Title-independent knowledge IDs and generated/human ownership regions | `chunker` derives compatibility chunk identities; no shipped stable-ID upsert/supersede/retire operation family | Defer explicit migration; preserve human prose. Do not silently relabel compatibility IDs as stable claim IDs. |
| Structured memory capture | `fs/memory/types.rs::MemoryEntry` has stable UUID, session, evidence and receipt; `spool` serializes entries and drains through existing persistence | Partial target. Keep existing capture and stage journals instead of adding another event database now. |
| Processing receipts and pending selection | `memory resolve`, `memory pending`; existing outcomes are promoted/merged/discarded/deferred | Shipped processing dispositions, not durable knowledge-operation receipts. |
| Mechanical distillation before semantic judgment | `generate_knowledge_distill_stable_prefix` still asks for `memory show --all` twice plus all-event JSON before resolving each candidate | Implement now: bounded deterministic preparation of currently unresolved candidates, with complete event lineage. |
| Canonical committed receipt, cursor publication, host-finalized knowledge operations | No canonical knowledge receipt module or bootstrap/enrich/GC CLI in current `KnowledgeCommands`; `resolve` records a journal receipt immediately | Deferred program migration. Do not describe existing receipts or best-effort archives as this guarantee. |
| Hash-cached semantic summaries, cruxes, clustering and global reduce | `context/refresh/semantic.rs::try_reconcile_semantic` calls source `ensure_snapshot`; “semantic” here names the source-graph half, not LLM synthesis | Missing as proposed. Defer until held-out recall/outcome evidence justifies cost and durable publication exists. |
| Explicit no-model bootstrap and semantic cost controls | Current `KnowledgeCommands` exposes update, replace-section, annotate, context, eval, telemetry, sync and check; bootstrap/enrich/GC from the report are absent | Do not resurrect the obsolete CLI during this feature. Use existing sync/check; design paid bootstrap separately. |
| Real evaluation and provider usage | `knowledge eval`, `knowledge telemetry`, provider accounting and execution/read receipts already exist | Preserve native-unit reporting; add focused prepass fixtures. No estimated savings claim. |
| Embeddings, LSP enrichment, SQLite, MCP transport | Optional later extensions rather than missing prerequisites | Defer until measured need. |

## The concrete remaining inefficiency

`loom/src/orchestrator/signals/cache.rs:189` asks for all stage memories, line
219 asks for another all-memory corrections sweep, and line 221 asks for all
entries in JSON. The underlying pending reader already knows which events have
receipts. A deterministic preparation command can avoid repeatedly sending the
curator settled entries and exact duplicates, while retaining every original ID.

`commands/memory/handlers/pending.rs::staged_entries` already reads sorted
journals once and merges only the current worktree's undrained spool. Reuse that
visibility boundary: do not scan sibling worktree spools or shared control state.
`pending_report` limits ordinary pending candidates to Note/Decision/Question;
Change entries remain counted separately. Preserve that policy.

The prepass should normalize metadata for comparison, group exact equivalents,
prioritize `stale-knowledge:` corrections, and produce a budgeted source-bearing
candidate page with every event ID/evidence reference and explicit omissions.
It must not decide truth, discard “procedural” records automatically, invoke a
model, mutate knowledge, or mark events processed. The curator remains the sole
semantic decision-maker and uses the existing resolve commands for every ID.

Repeated prepare/resolve cycles give bounded work without inventing persistent
watermarks. A digest identifies an observed input set; it is not an atomic
run snapshot, write-ahead log, cursor or durable receipt. Entries appearing after
one invocation are picked up on the next. Existing host finalization remains the
larger program's responsibility.

## Receipt and lineage limitations

`resolve.rs::ensure_event_exists` checks that an event exists. `resolution_details`
requires a nonempty target for promoted/merged and a reason for discarded/deferred.
It does not prove that a target section exists, that a knowledge mutation used the
event, or that knowledge and receipt were committed atomically. Each resolution
creates another journal entry, and no uniqueness transaction is enforced.

`pending_report` constructs settled IDs from every receipt regardless of outcome.
Thus **deferred is a terminal disposition for the current run**, not a retry
cursor. Changing this implicitly would break the current finish contract.
Preparation must report deferred counts distinctly while retaining that behavior.
A durable cross-run deferred backlog needs an explicit future migration.

If preparation observes multiple disagreeing dispositions for an event, it should
surface a blocking diagnostic rather than silently select one. Equivalent repeated
receipts can be diagnosed/deduplicated for presentation without rewriting history.
This detects inconsistent evidence; it does not create transactional receipt
integrity. If missing targets or obsolete knowledge are discovered, the curator
must verify current source and correct knowledge through the existing CLI.

`fs/memory/archive.rs::archive_run_state` is a best-effort archive of memory and
telemetry. Failure warns and cleanup continues. Archive paths are neither a
canonical Git receipt nor reliable cross-run provenance. Do not use them to claim
the report's durable cursor/publication guarantee.

## Existing program overlap

`doc/plans/PLAN-secure-distilled-loom-v2.md` already assigns:

- Stage 9: structured events, per-stream watermarks, stable-ID operations,
  WAL/CAS application, strict audit, canonical Git receipts and crash recovery.
- Stage 10: activation and merge/cleanup lifecycle unification.
- Stage 18: changed-hash semantic jobs, cluster identity/global reduce, explicit
  enrichment only when held-out retrieval demonstrates value.

The bounded prepass is an additive consumer of today's memory store. Future stage
9 can replace its input adapter while preserving grouping, output and lineage.
This plan does not execute or amend that sibling program. The sibling Markdown
provides stage descriptions; no sibling YAML was located by repository filename
search, so its prose is not evidence that its proposed subsystems shipped.

`PLAN-loom-state-confinement.md` explicitly says “design approved” and records
implementation on branch `state-confinement` in `.state-confinement/`. Its I1 owns
`commands/memory/handlers/{record,work_dir}.rs` and shared CLI dispatch; I2 owns
relay drain and knowledge completion. No Loom stage YAML is embedded in that
document. Preparation stays read-only and reuses existing pending visibility;
it neither supersedes the relay nor assumes resolution writes are synchronous.
The shared CLI owner must integrate additive routes after sibling changes.

## Other follow-ups to keep separate

The knowledge topic about absent source graphs records a historical degradation
gap, but current `retrieve/graph.rs` explicitly delegates never-built freshness to
`evaluate_state`. Do not blindly implement that stale concern; characterize the
current full retrieval result before changing the detached-rebuild trigger.

The retrieval knowledge topic records natural-language stopword recall gaps;
current source also contains subsequent stopword and rescue-floor work. Implement
a bounded held-out natural-language lifecycle evaluation now; add the case before
asserting that the issue remains, and change ranking only if it reproduces against
the current tree. Keep ranking changes in a separate worker from the memory
prepass so improvements can be attributed to their actual mechanism.

No parser, paid semantic synthesis, provider migration, new knowledge store,
aggregate knowledge cap, or security gateway is needed to ship this bounded step.

## Valuable ideas to retain for the next migration

Deferral here is a sequencing decision, not a rejection of these mechanisms.
The sibling program is a design document, not an executable dependency. It needs
fresh source grounding and executable stages before any of the following can run.

**Durable event-to-claim lineage: high value, implement after the recording and
completion authority is settled.** Give a claim a stable identity independent of
its heading; keep compatibility aliases for current chunk IDs. A knowledge batch
must name its input event IDs/digests, expected versions of affected claims,
decisions, source evidence and resulting claim versions. Stage the proposed
Markdown and batch manifest together, validate them, and publish with a recoverable
commit point. Advance processing watermarks only after publication succeeds.
Crash tests must cover preparation, partial file publication, receipt publication
and cursor advancement; replay must preserve human edits rather than overwrite
them. This cannot be achieved by adding a target-existence check to `resolve`.

The current prepass deliberately supplies a replaceable input adapter and retains
all original IDs, making that future migration easier. It does not counterfeit
the transaction with a digest. The scope to coordinate is existing memory
persistence, knowledge mutation and every host completion/cleanup route, which
the confinement and v2 designs both touch. Shipping an independent competing
writer in this plan would create conflicting authority over the same lifecycle.

**Typed source provenance and selective re-verification: high value.** Distinguish
current code evidence from historical incidents, user decisions and examples.
Record a source content hash and a stable symbol locator where available, with
the range only a convenience. A changed source invalidates the evidence relation
and queues the affected claim for review; it must not automatically delete or
supersede the claim. Reuse the existing source graph for reverse lookup, while
reporting unresolved edges. Acceptance must discriminate index freshness from
verified meaning: unchanged Markdown with changed source must become unverified
before it can be presented as current evidence. Preserve human notes and keep
historical knowledge accessible explicitly.

**Hash-cached semantic meaning: promising experiment, not a default yet.** The
structural graph already answers routine signatures and callers. Spend a model
call only on demonstrated gaps such as a non-obvious invariant or cross-file
rationale. Start with target-centered source windows and a file outline, then
cache the result against source/edge identity, prompt, output schema, generator,
provider/model and options. Multi-node synthesis additionally keys the complete
input set, topology and prior claim versions. A file hash alone cannot safely
cache cluster summaries. Run this only at explicit maintenance boundaries;
no hook-triggered semantic calls and no model-written source graph.

Before enabling it, compare structural retrieval against structural-plus-semantic
on independently labelled tasks, charge semantic maintenance to total usage,
and report provider counters separately from byte estimates. Retain a disabled
default if the improvement does not justify its maintenance and invalidation
cost. The hermetic evaluation in this plan supplies the first reusable baseline;
it is not itself evidence that model enrichment saves money or improves tasks.

**Cross-run deferred work: useful, but requires an explicit new contract.** Today
`deferred` settles this run. A future durable backlog needs an owner, visibility
after cleanup, a reopen action and review triggers (evidence changes or an explicit
maintenance request). Preserve existing historical receipts and migrate additively;
do not reinterpret past deferred dispositions as a suddenly failing current run.
This is why preparation reports deferrals distinctly instead of silently turning
them into an automatic retry queue.
