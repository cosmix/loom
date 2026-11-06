---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md
file: sha256:588de01ffd7b739ced9db7be236ffdcf5d6b9ce37d29d36ded148d5e07c37cd2
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#1-document-model-discovery-kinds-slugs-sections-predicate-watcher": "sha256:0462afe1c8a0e138c771743c73badeea79d9ab52e128e401bd5de2ca893ba30e"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#2-checklist-check-parsers-project-view-checklist-loom-plan-check": "sha256:85e309accb2744971c73fd4d4a0b72f66e11991defda7faa1370881b08757a11"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#decisions": "sha256:c9bd91d40a9057520ae29b78f7e77fa7d528d490d3e701557fe9b91a54a64c36"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#goals": "sha256:66cb5a6dbdf81b461fb71b9042eb1eddd619db2e83a031153085196b1b22c68a"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#integration-verification": "sha256:5f477f97fc20357fa6c1358a0cf81a08515282f5466d8ff96f9a9e8056171c34"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#knowledge-distillation": "sha256:10586aab338fe854d8b73c0b1174804747db05975e2839fb223ec0388d58d6ca"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#overview": "sha256:b652c460dcb11659c1fee074173c319a4d789000dc0bdc505b796cb38cf58ced"
---

# Document model and project checklist

## Overview

The plan builds the `loom::planning` library that discovers `doc/`, names its documents, resolves plan slugs, splits sections, hashes and watches the tree, and adds `loom plan check [--json]`. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#overview)

Plans 2 to 11 call this library and never re-derive its rules. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#goals)

The checklist is a pure function of the tree, with no app state, front matter or sidecar files. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#goals)

`loom plan check` needs no server and exits 1 on any convention problem. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#goals)

## Decisions

The library reads every document through no-follow opens, and a symlink counts as a document only when its resolved target is a regular file inside `doc/` and outside the summary store. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#decisions)

Locked status is decided by name on every call, and a path inside a locked plan's bundle that does not exist yet is locked too. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#decisions)

Briefs come from the `collect_inputs` and `extract_brief_paths` code that `loom run` already uses, so the app and the checker list the same briefs. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#decisions)

The watcher polls once per second, hashes only files that changed, and delivers a batch on the first quiet walk. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#decisions)

## Stages

Stage `document-model` builds the kinds, slugs, discovery, runtime paths, sections, path predicate and watcher, and freezes their signatures before the checklist uses them. [1. document-model](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#1-document-model-discovery-kinds-slugs-sections-predicate-watcher)

Stage `checklist-check` builds the document parsers, the cross-reference resolver, the project view, the checklist and the `loom plan check` command. [2. checklist-check](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#2-checklist-check-parsers-project-view-checklist-loom-plan-check)

Integration verification runs the full gate, three review subagents and functional runs of `loom plan check --json` on this repository and on a fixture project. [Integration Verification](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#integration-verification)

Knowledge distillation records the `loom::planning` architecture as a tier-2 topic and brings the knowledge index back under its size budget. [Knowledge Distillation](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-01-documents.md#knowledge-distillation)
