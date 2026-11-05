---
kind: document
of: doc/plans/DONE-PLAN-contract-phase-autonomy.md
file: sha256:7fa5df61b5dd43c16cb679dcd26a6e0d8459b1cdef1bc91586fe6272e61387bf
cites:
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#1-contract-collection": "sha256:678c7c5ecebb23d1618a3769b34614e0cae5d26e79ff23614b90ef5868b8ca98"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#2-refusal-adjudication": "sha256:28641dda9635467e62d8e9e0cd7908c9af442b1f490aa5c137504b41d5e29c9a"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#decisions": "sha256:f54d6bef3ff115eb9547a96d5494dcd98917479755ee8a77bb29055427de109e"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#execution-diagram": "sha256:2c6aa8b06ef70b4012251c1edffbab01c2bc173983e564a6830bb2c8d8eca5b9"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#goals": "sha256:e84cd44f4b9586160cf8dda591d2c41dc112f7c7c3c48ae1e6f996d54b1c4c65"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#integration-verification": "sha256:9dfa2fe994f86f5fde060e020ab3c5c2baca10a21d916f3e03f1222a026e230b"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#knowledge-distillation": "sha256:36a144f5433540d5eec33e7339af915a6876ceca4747a9727844e513a47c9c63"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#overview": "sha256:3ac3c867947edd70d7cab34b16b44a2b8f0ddbd79c4e664aa530b1bd68e68b33"
  "doc/plans/DONE-PLAN-contract-phase-autonomy.md#sibling-plans": "sha256:365445b5104ffff10478b8cf16ae8b570f80f7086f9ac10611d9f980be4ff7ce"
---

# Contract Phase Autonomy

## Problem

A contract stage in liquidmon could never freeze because the harness omitted a module file the writer had to add, and the refusal parked the stage for the operator. [Overview](../../../plans/DONE-PLAN-contract-phase-autonomy.md#overview)

## Plan

Loom derives the test module scaffold for each Rust contract, and the contract writer may add only those declarations. [Decisions](../../../plans/DONE-PLAN-contract-phase-autonomy.md#decisions)

Freeze and completion prove that each Rust contract compiles from its frozen file through a libtest target's module chain. [Decisions](../../../plans/DONE-PLAN-contract-phase-autonomy.md#decisions)

A refused freeze becomes a `freeze-refusal` dispute that an adjudicator decides, and no contract-phase state parks on the operator. [Decisions](../../../plans/DONE-PLAN-contract-phase-autonomy.md#decisions)

Direct-mode codex forwards get lifecycle records from their receipts, so their requested model shows once each forward ends. [Overview](../../../plans/DONE-PLAN-contract-phase-autonomy.md#overview)

## Goals

Verdicts must carry reasoning and a citation, and the stage validates them before recording, so no agent settles a disputed constraint alone. [Goals](../../../plans/DONE-PLAN-contract-phase-autonomy.md#goals)

Non-goals keep `AskUserQuestion`, `loom stage block` and rejected criterion disputes on the operator path, and the scaffold and chain checks cover only Rust. [Goals](../../../plans/DONE-PLAN-contract-phase-autonomy.md#goals)

## Stages

Stage 1, contract-collection, builds scaffold derivation, the chain check at freeze and completion, plan-verify lints, and direct-mode codex reconciliation. [1. contract-collection](../../../plans/DONE-PLAN-contract-phase-autonomy.md#1-contract-collection)

Stage 2, refusal-adjudication, adds the freeze-refusal dispute kind, the harness amendment field and the refusal filing step, and removes the contract-phase parks. [2. refusal-adjudication](../../../plans/DONE-PLAN-contract-phase-autonomy.md#2-refusal-adjudication)

Stage 2 depends on stage 1 because its adjudication calls the scaffold derivation. [2. refusal-adjudication](../../../plans/DONE-PLAN-contract-phase-autonomy.md#2-refusal-adjudication)

Integration verification runs the full suite, the lint and hook gates, and an end-to-end check of the liquidmon shape. [Integration Verification](../../../plans/DONE-PLAN-contract-phase-autonomy.md#integration-verification)

Knowledge distillation rewrites the contract-phase architecture note to current truth and records the open concerns. [Knowledge Distillation](../../../plans/DONE-PLAN-contract-phase-autonomy.md#knowledge-distillation)

## Coordination

This plan must not run concurrently with its sibling plans, because their declared file ownership overlaps. [Sibling Plans](../../../plans/DONE-PLAN-contract-phase-autonomy.md#sibling-plans)

Execution runs contract-collection, then refusal-adjudication, then integration-verify, then knowledge-distill. [Execution Diagram](../../../plans/DONE-PLAN-contract-phase-autonomy.md#execution-diagram)
