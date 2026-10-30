---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md
file: sha256:a7ed78f50e83431a3bbc26c79027944708a7a922cac5370a1182a30726bb51fd
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces": "sha256:df1b5b74ae35962d60fc5988d2b5bfdfbde8f3dec2272b50086376b789ddd7a3"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#decisions": "sha256:ce56b06bf058a5cb4f2152e59c9cbd9c5b4bed62dda0c2caf2364d3792dc4ba0"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#execution-diagram": "sha256:1752afdd6c3c7fb22e50e3c8971b76ce8c5f717c5d6c6438b4db6079817b4299"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#goals": "sha256:657776e550ce82c687d44526d8bbc5ee5d5da6ce4d548152cd9d8dfec2cd67b2"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#overview": "sha256:cdf39b74c9f7fc5676c8906e68fc120809fbdf54e1fa9c82cef8e84b459da6b4"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#stages": "sha256:2274760d023b9cf92978a0c8504a75990e7ee32bef1a7499946c44fd4d261e29"
---

# Chat and the review loop

## Scope

This plan builds the `loom plan` chat panel and connects review to authoring, in four stages with two feature stages and two bookends. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#overview)

The panel docks on every page and covers message rendering, tool rows, the state line, permission cards, the composer, turn footers, and starting and ending chats. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#overview)

The server side adds Restore and Restore all through plan 3's save path, Send to author suggestions, open-question counts, hashes and text at a git ref, the changed-after-built marker, and per-turn changed sections. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#overview)

A reviewer should attach passages, send a doubt, read the reply and footer, open diffs, and restore a wrong edit without a session. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#goals)

Every state, item and outcome must render with words rather than colour alone, and transcript, permission and suggestion content is never inserted as HTML. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#goals)

Session lifetime, the queue, briefs, summary rebuild scheduling, pressure runs and the Share popover are non-goals owned by other plans. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#goals)

## Interfaces

The plan depends on plans 3, 5, 7 and 8, and it publishes extension points that plans 10 and 11 fill, such as the last block of every turn footer. [Cross-plan interfaces](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces)

The attachment block is a parse contract: plan 7 owns the Rust format and parse, and the browser carries a TypeScript port tested byte for byte against plan 7's fixtures. [Cross-plan interfaces](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces)

Wire fields named `path` are repo-relative with a `doc/` prefix, while plan 3's request parameters and `/files/` addresses are relative to `doc/`. [Cross-plan interfaces](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#cross-plan-interfaces)

## Decisions

Restore is an HTTP save that takes each file's text from the turn-start pin and its opened hash from the turn-end pin, never from the request, and it is refused while a turn runs. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#decisions)

Suggestions are WebSocket commands that reach only the holder's connections, with a note limit of 1,000 characters, at most 20 pending, and refusal when no holder exists. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#decisions)

Changed-since-you-looked keeps its checkpoint in local storage, and the server validates a typed ref and resolves it to a commit before any later git call. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#decisions)

The changed-after-built marker reads the plan's run review and its generated stamp, shows a fixed note for a running plan, and shows nothing for a done plan without a review. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#decisions)

The chat model's browser side belongs to this plan, with zod schemas over plan 7's and plan 8's fixtures and vitest tests in place of a browser harness. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#decisions)

## Stages

The stages are chat-server for the Rust review routes, chat-web for the panel and review UI with a rebuilt bundle, integration-verify and knowledge-distill. [Stages](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#stages) [Execution Diagram](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-09-chat.md#execution-diagram)
