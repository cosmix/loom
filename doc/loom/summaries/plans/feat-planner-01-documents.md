---
kind: document
of: doc/plans/DONE-PLAN-feat-planner-01-documents.md
file: sha256:588de01ffd7b739ced9db7be236ffdcf5d6b9ce37d29d36ded148d5e07c37cd2
cites:
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#1-document-model-discovery-kinds-slugs-sections-predicate-watcher": "sha256:0462afe1c8a0e138c771743c73badeea79d9ab52e128e401bd5de2ca893ba30e"
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#2-checklist-check-parsers-project-view-checklist-loom-plan-check": "sha256:85e309accb2744971c73fd4d4a0b72f66e11991defda7faa1370881b08757a11"
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#decisions": "sha256:c9bd91d40a9057520ae29b78f7e77fa7d528d490d3e701557fe9b91a54a64c36"
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#execution-diagram": "sha256:49157c78d683a806939ea8c08bfe0036663e34d24512d1e6ad97f6191ca835df"
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#goals": "sha256:66cb5a6dbdf81b461fb71b9042eb1eddd619db2e83a031153085196b1b22c68a"
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#integration-verification": "sha256:5f477f97fc20357fa6c1358a0cf81a08515282f5466d8ff96f9a9e8056171c34"
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#knowledge-distillation": "sha256:10586aab338fe854d8b73c0b1174804747db05975e2839fb223ec0388d58d6ca"
  "doc/plans/DONE-PLAN-feat-planner-01-documents.md#overview": "sha256:b652c460dcb11659c1fee074173c319a4d789000dc0bdc505b796cb38cf58ced"
---

# Document model and project checklist

## Purpose

The plan builds the `loom::planning` library that discovers `doc/`, classifies its documents, resolves plan slugs, splits documents into sections and watches the tree, and it adds the `loom plan check` command. [Overview](../../../plans/DONE-PLAN-feat-planner-01-documents.md#overview)

Later planning plans call this library instead of re-deriving document rules, and every public signature is published in the team ledger. [Goals](../../../plans/DONE-PLAN-feat-planner-01-documents.md#goals)

The checklist is a pure function of the document tree, with no app state, front matter or sidecar files. [Goals](../../../plans/DONE-PLAN-feat-planner-01-documents.md#goals)

Server, bare `loom plan`, summaries, rendering and the pressure-tests writer are out of scope for this plan. [Goals](../../../plans/DONE-PLAN-feat-planner-01-documents.md#goals)

## Decisions

Discovery reads with no-follow opens, and a symlink counts as a document only when its target is a regular file inside `doc/` and outside the summary store. [Decisions](../../../plans/DONE-PLAN-feat-planner-01-documents.md#decisions)

Locked status is decided by plan name on every call, and a brief path under a locked plan's bundle counts as locked even when the file does not exist yet. [Decisions](../../../plans/DONE-PLAN-feat-planner-01-documents.md#decisions)

The watcher polls once a second, delivers a batch on the first walk that finds nothing new, and reports a removed and an added path with the same hash as a rename. [Decisions](../../../plans/DONE-PLAN-feat-planner-01-documents.md#decisions)

Span hashes normalise line endings and trailing whitespace before hashing, and `loom plan check` exits 0 with no problem, 1 with any problem and 2 when checking cannot run outside a git repository. [Decisions](../../../plans/DONE-PLAN-feat-planner-01-documents.md#decisions)

Plan metadata anchors come from the parser's byte ranges, and `loom run` reuses its own brief collection code so that the app's brief list matches the list it checks. [Decisions](../../../plans/DONE-PLAN-feat-planner-01-documents.md#decisions)

## Stages

The document-model stage splits its work into four codex units and four Claude units run in three waves. [1. document-model](../../../plans/DONE-PLAN-feat-planner-01-documents.md#1-document-model-discovery-kinds-slugs-sections-predicate-watcher)

The checklist-check stage builds the parsers, project view, checklist and check command in two lanes, and it extends the plan subcommand list assertion with `check`. [2. checklist-check](../../../plans/DONE-PLAN-feat-planner-01-documents.md#2-checklist-check-parsers-project-view-checklist-loom-plan-check)

The integration-verify stage runs the full gate, functional checks on this repository and a fixture project, and three review subagents for security, architecture and test coverage. [Integration Verification](../../../plans/DONE-PLAN-feat-planner-01-documents.md#integration-verification)

The knowledge-distill stage records the planning document model as a tier-2 topic and brings `INDEX.md` under its size budget. [Knowledge Distillation](../../../plans/DONE-PLAN-feat-planner-01-documents.md#knowledge-distillation)

## Execution order

The stages run in the order document-model, checklist-check, integration-verify and knowledge-distill. [Execution Diagram](../../../plans/DONE-PLAN-feat-planner-01-documents.md#execution-diagram)
