---
kind: document
of: doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md
file: sha256:5931a3bcf28e63af11b9f0cad0bee6eca59ea17c71e0bf5d2e34ef5be68c436b
cites:
  "doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#brief-amendments-binding-over-the-briefs": "sha256:efe5454057f5323c68f11ad376e1d11de4046709cf71f6b7082a8ed197f5e746"
  "doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#deviations-from-the-metaplan": "sha256:6c9734ef6d225dc6b886d4d266dc9b3ab97b54dcfe6645d4e43fe5b01aa53fc2"
  "doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#goals": "sha256:cb6cdf9f33bf7d267a4aa61c231eb7c7b1b7a1efd9356a7dfaf2f867f42d1180"
  "doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#live-protocol-operator-procedure-no-stage-runs-it": "sha256:62586ed8749c74b57babd7a61f19486ff0a92856827e17e10d61019f90884adc"
  "doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#overview": "sha256:96db8afc654a19d01abe2aed841f12bc804923bb448e5ee271c03b55e1b90f04"
  "doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#replay-harness-contract-for-plans-4-and-5": "sha256:8f906a5ec31b4dadd775d563429ee6867f98e519c0396ca6282dfcafe2717e91"
  "doc/plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#stages": "sha256:18cea038ae22bb44579a055c979f895b0eb26ff9380a1236455fcb5a2676a594"
---

# Usage Evidence

## Scope

The plan repairs the accounting in `loom usage` and `--compare` and claims no token saving itself. [overview](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#overview)

Plans 4 and 5 are judged on a deterministic replay, and plan 4 also on frozen single-stage replays compared with `loom usage --compare`. [overview](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#overview)

The plan runs after plans 3 and 1 have merged, because both edit `post-tool-use.sh`, `types_ops.rs` and `dispatch.rs`. [deviations](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#deviations-from-the-metaplan)

Forward receipts form from the PostToolUse payload, and a receipt whose thread has no rollout in the window reads `partial`. [goals](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#goals)

`--compare` keeps every current verdict for an artifact that declares nothing, except that its quota checks gain tolerances. [goals](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#goals)

Quota checks tolerate 300 s of `resets_at` jitter and one poll interval of 180 s for interval length. [goals](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#goals)

`loom usage --run-evidence` with a declaration file writes `RunEvidence` that `--compare` reads back. [goals](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#goals)

## Stages and replay

The first stage has five workers, three Claude and two Codex, and the main agent runs the gate through a verifier round. [stages](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#stages)

The second stage adds the net-total and latency rules, the `RunEvidence` producer and the replay harness. [stages](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#stages)

The replay test `signal_replay` is marked `#[ignore]`, and later plans locate it by path. [replay](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#replay-harness-contract-for-plans-4-and-5)

## Live evidence

Live trials consume subscription quota, so the live protocol is operator procedure and no stage runs it. [live](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#live-protocol-operator-procedure-no-stage-runs-it)

A missing measurement is unknown, never zero. [live](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#live-protocol-operator-procedure-no-stage-runs-it)

The net-total rule needs at least 3 pairs per artifact, and fewer read `insufficient-pairs`. [live](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#live-protocol-operator-procedure-no-stage-runs-it)

The producer refuses a run with in-scope usage from a provider the declaration does not assign. [amendments](../../../plans/optim-token/archive/DONE-PLAN-03-usage-evidence.md#brief-amendments-binding-over-the-briefs)
