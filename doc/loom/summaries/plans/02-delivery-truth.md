---
kind: document
of: doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md
file: sha256:63113a835ad21a639bf95dcb3e4f6bd73f93d287e30ba651166a4030a3cc142e
cites:
  "doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#coverage-of-metaplan-section-1": "sha256:00d830aa102c85d5c819c5c94f58857d11ae0b70ef3af81851cc37eb422d70ad"
  "doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#deviations-from-the-metaplan": "sha256:53603c0ace74b2cb4f3ed25c8783c6c8abbb622263648a922034132a8daea97e"
  "doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#goals": "sha256:ead237c78aca8b39e97a73abe2c7a2a87779eb56cd9fd11c613e1cbc9d450dbc"
  "doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#overview": "sha256:692d916984271f5273a313e1faaf9acdacd200a19c35728bb7e11d0eb8ce2eed"
  "doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#shared-files-and-sibling-plans": "sha256:dea5502975dfa6b5938b033f9dd6063c38e31ec29ad965752d69cecbba95d690"
  "doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#stages": "sha256:d37916abca7ed8eb0f6c9e509ac1a9977bf62e02fb021afbb41b47d0f7d4c8fe"
  "doc/plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#why-looms-stages-issued-no-worker-brief-step-3-established-while-planning": "sha256:79c8277d128759bbbaa5c3dd84ff6e5cb982224d38d403d040e2027bc621eecd"
---

# Delivery truth plan (plan 02)

## Scope

Plan 02 of the token-optimization metaplan removes the read-receipt path, files each worker-brief delivery record when the brief is issued, and resets an ordinary session's prompt record after compaction. [Overview](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#overview)
Its goals also stop loom-codex-forwarder and codex spawns from receiving a worker brief, and they exclude receipt repair, a stage compaction marker and Codex-side retrieval. [Goals](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#goals)
The change is a byte change that leaves agent behaviour alone, so no paired run is needed, and the operator checks a worker transcript after merge. [Overview](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#overview)

## Cause

Loom's own stages received no worker brief because the spawn guard's 3-second bound around the worker-brief hook was exceeded, since retrieval time grows with query length. [Why loom's stages issued no worker brief](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#why-looms-stages-issued-no-worker-brief-step-3-established-while-planning)
The fix caps the worker query at 2,048 bytes and raises the delegate bound to 5 seconds, and it does not trim pack items because the replay found no envelope over the limit. [Deviations from the metaplan](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#deviations-from-the-metaplan)

## Stages and checks

Stage delivery-fixes runs three Claude sonnet workers and two Codex units with disjoint territories, and integration-verify and knowledge-distill follow it. [Stages](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#stages)
The Codex lane takes the worker-brief and pre-compact units because they have pinned interfaces, while the receipt-module deletion and the hook edits stay on Claude. [Overview](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#overview)
The stage gate is the pre-push gate minus cargo audit, the flake check and the git-environment scrubbing, and the unfiltered suite runs in the verifier round. [Overview](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#overview) [Deviations from the metaplan](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#deviations-from-the-metaplan)
Expected test-integrity events cover the deleted receipt, bind and epoch tests, and the stage files one dispute for them after the final review round. [Stages](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#stages)

## Coordination

Plan 3 must merge before this plan starts, because the before_stage check looks for remind_memory_capture in post-tool-use.sh. [Overview](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#overview)
Shared files with plans 3, 2, 4 and 5 are listed with what each side assumes. [Shared files and sibling plans](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#shared-files-and-sibling-plans)
The coverage table maps each metaplan section 1 bullet to the workers, contracts and checks that cover it. [Coverage of metaplan section 1](../../../plans/optim-token/archive/DONE-PLAN-02-delivery-truth.md#coverage-of-metaplan-section-1)
