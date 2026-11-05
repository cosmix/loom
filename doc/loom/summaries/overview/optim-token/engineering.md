---
kind: overview
of: overview/optim-token/engineering
cites:
  "doc/plans/optim-token/index.md#plan-map": "sha256:c37687dbce10fe2504d3e8e49842a970f4845073429cb4f6131c7d12487664e5"
members:
  "plans/01-doctrine-truth-and-plan-guards": "sha256:4883703f55b8615db358dbf82a02241f597441ce9fc330c84e67c2ea47e38239"
  "plans/02-delivery-truth": "sha256:c6483a664a088d4ce1281a2670dc7b5ade436f09d6241bad3fccaf67bf97e45d"
  "plans/03-usage-evidence": "sha256:43fa8102b58a4f03a9600e5b4a053edfb9920a2ec27a7b318fb7dd650c13bff6"
  "plans/04-dependency-findings": "sha256:e8dca4ac2dbcfadd63f931d664d04f15cc1033a2d047caa3c934d7f0f2d7f961"
  "plans/05-guidance-and-gate-output": "sha256:1346ce0c412b49addfdd69920ea6734f6962eeefd622bf5302a64531cee65e3d"
  "plans/06-sandbox-route-closure": "sha256:c865a546e46147c9d50bc6a03fa2f0576f3aae787e842d831ae35e0af64d0376"
---

# Token Optimisation Engineering Overview

## Programmes

The token optimisation programme orders six plans that make loom's signals, accounting and sandbox more truthful and smaller. [Plan map](../../../../plans/optim-token/index.md#plan-map)

## Token optimisation

Doctrine truth corrects the signal, hooks and skills and adds guards for the bootstrap warning, stage model allowlist and archive failures. [01-doctrine](../../plans/01-doctrine-truth-and-plan-guards.md#s-068f9ddf) [01-doctrine](../../plans/01-doctrine-truth-and-plan-guards.md#s-682bed9f) [01-doctrine](../../plans/01-doctrine-truth-and-plan-guards.md#s-e8d99f19) [01-doctrine](../../plans/01-doctrine-truth-and-plan-guards.md#s-183d1dd7)

Delivery truth removes the read-receipt path and records a worker-brief delivery whenever the spawn guard issues a brief. [02-delivery](../../plans/02-delivery-truth.md#s-3416b984) [02-delivery](../../plans/02-delivery-truth.md#s-fc67d2e8)

Usage evidence fixes what `loom usage` reports and adds a signal replay harness that spends no provider tokens. [03-usage](../../plans/03-usage-evidence.md#s-a18f6e71) [03-usage](../../plans/03-usage-evidence.md#s-cfef112b)

Dependency findings let a successor stage see what its dependency changed, capped at a small byte budget. [04-findings](../../plans/04-dependency-findings.md#s-960e56e8) [04-findings](../../plans/04-dependency-findings.md#s-b59f82f7)

Guidance and gate output shortens the template and orchestration skill and changes bytes only, with no new command or hook. [05-guidance](../../plans/05-guidance-and-gate-output.md#s-0df7e231) [05-guidance](../../plans/05-guidance-and-gate-output.md#s-dd8f7ac8)

Sandbox route closure denies four routes by which a stage agent's Bash could escape the bubblewrap capsule. [06-sandbox](../../plans/06-sandbox-route-closure.md#s-af1897e4) [06-sandbox](../../plans/06-sandbox-route-closure.md#s-f56062b2)
