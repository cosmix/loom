---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md
file: sha256:03f8a9c157c6e3239cca2613ff98ad07697c3cc5e4619f8fbc04655a76416ef1
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#1-pressure-pipeline": "sha256:5b78e4c26648f27d0bd9558eb154f5fc6d9d7b54a61589d94265632d41d1c15e"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#2-app-handoff": "sha256:63f2ac10b0929a6b99babd22f91aeeb5f073b57219b5a7e046a081dabe02eb0a"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#3-handoff-web": "sha256:b8f7c2991c7fd1d9e6999f93049552fc7c7af1d1786932c98c5ad7ac38d5fc66"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#4-integration-verify-5-knowledge-distill": "sha256:75c9f3fdf33135a457a88bdd9fb8d44f761db68a658a759ef5198286bb867ad4"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#goals": "sha256:a33574b7fdb6435eaffc7a405f35f6b5375a3e3b2d3f5b7be5ef99efc9d8f0a0"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#overview": "sha256:cadca11c727a734178ccc530cfe47a0311055bb64b5b139883d562a03e50b5fc"
---

# Execution handoff

## Scope

This plan connects planning to `loom pressure` and `loom run`, and it moves the pressure pipeline into `loom::pressure` so that the terminal and app drivers share it. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)
The pipeline writes a `## Pressure tests` record after a run that completed every round, so terminal runs write it too. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)
The app runs Claude steps as pressure sessions and Codex as a child of the server, and it stops cleanly on Stop, a failed step, Ctrl-C or a dead server. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)
The run side adds the run dialog, the commit of `doc/` changes, `loom init` and `loom run` as children, and Abandon run with its confirmations and rename to `RETIRED-`. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)
Outside the app, loom's built-in default backend becomes tmux. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

## Goals

A recorded plan must still pass `loom plan verify` and the plan preflight in `loom init`. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)
A Codex step that changes `.claude/`, `.mcp.json` or `CLAUDE.md` ends the run and blocks new planning sessions until the holder confirms. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)
Non-goals are stage monitoring, which the dashboard does, and any check beyond loom's own refusals. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)

## Stages

The pressure-pipeline stage builds the shared pipeline, the record writer and the tmux default. [1. pressure-pipeline](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#1-pressure-pipeline)
The app-handoff stage builds the server side of pressure runs and the run handoff, and it needs the merged `loom::pressure` from stage one. [2. app-handoff](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#2-app-handoff)
The handoff-web stage builds the browser side in `web/` on top of the fixtures and routes from app-handoff. [3. handoff-web](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#3-handoff-web)
The final stages run the full gate, the reviews, an operator-run block for the real pressure test and commit-and-run, and then knowledge distillation. [4. integration-verify, 5. knowledge-distill](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-11-execution-handoff.md#4-integration-verify-5-knowledge-distill)
