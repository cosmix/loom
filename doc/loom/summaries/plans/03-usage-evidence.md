---
kind: document
of: doc/plans/archive/DONE-PLAN-03-usage-evidence.md
file: sha256:5931a3bcf28e63af11b9f0cad0bee6eca59ea17c71e0bf5d2e34ef5be68c436b
cites:
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md#brief-amendments-binding-over-the-briefs": "sha256:efe5454057f5323c68f11ad376e1d11de4046709cf71f6b7082a8ed197f5e746"
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md#deviations-from-the-metaplan": "sha256:6c9734ef6d225dc6b886d4d266dc9b3ab97b54dcfe6645d4e43fe5b01aa53fc2"
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md#goals": "sha256:cb6cdf9f33bf7d267a4aa61c231eb7c7b1b7a1efd9356a7dfaf2f867f42d1180"
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md#live-protocol-operator-procedure-no-stage-runs-it": "sha256:62586ed8749c74b57babd7a61f19486ff0a92856827e17e10d61019f90884adc"
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md#overview": "sha256:96db8afc654a19d01abe2aed841f12bc804923bb448e5ee271c03b55e1b90f04"
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md#replay-harness-contract-for-plans-4-and-5": "sha256:8f906a5ec31b4dadd775d563429ee6867f98e519c0396ca6282dfcafe2717e91"
  "doc/plans/archive/DONE-PLAN-03-usage-evidence.md#stages": "sha256:18cea038ae22bb44579a055c979f895b0eb26ff9380a1236455fcb5a2676a594"
---

# Usage Evidence

This plan fixes the accounting that `loom usage` reports, adds declared net-total and latency rules, writes `RunEvidence`, and builds a replay harness, and it claims no saving itself. [Source plan](../../../plans/archive/DONE-PLAN-03-usage-evidence.md)

## Scope

The plan runs after plans 3 and 1 have merged, and its first stage's `before_stage` checks fail fast when either has not. [Overview](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#overview)

Before this plan, removed stage worktrees went unread, Codex rows were not scoped to a project, forward receipts never formed, and quota continuity read 1 s `resets_at` jitter as a reset. [Overview](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#overview)

The plan adds no saving and changes nothing agents are given, and its replay spends no provider tokens. [Overview](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#overview)

## Stages

Stage one, `usage-accounting`, reads removed worktree transcripts, scopes Codex totals to a project's own threads, and forms forward receipts from the PostToolUse payload. [Stages](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#stages)

Stage two, `evidence-and-replay`, adds the declared net-total and latency rules, the quota tolerance, the `--run-evidence` producer and the replay harness. [Stages](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#stages)

The completeness reasons leave out fallback-coverage rows, because every current Codex rollout would otherwise read partial. [Deviations from the metaplan](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#deviations-from-the-metaplan)

## Verdict and replay

Quota continuity and the comparison tolerate 300 s of `resets_at` jitter and one 180 s poll interval. [Goals](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#goals)

The net-total rule needs at least three pairs per artifact and judges the mean difference against two standard errors. [Live protocol (operator procedure; no stage runs it)](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#live-protocol-operator-procedure-no-stage-runs-it)

The replay harness lives at `tests/signal_replay.rs`, and later plans locate it by that path rather than by grep. [Replay harness contract (for plans 4 and 5)](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#replay-harness-contract-for-plans-4-and-5)

The harness prints per-section signal bytes, worker-brief bytes and asset sizes for a frozen fixture. [Replay harness contract (for plans 4 and 5)](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#replay-harness-contract-for-plans-4-and-5)

## Operator steps

The live paired-trial protocol is operator procedure, because each trial consumes subscription quota. [Live protocol (operator procedure; no stage runs it)](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#live-protocol-operator-procedure-no-stage-runs-it)

The producer refuses a run that has in-scope usage from a provider its declaration does not assign. [Brief amendments (binding over the briefs)](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#brief-amendments-binding-over-the-briefs)

Forward receipts need a wider argv parser, `forward_argv`, because `exact_forward_argv` accepts only the 8-word wrapper form. [Deviations from the metaplan](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#deviations-from-the-metaplan)

Brief amendments bind over the worker briefs, and each stage's main agent quotes them into every worker's spawn prompt. [Brief amendments (binding over the briefs)](../../../plans/archive/DONE-PLAN-03-usage-evidence.md#brief-amendments-binding-over-the-briefs)
