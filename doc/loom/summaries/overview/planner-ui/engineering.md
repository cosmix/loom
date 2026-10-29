---
kind: overview
of: overview/planner-ui/engineering
cites:
  "doc/plans/planner-ui/index.md#1-document-model-and-project-checklist-plan-feat-planner-01-documentsmd": "sha256:2d0a45ed5df49eda815d8d633d75af8a6f81453207d3d1ddec110327512f3c79"
  "doc/plans/planner-ui/index.md#10-summary-rebuilds-and-cost-plan-feat-planner-10-summary-rebuildsmd": "sha256:36aef50a7f33f8cffb9bc3d563b8f16684fcd853a8650d07db35ab139c36c572"
  "doc/plans/planner-ui/index.md#11-execution-handoff-plan-feat-planner-11-execution-handoffmd": "sha256:c89d2d12fd0eb8b924390b9f10f552ea19f712938db1cd655534d406b4b90eaa"
  "doc/plans/planner-ui/index.md#2-server-foundation-and-access-plan-feat-planner-02-servermd": "sha256:a7b8a4fe1af4605ac8d5214a72c7c6862c0f81ee64a6f546d3c499ea4afb029a"
  "doc/plans/planner-ui/index.md#3-file-mode-plan-feat-planner-03-file-modemd": "sha256:f7f62eccb67c7bb4764996e4cfdcf74f99ae2e7a04038d10ba442494d290f355"
  "doc/plans/planner-ui/index.md#4-summary-store-and-cli-plan-feat-planner-04-summary-storemd": "sha256:532364fb2837ddf400c3d0f31aa9e655d06626bd15d57df684d4e3ea269e69e1"
  "doc/plans/planner-ui/index.md#5-concept-mode-plan-feat-planner-05-concept-modemd": "sha256:4ddcb6d5e7e9c639d3d4dc0de2d18beafc5ed05871949e00f69e096c7a3c731d"
  "doc/plans/planner-ui/index.md#6-planning-session-sandbox-plan-feat-planner-06-session-sandboxmd": "sha256:e9575b44054198d24cdb6b37d82b069e4a5753dd2e33e25a461742bb87dd8d54"
  "doc/plans/planner-ui/index.md#7-session-bridge-plan-feat-planner-07-session-bridgemd": "sha256:45ec454d2e0fb7b85c197fe5c27ec113bc4629f2c44a5f525941eb4b3f121020"
  "doc/plans/planner-ui/index.md#8-session-guidance-and-the-loom-plan-plugin-plan-feat-planner-08-guidancemd": "sha256:ec0bb471ef2fc62bde7e94df8f6643b3fd53229d21173d831dff0845c45ec39d"
  "doc/plans/planner-ui/index.md#9-chat-and-the-review-loop-plan-feat-planner-09-chatmd": "sha256:7617cd88e2873db578995d7367f61d19e456dc9a807992b82f0d4543edd203db"
  "doc/plans/planner-ui/index.md#objective-and-binding-constraints": "sha256:fc3c645554bf2f82c688ef1695107ea0cee60bb2cdcf75ddb912900d7956a007"
  "doc/plans/planner-ui/index.md#plan-map": "sha256:37931643933d66da9cff1cccb5a7cfcc5622fdca6d412da87d5fa847a02a335c"
members: {}
---

# Planner UI Engineering Overview

## Programmes

The planner UI programme builds `loom plan` in eleven plans, from the document model through the execution handoff. [Plan map](../../../../plans/planner-ui/index.md#plan-map)

The plans run in the order of the table, because one checkout runs one plan. [Plan map](../../../../plans/planner-ui/index.md#plan-map)

Every plan inherits the constraints that `doc/` files are the truth, sessions use the subscription and nothing outside `doc/` changes. [Objective](../../../../plans/planner-ui/index.md#objective-and-binding-constraints)

## Planner foundations

The document model plan builds a library that discovers `doc/`, classifies documents and computes the project checklist as a pure function of the tree. [Plan 1](../../../../plans/planner-ui/index.md#1-document-model-and-project-checklist-plan-feat-planner-01-documentsmd)

The server plan moves the dashboard's server parts into a shared module so bare `loom plan` can start an authenticated server beside it. [Plan 2](../../../../plans/planner-ui/index.md#2-server-foundation-and-access-plan-feat-planner-02-servermd)

File mode adds document, tree, save, diff and search routes, validates plan saves with the code behind `loom plan verify`, and never renders raw HTML. [Plan 3](../../../../plans/planner-ui/index.md#3-file-mode-plan-feat-planner-03-file-modemd)

The summary store plan builds the format, evaluator and commands that let agents seal summaries without any session. [Plan 4](../../../../plans/planner-ui/index.md#4-summary-store-and-cli-plan-feat-planner-04-summary-storemd)

Concept mode adds the Product and Engineering tabs, where a reader opens an overview sentence into summary sentences and then into inset source passages. [Plan 5](../../../../plans/planner-ui/index.md#5-concept-mode-plan-feat-planner-05-concept-modemd)

## Planner sessions

The sandbox plan gives every planning session a generated settings file and a path-guard hook that fails closed. [Plan 6](../../../../plans/planner-ui/index.md#6-planning-session-sandbox-plan-feat-planner-06-session-sandboxmd)

The bridge plan runs Claude Code sessions in their own tmux servers, builds chat messages from transcripts, and answers permission requests through answer files. [Plan 7](../../../../plans/planner-ui/index.md#7-session-bridge-plan-feat-planner-07-session-bridgemd)

The guidance plan adds the `loom-plan` plugin and generates the brief, opening messages and closing message that each session receives. [Plan 8](../../../../plans/planner-ui/index.md#8-session-guidance-and-the-loom-plan-plugin-plan-feat-planner-08-guidancemd)

The chat plan builds the chat panel and connects review to authoring through attachments, restores and suggestions to the author. [Plan 9](../../../../plans/planner-ui/index.md#9-chat-and-the-review-loop-plan-feat-planner-09-chatmd)

The rebuild plan schedules background summary sessions after each watcher batch and reports their cost from session transcripts. [Plan 10](../../../../plans/planner-ui/index.md#10-summary-rebuilds-and-cost-plan-feat-planner-10-summary-rebuildsmd)

The handoff plan moves the pressure pipeline into a shared module and adds the run dialog that starts `loom init` and `loom run`. [Plan 11](../../../../plans/planner-ui/index.md#11-execution-handoff-plan-feat-planner-11-execution-handoffmd)
