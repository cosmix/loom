---
kind: document
of: doc/plans/DONE-PLAN-feat-planner-07-session-bridge.md
file: sha256:31b1edd28a3bbc313fec1d3a12715267cf136ff4d186bdbbb518787b38be4c8d
cites:
  "doc/plans/DONE-PLAN-feat-planner-07-session-bridge.md#decisions": "sha256:d32c87b265f5fcbb8f459024244404dd1595fb66330f119a9c6323c6ce0adb15"
  "doc/plans/DONE-PLAN-feat-planner-07-session-bridge.md#goals": "sha256:ffcea48914e7947941243891bdfc5519bfdf64ac5aa60dd8f1261b4510792e8d"
  "doc/plans/DONE-PLAN-feat-planner-07-session-bridge.md#overview": "sha256:cd8cee948894b694b2a58db20d3633acfc49066e1e180d231f694d4de2913aaf"
  "doc/plans/DONE-PLAN-feat-planner-07-session-bridge.md#stages": "sha256:ab01464f7c9c767726bf18f962e13bbf85b6b7d6871221922c1a6f82b9ffdeae"
---

# Session bridge plan summary

## Overview

The `loom plan` server owns interactive Claude Code sessions and starts each one in its own tmux server under plan 6's sandbox settings, as set out in [Overview](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#overview)

It reads each transcript into chat messages and learns the states the transcript does not carry from a hook event spool, as set out in [Overview](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#overview)

It answers permission requests through answer files the session cannot write, delivers user messages into the pane only while the session is idle, and recovers sessions a dead server left running, as set out in [Overview](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#overview)

## Goals

`Bridge::start_session` is the one way plans 8, 9, 10 and 11 start a session, and `BridgeEvent`s report what happened, as set out in [Goals](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

Subagent events never move the session state, and liveness goes through `ProcessIdentity` rather than `tmux has-session`, as set out in [Goals](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

Input reaches the pane only while Idle, sanitised and as a bracketed paste, and is confirmed by `UserPromptSubmit` within 10 seconds, as set out in [Goals](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

Every permission request is answered before Claude Code's one-hour hook timeout, as set out in [Goals](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

The chat model is rebuilt from transcripts only, and nothing is resumed across servers, as set out in [Goals](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#goals)

## Decisions

All host calls run on the driver thread, so a wedged tmux call stalls one tick and never a WebSocket, as set out in [Decisions](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#decisions)

The server denies a pending request at 3590 seconds, before the hook gives up at 3595 seconds and Claude Code's registered timeout of 3600 seconds, as set out in [Decisions](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#decisions)

The opening message is pasted after the first `SessionStart` and never passed as a launch argument, because the process command line under `/proc` is readable by other local users, as set out in [Decisions](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#decisions)

The operator end-to-end run against a logged-in Claude Code is a person's step, which integration verification blocks with the exact command, as set out in [Decisions](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#decisions)

## Stages

The plan has two standard stages: `bridge-core` for the pure model, attachments, transcript mapper, spool, permission hook and state machine, and `bridge-runtime` for launch, tmux input, the bridge and server wiring, as set out in [Stages](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#stages)

Integration verification runs the metaplan's full gate, a security review against section 12, and the recovery test on real tmux where the host allows it, as set out in [Stages](../../../plans/DONE-PLAN-feat-planner-07-session-bridge.md#stages)
