---
kind: document
of: doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md
file: sha256:2d987a8f71ee8f2631719f8ff751a9f019c512d48e98ff205a2acdef745d6268
cites:
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#1-file-server": "sha256:87186e6b769dc836431e652daf12da78209d795874f2065127aa70e8783050cf"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#2-file-reader-web": "sha256:9f5069ac04be12fa892be2ca6ab4445553b004a974f662a2c09126a22771748d"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#3-file-editor-web": "sha256:4bd4180bfee2b9cdf514942afad411eb0e3f22927305734e1434c69603b53049"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#4-integration-verification": "sha256:8b560fe1d9e6eda45dce770bc5bb8cb27b133c7b89869f965925dd8a5e64552c"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#5-knowledge-distillation": "sha256:481a48563327a04ecb008a3198cbdee1963e532a2e8c476f567e7be3e4585873"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#decisions": "sha256:1cba612980bcc85b4b42700653c2855a12bc0e87dba799103323a64ed02a2932"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#environment-inventory": "sha256:f897c2a65f0e6d4186e36b98152fdc2025c30a51962d82268ae01803042fc469"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#goals": "sha256:36aeb4aba452aabde0bcae3af5f9a6ca9314f18940ed00b5da47a6fa08c57a1c"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#overview": "sha256:a3807dbdf2a00f13900ab96c916b65b8aee4d51f98d86f5ea5b1dfd7736bef19"
---

# loom plan file mode

## Overview

File mode is the planning app's reader and editor, reading every discovered document and saving Markdown under the product, plans and other areas. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#overview) [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#goals)

The server gains document, tree, save, diff and search routes, a text store with per-turn pins, and in-process plan validation. [Overview](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#overview)

Plan saves are validated by the same code as loom plan verify, and loom plan verify --json stays byte-identical. [Goals](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#goals)

## Decisions

The renderer builds React elements from an mdast tree, so the parser produces data and never raw HTML. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

Atomic save uses a new fs::safe_replace that walks without following links from a root to the target's directory. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

Saves never create, rename or delete documents, and save_document refuses a missing target. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

Search recomputes BM25 per query over in-memory chunks and writes no index. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

The server's only git read is git show of a HEAD revision of the path, taken after the path passes syntax checks and discovery. [Decisions](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

## Stages

The file-server stage adds the server routes and splits loom plan verify into a function over proposed text. [1. file-server](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#1-file-server)

The file-reader-web stage adds the tree, the safe Markdown renderer, the plan view, search and Add to chat. [2. file-reader-web](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#2-file-reader-web)

The file-editor-web stage adds the lazy CodeMirror editor, drafts, save conflicts and the three diff kinds. [3. file-editor-web](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#3-file-editor-web)

Integration verification runs the full gate, checks that CodeMirror stays out of both initial bundles, and reviews security. [4. Integration verification](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#4-integration-verification)

Knowledge distillation records file mode in the plan-server architecture and the web conventions. [5. Knowledge distillation](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#5-knowledge-distillation)

## Environment

Implementation needs registry.npmjs.org for bun add and no cargo dependency. [Environment inventory](../../../plans/planner-ui/DONE-PLAN-feat-planner-03-file-mode.md#environment-inventory)
