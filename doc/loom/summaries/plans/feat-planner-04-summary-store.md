---
kind: document
of: doc/plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md
file: sha256:12ac63d6bbddd293935d960e627329dc7e715e27562805db4fe3fdc354c30816
cites:
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions": "sha256:c6ef875566869daf8ea21ae3a1b03b3afc9a6a0286834f354ebf2bec5512b9d8"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#goals": "sha256:f7149764779a44ba8db409ede706ccb7e74f5dd0f76a1a60d2de2be7cab8041c"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#integration-verification": "sha256:2d03846f38fc27dbb88b9e564801c607ff386b46161928321dc3f2ce433b553d"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#knowledge-distillation": "sha256:0765695d73a609837072f1fb2496c696b5ef1387504a9bfa7d70e34abed56aba"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#overview": "sha256:119c110c8649b893318605476caaaa7c39b7e894fa4250b526f0aeb77bb8b971"
  "doc/plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#stages": "sha256:febd2142e299b9dd98ac62161f8746c3c74fbac9b9ca9308ae9460d60a5c8867"
---

# Plan summary store and CLI

## Overview

The plan builds the summary engine without any session: the store, the file format with sentence ids computed from text, the evaluator, the same-hash rename step and the commands `loom plan summary seal`, `brief`, `status` and `release`. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#overview)
It also adds four `[plan]` model and effort keys for summary building. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#overview)

## Goals

The `loom::planning::summary` module is the contract between agents and the reader, with strict validation that names the offending draft line, deterministic output, and no hash written by an agent. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#goals)
`seal` writes through the store lock using a no-follow temporary file and a rename anchored at the repository root's descriptor, so a planted symlink cannot redirect it. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#goals)
The evaluator is a pure function of the files plus the claim list, which plan 5 renders and plan 10 schedules from. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#goals)

## Decisions

Hashes come from plan 1's `normalized_hash` and anchors from its sections module, including the stage anchors, so a link edit in a document summary leaves overviews current. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Front matter is written by `render` and read with `serde-saphyr`, with a fixed key order, sorted maps, and no time or model, so two seals of the same draft are byte-identical. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Sentence lines are checked by structure rather than sentence-boundary guessing, and lists, tables, fences, indented code, block quotes, HTML and level-3+ headings are rejected. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Seal and the engine write through an exclusive store lock, creating a temporary file with `safe_create_new_in_workdir` and renaming it with `safe_rename_in_workdir`. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Claims are files created with `O_EXCL`, seal deletes only the caller's claim, and release deletes the caller's claims. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Renames need no agent: when a document summary's source moves and its recorded file hash still matches, the engine rewrites the paths. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
The plan has one implementation stage because the format, evaluation, engine, commands and config are exercised together by the contracts. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#decisions)

## Stages

Stage `summary-settings-web` adds the settings dialog caption for the `[plan]` section and runs the web gates over the regenerated config fixture. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#stages)

## Integration

Integration verification runs the full gate, a security review against section 12 of the product spec, and a hand-sealed summary on a fixture project. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#integration-verification)
Knowledge distillation records a tier-2 topic for the summary format and engine and the `[plan]` keys in the model-and-effort convention. [§](../../../plans/planner-ui/DONE-PLAN-feat-planner-04-summary-store.md#knowledge-distillation)
