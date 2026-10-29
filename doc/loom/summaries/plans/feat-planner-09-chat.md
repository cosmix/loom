---
kind: document
of: doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md
file: sha256:a7ed78f50e83431a3bbc26c79027944708a7a922cac5370a1182a30726bb51fd
cites:
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#1-chat-server-working_dir-loom": "sha256:b1fbeffee8c990725b44c39cf58006a182bf122b5aa0aa9a4cdaad61a95cf173"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#2-chat-web-working_dir-web": "sha256:6e16d7854d1516632aaf92a5359e811983fb546f431f18eb4d7bd30edb4c4b07"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#3-integration-verify": "sha256:9218689c47c321cb3b97d739edca534a98888f672a088da0fd0f549c72f7a3d2"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#4-knowledge-distill": "sha256:88095d0bc73431610d4e56c4c524b44b28a48ef6f9ee063d8e2c5530bfbf4e7f"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces": "sha256:df1b5b74ae35962d60fc5988d2b5bfdfbde8f3dec2272b50086376b789ddd7a3"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#decisions": "sha256:ce56b06bf058a5cb4f2152e59c9cbd9c5b4bed62dda0c2caf2364d3792dc4ba0"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#goals": "sha256:657776e550ce82c687d44526d8bbc5ee5d5da6ce4d548152cd9d8dfec2cd67b2"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#overview": "sha256:cdf39b74c9f7fc5676c8906e68fc120809fbdf54e1fa9c82cef8e84b459da6b4"
---

# Plan: loom plan chat and the review loop

## Overview

This plan builds the chat panel of loom plan and connects review to authoring, with a browser panel and server-side review routes. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#overview)

The plan depends on plans 3, 5, 7 and 8, and plans 10 and 11 add rows to its turn footer and run header through its extension points. [Cross-plan interfaces](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces)

## Goals

A reviewer attaches passages, writes a doubt, sends it, reads the reply and footer, opens diffs, and restores a wrong edit without a session. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#goals)

Non-goals include session lifetime, the queue's server side, briefs, summary rebuild scheduling, pressure runs and the Share popover. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#goals)

## Cross-plan interfaces

Wire fields named `path` are repo-relative, while request parameters and `/files/` addresses are relative to doc/. [Cross-plan interfaces](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces)

Attachments are formatted by one Rust module, and the browser carries a TypeScript port tested byte for byte against plan 7's fixtures. [Cross-plan interfaces](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces)

## Decisions

Restore is an HTTP save, never a WebSocket command, and it takes each file's text from the turn-start pin rather than from the request. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#decisions)

Suggestions are WebSocket commands with a 1,000-character note limit, at most 20 pending, and delivery only to the holder's connections. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#decisions)

Changed-since checkpoints stay in local storage, and every later git call takes a resolved commit with paths after `--`. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#decisions)

The built marker reads the plan's run review and compares its generation stamp with the section's last change date. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#decisions)

The chat model's browser side belongs to this plan, and pressure runs render through the shared run header and session divider. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#decisions)

## Stages

The chat-server stage adds the review-loop routes under the planning server's review module. [1. chat-server (working_dir: loom)](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#1-chat-server-working_dir-loom)

The chat-web stage builds the panel and review UI under web/src/plan and rebuilds the embedded bundle. [2. chat-web (working_dir: web)](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#2-chat-web-working_dir-web)

The integration-verify stage runs the full Rust and web suites, a threat-model code review and a smoke test of a real server. [3. integration-verify](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#3-integration-verify)

The knowledge-distill stage curates stage memories into the knowledge base and adds a chat paragraph to the README. [4. knowledge-distill](../../../plans/planner-ui/DONE-PLAN-feat-planner-09-chat.md#4-knowledge-distill)
