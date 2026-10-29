---
kind: document
of: doc/plans/DONE-PLAN-feat-planner-08-guidance.md
file: sha256:2c1a786f34b3cf74ffbf93fd2d78490e7fe778963d9083a2daf7be6656a40c55
cites:
  "doc/plans/DONE-PLAN-feat-planner-08-guidance.md#1-guidance-engine": "sha256:ec5c8819b89c3c981f5c23eb7eba40ee576445e5e4508ae6d926290839d7f267"
  "doc/plans/DONE-PLAN-feat-planner-08-guidance.md#2-integration-verify": "sha256:2e567361414ec7339f2b25811cacc11fdedad8c0604575fb48bc1d9a2bf4bee5"
  "doc/plans/DONE-PLAN-feat-planner-08-guidance.md#3-knowledge-distill": "sha256:534aac45f345497672ef12c2638a8c3a59786876b58bcf0831442b779e8bf0bd"
  "doc/plans/DONE-PLAN-feat-planner-08-guidance.md#goals": "sha256:f89cf2f2f349fbb6ce0aeb58129c4f908abf91fe06489de374a5509a9f3bc3fa"
  "doc/plans/DONE-PLAN-feat-planner-08-guidance.md#overview": "sha256:a2f5acd42c01a1abc94f9c1658a6b1abcf090d37b153dfe7b63c5746723a9a1e"
---

# Session guidance and the loom-plan plugin

## Overview

The plan embeds the text every planning session receives at launch into the loom binary, renders it per session kind, and switches the session launch to the rendered guidance. [Overview](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#overview)
It depends on plans 1, 4, 6 and 7, and plans 9, 10 and 11 start their sessions through it. [Overview](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#overview)

## Goals

It adds a plan-plugin directory at the repository root with the loom-plan manifest, nine skills, and the summary and overview agents. [Goals](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#goals)
The pressure commands are taken from the embedded commands table at render time, with no second copy. [Goals](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#goals)
The guidance module renders a per-kind plugin and a brief.md into each session directory, along with the opening message for every offer. [Goals](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#goals)
It serves each session's brief over the session brief route for the chat's Brief link. [Goals](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#goals)
A config-block file in the runtime directory makes every session kind refuse to launch while it exists. [Goals](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#goals)
The plugin is embedded as an asset group and is never installed by the install_all step. [Goals](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#goals)
Launching sessions, chat rendering of messages, the summary scheduler and pressure runs are non-goals. [Goals](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#goals)

## Stages

Stage guidance-engine writes the plugin sources, the Rust renderer, the authoring keys and the wiring, then runs the gate with a code review round. [guidance-engine](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#1-guidance-engine)
Its workers run in waves, and the content and Rust workers share one wave because the Rust build needs only the plugin manifest. [guidance-engine](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#1-guidance-engine)
Stage integration-verify runs the full suite and proves that a bridge-launched planning session gets its rendered brief and plugin. [integration-verify](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#2-integration-verify)
Operator testing of live sessions is deferred to a follow-up after merge and is not a gate. [integration-verify](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#2-integration-verify)
Stage knowledge-distill records the plugin layout, render rules and authoring keys in the knowledge base. [knowledge-distill](../../../plans/DONE-PLAN-feat-planner-08-guidance.md#3-knowledge-distill)
