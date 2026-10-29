---
kind: overview
of: overview/optim-token/engineering
cites:
  "doc/plans/optim-token/index.md#plan-map": "sha256:c37687dbce10fe2504d3e8e49842a970f4845073429cb4f6131c7d12487664e5"
members:
  "plans/01-doctrine-truth-and-plan-guards": "sha256:db6c41f61eb5a83812cf68663cd3d6e62c59b54de563ad4c17ea487ed534e724"
  "plans/02-delivery-truth": "sha256:0ff5cbe2f22300238c8890556379a67281af9719e7d718faed20ada5b4b2858b"
  "plans/03-usage-evidence": "sha256:a0fae804e2c9872c67632dac3cc3af52fe0fbd5793c0f3960cc9fdbdd990fee4"
  "plans/04-dependency-findings": "sha256:7b08ff2c8a21f7065cddfb324f703a2e2a01b046af9cb9e9c20a93141376908f"
  "plans/05-guidance-and-gate-output": "sha256:b8e4ad07b960a092caf61a6620f40f013bdd275ed3d4fdfdb5e418f2babd5a55"
  "plans/06-sandbox-route-closure": "sha256:b9a2646b7ae6be66b4cfcf7430d29d13e7df8da966eb6f47a462a1850f0728f6"
---

# Token Optimisation Engineering Overview

## Programmes

The token optimisation programme orders six plans that make loom's signals, accounting and sandbox more truthful and smaller. [Plan map](../../../../plans/optim-token/index.md#plan-map)

## Token optimisation

Doctrine truth corrects the signal, hooks and skills and adds guards for the bootstrap warning, stage model allowlist and archive failures. [01-doctrine](../../plans/01-doctrine-truth-and-plan-guards.md#s-c8fa8623) [01-doctrine](../../plans/01-doctrine-truth-and-plan-guards.md#s-e82028dd)

Delivery truth removes the read-receipt path and records a worker-brief delivery whenever the spawn guard issues a brief. [02-delivery](../../plans/02-delivery-truth.md#s-36213ed9) [02-delivery](../../plans/02-delivery-truth.md#s-a03b9c9f)

Usage evidence fixes what `loom usage` reports and adds a signal replay harness that spends no provider tokens. [03-usage](../../plans/03-usage-evidence.md#s-56586398) [03-usage](../../plans/03-usage-evidence.md#s-96189c25)

Dependency findings let a successor stage see what its dependency changed, capped at a small byte budget. [04-findings](../../plans/04-dependency-findings.md#s-1fd3ff49) [04-findings](../../plans/04-dependency-findings.md#s-da13675b)

Guidance and gate output shortens the template and orchestration skill and changes bytes only, with no new command or hook. [05-guidance](../../plans/05-guidance-and-gate-output.md#s-12327293) [05-guidance](../../plans/05-guidance-and-gate-output.md#s-4457a301)

Sandbox route closure denies four routes by which a stage agent's Bash could escape the bubblewrap capsule. [06-sandbox](../../plans/06-sandbox-route-closure.md#s-40a752b8) [06-sandbox](../../plans/06-sandbox-route-closure.md#s-34d998dd)
