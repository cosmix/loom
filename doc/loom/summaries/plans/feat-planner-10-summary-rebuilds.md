---
kind: document
of: doc/plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md
file: sha256:f5a69ef4a411d9a113cda5d361085511cddd735b660e1f642e711073de8e3a60
cites:
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions": "sha256:5c8c7a4014277ed937867acc866609ffaa1b6503f30902a1bd8dc965d5a86050"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals": "sha256:2313cf9f0780fb05c99d0453bb11901d202002705f70d20d28e60ef96b6558bc"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#integration-verification": "sha256:f0320d4e7cd86f5192094018e339d88d03ac1aafdf078291f2049f1fe700c7ff"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#knowledge-distillation": "sha256:c579915a16b215fb49b469e769bbee323a4a37ba6f9787d10365326e8c0ca909"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview": "sha256:e4a9868ad177e96de840b0092f347979dc4301eaf963520fa5c7762b5e76062e"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#stages": "sha256:b14b43c4076de86fdd736a9e85d00521184c171e1ba24ea3409d2f94c3dd862d"
---

# Summary rebuilds and cost

## Overview

After each watcher batch the planning server collects the stale, incomplete or missing summaries, starts one background summary session for up to five, and asks every browser first above five. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview)
The server clears claims of ended sessions, stops a summary session once its claims are done or after ten minutes without a seal, and logs each rebuild's tokens. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview)
The summary panel shows the running rebuild, the last cost, today's and the week's totals, and the quota meters. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview)

## Goals

No more than five summaries rebuild without a question, a first start with no summaries asks, and a decline is remembered for the summaries it covered. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)
Nothing starts while an authoring turn or a pressure run is active, and a failed rebuild leaves the last sealed text readable and marked. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)
Costs match the counts of `loom status` because both are computed by the same accumulator. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)

## Decisions

The scheduler is pure and starts or stops sessions only through a spawner trait, so its rules are tested with a stub spawner and an injected clock. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)
The question counts only the newly invalidated set, and the first start asks because every missing summary counts as invalidated. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)
Authoring turns and pressure runs suspend starts through RAII guards, and a summary session ends at teardown, once its claims are done, or after the no-seal timeout. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)
The server clears a session's claims when the session ends and a subagent's claims when that subagent stops, read from its transcript. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)
Cost comes from the token tracker's accumulator, so a streamed message counts once, and a background session's cost is attributed per summary. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)
Estimates use a summary's last recorded cost, else the per-level median of cost per source byte, else a labelled starting figure. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)

## Stages

The `rebuild-engine` stage adds the Rust scheduler, cost library, facade, claim clearing and routes, and `rebuild-web` adds the panel and buttons in `web/`. [Stages](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#stages)
Integration verification runs the full Rust and web gates and an operator-run rebuild of this repository's summaries. [Integration Verification](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#integration-verification)
Knowledge distillation records the scheduler rules and cost model as a tier-2 topic and adds a README note on the panel. [Knowledge Distillation](../../../plans/planner-ui/DONE-PLAN-feat-planner-10-summary-rebuilds.md#knowledge-distillation)
