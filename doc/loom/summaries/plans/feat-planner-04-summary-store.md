---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md
file: sha256:12ac63d6bbddd293935d960e627329dc7e715e27562805db4fe3fdc354c30816
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#decisions": "sha256:c6ef875566869daf8ea21ae3a1b03b3afc9a6a0286834f354ebf2bec5512b9d8"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#goals": "sha256:f7149764779a44ba8db409ede706ccb7e74f5dd0f76a1a60d2de2be7cab8041c"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#overview": "sha256:119c110c8649b893318605476caaaa7c39b7e894fa4250b526f0aeb77bb8b971"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#stages": "sha256:febd2142e299b9dd98ac62161f8746c3c74fbac9b9ca9308ae9460d60a5c8867"
---

# Summary store and CLI

## Overview

The plan builds a summary engine without any session: the store under doc/loom/summaries/, the summary file format with sentence ids computed from text, and the evaluator. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#overview)
It also adds four `[plan]` model and effort keys for summary building. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#overview)
The four commands `loom plan summary seal|brief|status|release` work from any terminal without a server. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#overview)

## Goals

The contract is strict, with validation messages that name the offending draft line, deterministic output, and no hash written by an agent. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#goals)
Seal writes through the store lock using a no-follow temp file and a rename anchored at the repository root's descriptor, so a planted symlink cannot redirect it. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#goals)
The evaluator is a pure function of the files plus the claim list. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#goals)
Background rebuilds, claim clearing for ended sessions and cost are out of scope and belong to plan 10. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#goals)

## Decisions

Hashes reuse plan 1's normalized hash and anchors, and overview members hash the newline-joined ordered sentence ids. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Front matter is written by render with keys in a fixed order and no time or model, so two seals of the same draft are byte-identical. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
A sentence line is text followed by trailing links, and lists, tables, fences, indented code, block quotes, HTML and level-3+ headings are rejected. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Writes use exclusive no-follow creates and renames inside the store lock, and claims are files created with O_EXCL. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#decisions)
Renames need no agent: when the recorded file hash matches the source's current hash, the engine rewrites the path, the cites keys and the body links. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#decisions)

## Stages

The plan has one implementation stage, summary-store, because format, evaluation, engine, commands and config are exercised together. [Stages](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#stages)
That stage carries the contract surface and a risk checklist, and a second stage adds the settings dialog caption. [Stages](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#stages)
Integration verification runs the full gate and a security review against section 12, and knowledge distillation records the summary format. [Stages](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-04-summary-store.md#stages)
