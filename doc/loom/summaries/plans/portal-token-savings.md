---
kind: document
of: doc/plans/DONE-PLAN-portal-token-savings.md
file: sha256:42e7b271ec56802cfadf69dfe26a0d7808d0b927d8f6b0040aadb9c33b7bfc5a
cites:
  "doc/plans/DONE-PLAN-portal-token-savings.md#background": "sha256:8f896688ac141d019795d4d6740b057e5d6f71614afc5197f13b0e4feb8c6559"
  "doc/plans/DONE-PLAN-portal-token-savings.md#execution-diagram": "sha256:5314858c631e70ed70bdcf17972d1d37ab39a947a7fd71f83e29342a9a9fd8c2"
  "doc/plans/DONE-PLAN-portal-token-savings.md#goals": "sha256:d8010f37b2c249f4cb63712653e649ff598c9ffa2218b2a61531bbdf22c6b28b"
  "doc/plans/DONE-PLAN-portal-token-savings.md#implementers": "sha256:18b0edea4a6cc030b3866333e05ec978dc2ebcd43056ded0fb111ec3dd476359"
  "doc/plans/DONE-PLAN-portal-token-savings.md#later-decisions-outside-this-plan": "sha256:b8fe1e1729fbcf291f54476b07f57378874a1ebcd615022549a1110dce3276ae"
  "doc/plans/DONE-PLAN-portal-token-savings.md#non-goals": "sha256:de9439d9ce9390ab6669a19b6d240be202a0c66c1501d21a000c5eeb7b613511"
  "doc/plans/DONE-PLAN-portal-token-savings.md#rejected": "sha256:fd104064ce5668eb9b8b55ebe19bbb50fad847d895b7d470e89e9786677b5ab2"
  "doc/plans/DONE-PLAN-portal-token-savings.md#sibling-plans": "sha256:38f6b38f03e7c95cdce7a658c7d023d6b93b08de94de4b6759c4acddd5d82c53"
  "doc/plans/DONE-PLAN-portal-token-savings.md#what-the-numbers-decided": "sha256:963d03fd2089d17f465e8eba048e07cbc05b0263572db823cf6ef0c3804bb0da"
---

# Portal token savings plan

## Scope and origin

[keep] The plan takes three ideas from Shunt, a Portal plugin: enforcement over advice, a worker that is one call with no agent loop, and per-role configuration. [Background](../../../plans/DONE-PLAN-portal-token-savings.md#background)

[keep] It does not build Shunt's code-writer equivalent, because Write and Edit output is about 1.1% of cost. [Rejected](../../../plans/DONE-PLAN-portal-token-savings.md#rejected)

## Cost findings

[keep] Attribution comes first because 42% of measured cost has no explanation yet. [What the numbers decided](../../../plans/DONE-PLAN-portal-token-savings.md#what-the-numbers-decided)

[keep] A Shunt-style worker saves at most 3.7% to 5.0% under its stated assumptions, so the read worker ships for evaluation only. [What the numbers decided](../../../plans/DONE-PLAN-portal-token-savings.md#what-the-numbers-decided)

[keep] A spawned reader loses to a direct read, so the worker must be a single call. [What the numbers decided](../../../plans/DONE-PLAN-portal-token-savings.md#what-the-numbers-decided)

[keep] The read gate needs no model, because 72% of the uncovered-file warnings are for Markdown files that a heading outline can serve. [What the numbers decided](../../../plans/DONE-PLAN-portal-token-savings.md#what-the-numbers-decided)

[keep] The 400-line threshold stays, since a denied call breaks even near 290 lines at Shunt's measured rate. [What the numbers decided](../../../plans/DONE-PLAN-portal-token-savings.md#what-the-numbers-decided)

[keep] Startup is the cheapest lever, because engineer subagents start with more resident tokens than reviewers. [What the numbers decided](../../../plans/DONE-PLAN-portal-token-savings.md#what-the-numbers-decided)

## Goals and limits

[keep] The goals are attribution of cost by agent type, a mechanical worker with no skill listing, a deny-once read gate in live stages, Markdown outlines, and a one-call ask command. [Goals](../../../plans/DONE-PLAN-portal-token-savings.md#goals)

[keep] No agent is told to use the ask command, and no local models are used. [Non-goals](../../../plans/DONE-PLAN-portal-token-savings.md#non-goals)

[keep] Review, debugging and verification are never routed to a cheaper worker. [Non-goals](../../../plans/DONE-PLAN-portal-token-savings.md#non-goals)

[keep] Only Sonnet and Opus implement the stages, and every standard stage lists claude as its implementer. [Implementers](../../../plans/DONE-PLAN-portal-token-savings.md#implementers)

[keep] Skill listing size and the Sonnet and Opus tool lists are deferred to decisions outside this plan. [Later decisions, outside this plan](../../../plans/DONE-PLAN-portal-token-savings.md#later-decisions-outside-this-plan)

## Execution

[keep] The plan runs alone after the feat-planner-13 plan merges, because both write the knowledge tree and the check baseline. [Sibling plans](../../../plans/DONE-PLAN-portal-token-savings.md#sibling-plans)

[keep] The three feature stages run concurrently, then integration verification and knowledge distillation run in order. [Execution Diagram](../../../plans/DONE-PLAN-portal-token-savings.md#execution-diagram)
