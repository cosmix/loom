---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md
file: sha256:2c1a786f34b3cf74ffbf93fd2d78490e7fe778963d9083a2daf7be6656a40c55
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#1-guidance-engine": "sha256:ec5c8819b89c3c981f5c23eb7eba40ee576445e5e4508ae6d926290839d7f267"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#2-integration-verify": "sha256:2e567361414ec7339f2b25811cacc11fdedad8c0604575fb48bc1d9a2bf4bee5"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#3-knowledge-distill": "sha256:534aac45f345497672ef12c2638a8c3a59786876b58bcf0831442b779e8bf0bd"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#goals": "sha256:f89cf2f2f349fbb6ce0aeb58129c4f908abf91fe06489de374a5509a9f3bc3fa"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#overview": "sha256:a2f5acd42c01a1abc94f9c1658a6b1abcf090d37b153dfe7b63c5746723a9a1e"
---

# Plan: Session guidance and the loom-plan plugin

## Scope

This plan writes the guidance and the loom-plan plugin that every loom plan session receives at launch. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#overview)
The plugin lives in plan-plugin/ at the repository root with nine skills and the summary and overview agents, and the pressure commands are taken from commands/ at render time. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#goals)
The text is embedded in the binary as PLAN_PLUGIN and is never installed by install_all. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#goals)
The guidance module renders a per-kind plugin, a brief, the opening message for every offer and the closing message. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#goals)
The server serves each session's brief over the Brief link route and installs its guidance provider into plan 7's slot. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#goals)
The plan adds four [plan] authoring keys to the typed key registry. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#goals)
It depends on plans 1, 4, 6 and 7, and plans 9, 10 and 11 start sessions through it. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#overview)
Launching sessions, chat rendering, the summary scheduler and pressure runs are out of scope. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#goals)

## Stages

The guidance-engine stage runs its workers in waves over disjoint files, and the content and Rust work share one wave. [1. guidance-engine](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#1-guidance-engine)
Each risk checklist item maps to a named contract test, such as render-refuses-symlinked-plugin-dir. [1. guidance-engine](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#1-guidance-engine)
The integration-verify stage proves that a planning session started through plan 7's spawn path gets its rendered brief and plugin. [2. integration-verify](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#2-integration-verify)
The operator run of one session per skill family is an ignored test that the stage cannot run. [2. integration-verify](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#2-integration-verify)
The knowledge-distill stage records the plugin layout, render rules and authoring keys in the knowledge base. [3. knowledge-distill](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-08-guidance.md#3-knowledge-distill)
