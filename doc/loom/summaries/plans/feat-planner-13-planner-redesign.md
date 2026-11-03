---
kind: document
of: doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md
file: sha256:a9c594cd745db9fc5b4818490c86591574842534bc1f3c34dec321a736c0d124
cites:
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#1-session-and-wire": "sha256:2342a0b520495b6b43b80f22f922987f2394b8832a45cbf132dfb3d39324ab12"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#2-shell-and-spine": "sha256:d261272dbc9d522a4fb21f61717a4949399f055a292170f4f417210799304921"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#3-pages-and-reading": "sha256:cf8f1382787232f79b0a3b6e9c457252dbb320c4551fe2201ae448d6313af07e"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#4-chat-redesign": "sha256:a688d59459e4604e9500d33bfcf77325b11a367685b5d2670de51890cb217991"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#evidence-behind-the-decisions": "sha256:1307b5185b0ac020ed44cf74630b6b5f25274c4638381b0060e60bbdd2798988"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#goals": "sha256:3eebb6c476e1f9a1b6c8af0bec84f1fa43a659cff6664c311d3ebc3815456c8f"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#integration-verification": "sha256:917972ac7e330adf8694094bfc08da1a6fa1542615648fb7811f64dbae0a5bad"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#knowledge-distillation": "sha256:fe5a83c1de1b9f1d16d0b486813022908125ffe386cc8afe3b7f30cd7ced5176"
  "doc/plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#overview": "sha256:e85113e912dd52948d67f8d30cc77f69a9857cd951a3ed2650768f4a95d43041"
---

# Plan 13: Redesign the planner around reading and contextual chat

## Overview

Plan 12 passed every gate yet left the feature screen, concept mode and chat hard to use. [Overview](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#overview)
The feature screen becomes a step spine that opens real content, and finished features open on the product overview. [Overview](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#overview)
Chat becomes contextual, names the document the user views, and attaches it to the next message. [Goals](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#goals)
Planning sessions that outlive their server and test runs that start real Claude sessions in the operator's home are both stopped. [Overview](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#overview)

## Goals

Every sidebar row opens real content, and no screen draws one offer twice. [Goals](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#goals)
Authoring chats stop prompting for innocuous actions, and a classifier block shows in the chat. [Goals](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#goals)
No planning session or tmux server outlives its server by more than a 15-minute grace. [Goals](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#goals)
Per-stage run progress on the Run page, multi-user changes and a new theme system are out of scope. [Goals](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#goals)

## Evidence behind the decisions

Auto mode showed "auto mode on" in a real interactive Claude Code session, but the classifier block path was not exercised. [Evidence behind the decisions](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#evidence-behind-the-decisions)
Eight real sessions reached 5 to 49 hours on the operator's machine, and all eight had roots no later server could recover. [Evidence behind the decisions](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#evidence-behind-the-decisions)
Baselines at 56a71845 exited 0 for tsc, oxlint, oxfmt and vitest, while the Rust gates were not re-run because they start real sessions in the operator's home. [Evidence behind the decisions](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#evidence-behind-the-decisions)

## Stages

Stage session-and-wire carries all Rust backend work, run on gpt-6.1-sol through loom-codex-forwarder in 24 units. [### 1. session-and-wire](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#1-session-and-wire)
Stage shell-and-spine sets the visual system, the header, the spine and landing, and the browser journey script. [### 2. shell-and-spine](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#2-shell-and-spine)
Stage pages-and-reading gives Pressure and Run real pages, and chat-redesign makes the chat follow the viewed document. [### 3. pages-and-reading](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#3-pages-and-reading) [### 4. chat-redesign](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#4-chat-redesign)
Integration Verification runs the full suites and journeys in a real browser, and Knowledge Distillation rewrites the planner web shell knowledge. [### Integration Verification](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#integration-verification) [### Knowledge Distillation](../../../plans/feat-planner/DONE-PLAN-feat-planner-13-planner-redesign.md#knowledge-distillation)
