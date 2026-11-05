---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md
file: sha256:f5a69ef4a411d9a113cda5d361085511cddd735b660e1f642e711073de8e3a60
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions": "sha256:5c8c7a4014277ed937867acc866609ffaa1b6503f30902a1bd8dc965d5a86050"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals": "sha256:2313cf9f0780fb05c99d0453bb11901d202002705f70d20d28e60ef96b6558bc"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview": "sha256:e4a9868ad177e96de840b0092f347979dc4301eaf963520fa5c7762b5e76062e"
---

# Summary rebuilds and cost

## Overview

The plan keeps summaries current after each watcher batch by starting one background summary session for up to five stale, incomplete or missing summaries, and by asking browsers first above five. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview)

It clears the claims of ended sessions and subagents, stops a summary session once its claims are sealed or released or after ten minutes without a seal, and measures every rebuild's tokens from its transcripts. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview)

It shows the running rebuild, the last cost, today's and the week's totals and the quota meters in a summary panel. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#overview)

## Goals

No more than five summaries rebuild without a question, a first start with no summaries asks, and a decline is remembered for the summaries it covered. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)

Nothing starts while an authoring turn or a pressure run is active, and at turn end what is still out of date goes to the queue. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)

A failed rebuild leaves the last sealed text readable and marked, and the summaries it held show as failed with the reason. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)

Costs match the `loom status` counts for the same transcripts because both use the same accumulator. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)

Non-goals are the summary skills and agents, the session bridge, and concept mode's rendering. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#goals)

## Decisions

The scheduler is pure and starts or stops sessions only through a spawner trait, so a stub spawner and an injected clock test every rule. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)

The question triggers on the newly invalidated set, and after a decline those ids stay out of later counts until they become current or the user presses rebuild. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)

Suspension is an RAII guard, and on the last drop the scheduler recomputes the out-of-date set and applies the threshold. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)

Claims are cleared by the server on a session's ended event or on a subagent's stop, and estimates use the per-level median cost per source byte with fixed starting figures when no log line exists. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)

Read-only mode starts nothing and shows the reason and fix in the panel. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-10-summary-rebuilds.md#decisions)
