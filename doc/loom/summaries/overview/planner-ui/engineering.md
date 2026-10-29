---
kind: overview
of: overview/planner-ui/engineering
cites:
  "doc/plans/planner-ui/index.md#plan-map": "sha256:37931643933d66da9cff1cccb5a7cfcc5622fdca6d412da87d5fa847a02a335c"
members:
  "plans/feat-planner-01-documents": "sha256:135d955e88cf6224b6ad14e25679637f46bf59dba2ac68c464db4268778fd051"
  "plans/feat-planner-02-server": "sha256:5930b986f5466de6b85d0b988bc39edf5070ffc3bf6cf80579cd5f06369e568d"
  "plans/feat-planner-03-file-mode": "sha256:f3c1da21fe9a825a2e612f2ef9594b5df220b856e3de1d22dcc282bb260e5635"
  "plans/feat-planner-04-summary-store": "sha256:b2afacdf2c5b0d11818fd2f0819a5a87fa4f5a5c0708f85e5d5e4a776dfe1b40"
  "plans/feat-planner-05-concept-mode": "sha256:6703215b85444a0231b577b284fb0f5007f165b44df2e6cb8f051179b0e05e64"
  "plans/feat-planner-06-session-sandbox": "sha256:e915ace9b95c9fe1c5a3994e833f02b8c5cadf39d74324fdc425d2dc2dcc7aa7"
  "plans/feat-planner-07-session-bridge": "sha256:83c537c824fb5f2a4c24a0d1ae91f6895ec51793fa1de88c6b6332c3cf32b514"
  "plans/feat-planner-08-guidance": "sha256:40fecc09aa58daf04a9c0be41b32e40e997241c8f7d3b46214d6156d66dc6795"
  "plans/feat-planner-09-chat": "sha256:9b5f35fe8e7b1abd3aeababf8fb9156b39e0ce91b0db7fd33fbb106df38d471f"
  "plans/feat-planner-10-summary-rebuilds": "sha256:cdf341b05497c9700ec9cd7aabd12b37d69c3a492eab7decf4009f5810d69aa0"
  "plans/feat-planner-11-execution-handoff": "sha256:adfffbf85ad0801e991ac4dfcd7fa146ef76e7b155072576b6ccc9dad981cb25"
---

# Planner UI Engineering Overview

## Programmes

The planner UI programme builds `loom plan` in eleven plans, from the document model through the execution handoff. [Plan map](../../../../plans/planner-ui/index.md#plan-map)

## Planner foundations

The document model plan builds a library that discovers `doc/`, classifies documents and computes the project checklist as a pure function of the tree. [feat-planner-01](../../plans/feat-planner-01-documents.md#s-f5d84e59) [feat-planner-01](../../plans/feat-planner-01-documents.md#s-ec2d49ec)

The server plan moves the dashboard's server parts into a shared module so bare `loom plan` can start an authenticated server beside it. [feat-planner-02](../../plans/feat-planner-02-server.md#s-437c9443) [feat-planner-02](../../plans/feat-planner-02-server.md#s-6a73d983)

File mode adds document, tree, save, diff and search routes, validates plan saves with the code behind `loom plan verify`, and never renders raw HTML. [feat-planner-03](../../plans/feat-planner-03-file-mode.md#s-dcfce43b) [feat-planner-03](../../plans/feat-planner-03-file-mode.md#s-ebc24f4f) [feat-planner-03](../../plans/feat-planner-03-file-mode.md#s-09e64ab2)

The summary store plan builds the format, evaluator and commands that let agents seal summaries without any session. [feat-planner-04](../../plans/feat-planner-04-summary-store.md#s-9bab6b36) [feat-planner-04](../../plans/feat-planner-04-summary-store.md#s-7f5fa71a)

Concept mode resolves each page on the server and lets a reader follow citations from overview to summary to source passage. [feat-planner-05](../../plans/feat-planner-05-concept-mode.md#s-2bb17d7f) [feat-planner-05](../../plans/feat-planner-05-concept-mode.md#s-a5e0568a)

## Planner sessions

The sandbox plan gives every planning session a generated settings file and a guard that only denies or stays silent. [feat-planner-06](../../plans/feat-planner-06-session-sandbox.md#s-79bbe3b8) [feat-planner-06](../../plans/feat-planner-06-session-sandbox.md#s-fb56aaa7)

The bridge plan runs interactive Claude Code sessions in tmux, builds chat messages from transcripts, and answers permission requests through files the session cannot write. [feat-planner-07](../../plans/feat-planner-07-session-bridge.md#s-42bcb2b9) [feat-planner-07](../../plans/feat-planner-07-session-bridge.md#s-cfee483b)

The guidance plan embeds the brief, opening message and `loom-plan` plugin that every planning session receives at launch. [feat-planner-08](../../plans/feat-planner-08-guidance.md#s-c3f6885b) [feat-planner-08](../../plans/feat-planner-08-guidance.md#s-98ce171b)

The chat plan builds the panel that connects review to authoring, with attachments, restores and suggestions to the author. [feat-planner-09](../../plans/feat-planner-09-chat.md#s-cd9f03a4) [feat-planner-09](../../plans/feat-planner-09-chat.md#s-94bc3646)

The rebuild plan schedules background summary sessions after each watcher batch and reports their cost from the shared token accumulator. [feat-planner-10](../../plans/feat-planner-10-summary-rebuilds.md#s-a49fccd0) [feat-planner-10](../../plans/feat-planner-10-summary-rebuilds.md#s-4950b4fd)

The handoff plan moves the pressure pipeline into a shared library and adds the run dialog that starts `loom init` and `loom run`. [feat-planner-11](../../plans/feat-planner-11-execution-handoff.md#s-147c54f6) [feat-planner-11](../../plans/feat-planner-11-execution-handoff.md#s-410fd509)
