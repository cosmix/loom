---
kind: document
of: doc/plans/PLAN-web-host-graft-followthrough.md
file: sha256:00faafab0d5413d4118ed53236b0da6a521d400f069e9ba5c82b6edecfb28356
cites:
  "doc/plans/PLAN-web-host-graft-followthrough.md#behavior-contracts": "sha256:66175e8239942de99bb0d7cd63ad5243796a339502a7920a9c995a44700e4cc9"
  "doc/plans/PLAN-web-host-graft-followthrough.md#execution-and-model-allocation": "sha256:042fbd86303b31d440163b1cacd3dbeece11ab27b26e8e4ad64218cd765f61c4"
  "doc/plans/PLAN-web-host-graft-followthrough.md#final-stages": "sha256:4beeae344710acdb98367df4aa36908278bfd18ddd4230b9acdb035127610951"
  "doc/plans/PLAN-web-host-graft-followthrough.md#outcome": "sha256:4a315a6b5f0522701beeba62b660625f1a007804c5770316b2bbacfb6eb223ca"
  "doc/plans/PLAN-web-host-graft-followthrough.md#report-decisions": "sha256:40d958a8008a6cb76a186bc6720443a55cd197476c79a9e057ad869886971071"
  "doc/plans/PLAN-web-host-graft-followthrough.md#sibling-plans-and-validation-limits": "sha256:a522cc0d271b32d5116b42b70c8ee5d3e6297778cb0e40158d85dc0bc0a95b92"
---

# Web host and Graft follow-through

## Outcome

The plan enables `loom status --web --host 0.0.0.0` with authenticated remote access while keeping `127.0.0.1` as the default and the existing port selection. [Outcome](../../../plans/PLAN-web-host-graft-followthrough.md#outcome)

It also applies Graft ideas: bounded memory batches before semantic curation, preservation of every contributing event ID, and natural-language retrieval checked by reproducible answer-recall tests. [Outcome](../../../plans/PLAN-web-host-graft-followthrough.md#outcome)

## Report decisions

Shipped graph and retrieval work is retained, cheap preparation is added without moving durable knowledge, and semantic summaries and transactional publication are deferred. [Report decisions](../../../plans/PLAN-web-host-graft-followthrough.md#report-decisions)

## Behavior contracts

The `--host` option accepts IP literals and `localhost`, and omission means IPv4 loopback. [Behavior contracts](../../../plans/PLAN-web-host-graft-followthrough.md#behavior-contracts)

Wildcard binds print the actual listening address and are never advertised as remote destinations. [Behavior contracts](../../../plans/PLAN-web-host-graft-followthrough.md#behavior-contracts)

All non-loopback listeners require a process token, and every remote HTTP and WebSocket route requires authentication. [Behavior contracts](../../../plans/PLAN-web-host-graft-followthrough.md#behavior-contracts)

`loom memory prepare` observes memory read-only, groups exact equivalents, prioritizes corrections, and retains every original event ID within a default budget of 2000 estimated tokens. [Behavior contracts](../../../plans/PLAN-web-host-graft-followthrough.md#behavior-contracts)

Preparation never performs semantic deduplication, writes knowledge or issues receipts. [Behavior contracts](../../../plans/PLAN-web-host-graft-followthrough.md#behavior-contracts)

## Execution and model allocation

Implementation runs as one standard stage with disjoint file territories and a serial CLI wiring worker, followed by integration verification and knowledge distillation. [Execution and model allocation](../../../plans/PLAN-web-host-graft-followthrough.md#execution-and-model-allocation)

Workers never spawn subagents, run git or run verification, and Codex forwards use `--effort xhigh` in foreground invocation. [Execution and model allocation](../../../plans/PLAN-web-host-graft-followthrough.md#execution-and-model-allocation)

## Sibling plans and validation limits

Sibling plans are not upstream dependencies, and the planning baseline includes one date-sensitive usage fixture that the implementation stage must repair. [Sibling plans and validation limits](../../../plans/PLAN-web-host-graft-followthrough.md#sibling-plans-and-validation-limits)

## Final stages

Integration verification exercises the built binary, reviews access, lineage and ranking, and records results in `doc/web-host-graft-verification.md`. [Final stages](../../../plans/PLAN-web-host-graft-followthrough.md#final-stages)

Knowledge distillation curates verified outcomes and publishes `doc/web-host-and-distillation.md`, finishing with strict pending and check. [Final stages](../../../plans/PLAN-web-host-graft-followthrough.md#final-stages)
