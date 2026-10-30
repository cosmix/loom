---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md
file: sha256:31b1edd28a3bbc313fec1d3a12715267cf136ff4d186bdbbb518787b38be4c8d
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#decisions": "sha256:d32c87b265f5fcbb8f459024244404dd1595fb66330f119a9c6323c6ce0adb15"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#goals": "sha256:ffcea48914e7947941243891bdfc5519bfdf64ac5aa60dd8f1261b4510792e8d"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#integration-verification": "sha256:d7afc903dafafb9e9e1eb05308848ea6a66cf86bfc6e77ca127f1bb6c9ffe68e"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#knowledge-distillation": "sha256:cedaadb4e9d187449a3bbf085dac516e09740e1f0c387a10ca57c2cf36319485"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#overview": "sha256:cd8cee948894b694b2a58db20d3633acfc49066e1e180d231f694d4de2913aaf"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#stages": "sha256:ab01464f7c9c767726bf18f962e13bbf85b6b7d6871221922c1a6f82b9ffdeae"
---

# Session bridge plan

## Overview

The loom plan server starts each interactive Claude Code session in its own tmux server under the sandbox settings of plan 6 and rebuilds its chat from the session transcript. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#overview)

States the transcript does not carry come from a hook event spool, and permission requests are answered through answer files the session cannot write. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#overview)

The user's messages reach the pane only while the session is idle, and only the authoring holder's connection may command the chat. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#overview)

## Goals

`Bridge::start_session` is the one way plans 8 to 11 start a session, and `BridgeEvent`s report what happened. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

Subagent events never move the session state, and liveness goes through `ProcessIdentity` rather than `tmux has-session`. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

Every permission request is answered before Claude Code's one-hour hook timeout, so no session waits on a dialog nobody sees. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

The chat model is rebuilt from transcripts only, and nothing is resumed across servers. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

## Decisions

Input is sent as a bracketed paste from a loaded tmux buffer, never with `load-buffer -w`. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#decisions)

The opening message is pasted after the first `SessionStart`, never passed as a command-line argument, because other local users can read the command line. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#decisions)

The server denies a pending request at 3590 seconds, so the hook gives up at 3595 seconds, before Claude Code's registered 3600-second timeout. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#decisions)

## Stages

Stage `bridge-core` builds the pure halves, and stage `bridge-runtime` adds launch, tmux input, the bridge and the server wiring. [Stages](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#stages)

The operator end-to-end run with a logged-in Claude Code is a person's step, and integration verification blocks with its exact command. [Integration Verification](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#integration-verification)

Knowledge distillation records the bridge as a tier-2 topic and any transcript or hook shape the stages found to differ from the briefs. [Knowledge Distillation](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-07-session-bridge.md#knowledge-distillation)
