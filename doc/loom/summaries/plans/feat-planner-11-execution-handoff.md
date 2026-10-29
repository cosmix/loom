---
kind: document
of: doc/plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md
file: sha256:dcd4f79eadf92d75d1621ab9fce0953959ca2471ccba1f4646a4388ffa9086d9
cites:
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#execution-diagram": "sha256:f541a2dcee01ad694b9a5015d7116e53d558268d0e0ec36af5099b5d5661ca0a"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#goals": "sha256:a33574b7fdb6435eaffc7a405f35f6b5375a3e3b2d3f5b7be5ef99efc9d8f0a0"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview": "sha256:cadca11c727a734178ccc530cfe47a0311055bb64b5b139883d562a03e50b5fc"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#stages": "sha256:5027381bc23f60a1dced89a5687cda2b7c48fb08babeda403de299f2346fca4a"
---

# Execution handoff

The plan moves the pressure pipeline out of `loom/src/commands/pressure/` into `loom::pressure`, shared by the terminal and app drivers. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

It adds a `## Pressure tests` record that section 01 detection reads, written by the shared pipeline after a run completes every round. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

The app runs Claude steps as pressure sessions through plan 7 and runs Codex as a child of the server. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

The app stops cleanly on Stop, a failed step, Ctrl-C or a dead server. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

The run side adds the run dialog, the commit of `doc/` changes, `loom init` and `loom run` as children, and Abandon run. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

Outside the app, loom's built-in default backend becomes tmux. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

The plan depends on plans 1, 2, 6, 7, 8, 9 and 10, and nothing depends on it. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#overview)

## Goals

The goals require one pressure pipeline for both drivers, with `loom pressure --dry-run` listing the record step. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)

App pressure runs refuse invalid options and run Claude steps with `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)

Run handoff covers workspace-state offers, the uncommitted split at `doc/`, and Abandon run with its two confirmations. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)

A Codex step that changes `.claude/`, `.mcp.json` or `CLAUDE.md` ends the run and blocks new planning sessions until the holder confirms. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)

Non-goals are stage monitoring, which the dashboard does, and any check beyond loom's own refusals. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#goals)

## Execution diagram

The diagram chains `pressure-pipeline`, `app-handoff`, `handoff-web`, `integration-verify` and `knowledge-distill` in that order. [Execution diagram](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#execution-diagram)

## Stages

The `pressure-pipeline` stage exists on its own so the terminal driver and the record are proven before the app driver builds on them. [Stages](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#stages)

The `handoff-web` stage works in `web/` with its own working directory and consumes stage 2's fixtures and routes. [Stages](../../../plans/planner-ui/DONE-PLAN-feat-planner-11-execution-handoff.md#stages)
