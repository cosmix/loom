---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md
file: sha256:2d987a8f71ee8f2631719f8ff751a9f019c512d48e98ff205a2acdef745d6268
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#1-file-server": "sha256:87186e6b769dc836431e652daf12da78209d795874f2065127aa70e8783050cf"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#2-file-reader-web": "sha256:9f5069ac04be12fa892be2ca6ab4445553b004a974f662a2c09126a22771748d"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#3-file-editor-web": "sha256:4bd4180bfee2b9cdf514942afad411eb0e3f22927305734e1434c69603b53049"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#4-integration-verification": "sha256:8b560fe1d9e6eda45dce770bc5bb8cb27b133c7b89869f965925dd8a5e64552c"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#5-knowledge-distillation": "sha256:481a48563327a04ecb008a3198cbdee1963e532a2e8c476f567e7be3e4585873"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#decisions": "sha256:1cba612980bcc85b4b42700653c2855a12bc0e87dba799103323a64ed02a2932"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#environment-inventory": "sha256:f897c2a65f0e6d4186e36b98152fdc2025c30a51962d82268ae01803042fc469"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#execution-diagram": "sha256:fc8716d774bd9674c245e99b7b863aed625de29d50b133ebe359cb4bc220d733"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#goals": "sha256:36aeb4aba452aabde0bcae3af5f9a6ca9314f18940ed00b5da47a6fa08c57a1c"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#interfaces-later-plans-consume": "sha256:5e7fc8d3a7ef7379c0006d7633f834cb9bb4902a2dc4454ce57a4f5a49dc74cc"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#interfaces-this-plan-consumes": "sha256:d4b4f2d068b1941d5de863cfe2dce6b6937ce024adee33606aaff85c3b0a7907"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#overview": "sha256:a3807dbdf2a00f13900ab96c916b65b8aee4d51f98d86f5ea5b1dfd7736bef19"
---

# Plan: loom plan file mode

## Scope

File mode is the planning app's reader and editor, adding server document, tree, save, diff and search routes plus in-process plan validation. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#overview)

The browser side adds the navigation tree, a Markdown renderer that never renders raw HTML, the plan view with a Stages panel, search, a lazy CodeMirror editor, three diff kinds and the Add to chat control. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#overview)

Goals are readable documents at a `/files/` route addressed by path and anchor, saves limited to the product, plans and other areas, and `loom plan verify --json` staying byte-identical. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#goals)

CodeMirror must stay out of both initial bundles, and chat, attachment delivery, Restore, concept mode and turn boundaries are non-goals. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#goals)

## Decisions

The renderer takes heading anchors from the server instead of re-implementing GitHub anchor numbering, and builds React elements from an mdast tree so no HTML sanitiser is needed. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

Atomic save needs a new `fs::safe_replace` because no existing helper renames through a no-follow descriptor, and saves never create, rename or delete files. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

Plan saves are validated over the proposed text, a plan without a metadata block validates with a note, and the one server git read is `git show` of the file at `HEAD` with a checked path. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

Search recomputes BM25 per query over in-memory chunks, writes no index, and includes done and archived plans. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#decisions)

## Dependencies

The plan depends on plan 1 (documents) and plan 2 (server) and consumes their planning and plan server interfaces. [Interfaces this plan consumes](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#interfaces-this-plan-consumes)

Later plans consume `verify_text`, `save_document`, the text store pins, `validate_plan`, the search index, the file-mode routes and the web Markdown, attachment and editor modules. [Interfaces later plans consume](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#interfaces-later-plans-consume)

## Stages

The stages run file-server, then file-reader-web, then file-editor-web, then integration verification and knowledge distillation. [Execution Diagram](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#execution-diagram)

The file-server stage splits `loom plan verify` into a function over text, adds the file-mode routes and text store, and is gated by contract tests for refusals such as done plans, symlinks and oversized bodies. [1. file-server](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#1-file-server)

The reader stage adds the tree, the safe Markdown renderer, the plan view and search, and the editor stage adds the lazy CodeMirror editor, drafts, the merge view and the three diffs. [2. file-reader-web](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#2-file-reader-web) [3. file-editor-web](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#3-file-editor-web)

Integration verification runs the full gate and a security review against section 12, and knowledge distillation records the file-mode architecture and the renderer and editor rules. [4. Integration verification](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#4-integration-verification) [5. Knowledge distillation](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#5-knowledge-distillation)

## Environment

Implementation needs only bun installs from the npm registry and no cargo dependency, and no criterion uses credentials. [Environment inventory](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-03-file-mode.md#environment-inventory)
