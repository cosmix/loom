---
kind: document
of: doc/plans/DONE-PLAN-planner-review-fixes.md
file: sha256:d01d836503d0f0b98487ec126dd9a8b609ec57aecbef042e7387f2fc937670b0
cites:
  "doc/plans/DONE-PLAN-planner-review-fixes.md#1-reading-and-vision": "sha256:3d49fdfc61f069edd148d9470b0a73e6e50068e60e5845cba90e83e6d30c160b"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#2-chat-and-attachments": "sha256:979d6127473f03cd84fa6582b626469af3538760057a9f2e0c04b91742940556"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#3-kickoff-and-drafts": "sha256:a717717b98d312698620b72b8cc06c6e25026e4f39b670ff87c307101d3269d8"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#4-home-and-shell": "sha256:a6ee256315f0fc2e3d7df7984bddfe641314df508b5dc5fd60d1eb1b68a3a8d1"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#before-loom-run": "sha256:9c72d149e5b8a0c66b8077f55d9a677684010e0e907ee3316246a141caab2c64"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#evidence-behind-the-diagnoses": "sha256:df888349a56586ee3c16a547f142d68333b5b1b6081bb4136ca0a66f75afc949"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#execution-diagram": "sha256:0af3f2c207b364faa93109bd28cb917c5bf7feb84269f76344d908b4be953a49"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#goals": "sha256:d3c1848062f17dcb4b25daaa03597bbfccbf5909a48c16d5e0be918bc98c861d"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#integration-verification": "sha256:a627e4f11cae274c59fe50bc40d457c07b414f7e1d38b582b900175cd8776797"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#operator-decisions-asked-2026-11-04": "sha256:2e42776b094e3b2a85f25e63c62fb2d4fb8423d076ad415642b89c0f90435731"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#overview": "sha256:e9682855164acf91c025d3b33d8b4086345d82dbdd908a6b205a65fa1ebdf64f"
  "doc/plans/DONE-PLAN-planner-review-fixes.md#sibling-plans": "sha256:35da2c17a839bb187c481e4fa0437f6312ddf332bf472868b49007395efafd28"
---

# Planner review fixes

The plan fixes fifteen issues the operator found when reviewing the planner on 2026-11-04. [Overview](../../../plans/DONE-PLAN-planner-review-fixes.md#overview)

Root causes are fixed at their owner with a test that fails on the old behaviour, and integration verification checks all fifteen issues in the running app, one line per issue with evidence. [Goals](../../../plans/DONE-PLAN-planner-review-fixes.md#goals)

Attachments beyond repository documents are built now, and implementation lanes are Claude only. [Operator decisions](../../../plans/DONE-PLAN-planner-review-fixes.md#operator-decisions-asked-2026-11-04)

Haiku and Sonnet do the development, Opus at medium effort does each visual design, and Fable is used only after Opus fails twice on one item. [Overview](../../../plans/DONE-PLAN-planner-review-fixes.md#overview)

## Diagnoses

The kickoff transcript shows one opening user record that hides the typed sentence behind the guidance wrapper, and each assistant reply ends with a loom-kickoff fence that renders as a code block. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-review-fixes.md#evidence-behind-the-diagnoses)

Issue 3 is a store gap, since summary_id_for gives ids only to sections and plans, and the product spec says the vision has no summary. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-review-fixes.md#evidence-behind-the-diagnoses)

Issue 5 was measured: 7 of 25 member-citing sentences in feat-planner/product and 12 of 27 in feat-planner/engineering cite exactly one member sentence. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-review-fixes.md#evidence-behind-the-diagnoses)

## Stages

Reading-and-vision runs beside chat-and-attachments, kickoff-and-drafts follows chat-and-attachments, home-and-shell follows reading-and-vision and kickoff-and-drafts, and integration verification and knowledge distillation close the run. [Execution Diagram](../../../plans/DONE-PLAN-planner-review-fixes.md#execution-diagram)

Reading-and-vision adds the vision summary, the two-member overview rule, the sources key and a feature source route, with a visual worker at opus medium. [1. reading-and-vision](../../../plans/DONE-PLAN-planner-review-fixes.md#1-reading-and-vision)

Chat-and-attachments fixes the opening split, adds the upload route and the file attachment kind for images, sketches, PDFs and text transcripts, and redesigns the chat panel. [2. chat-and-attachments](../../../plans/DONE-PLAN-planner-review-fixes.md#2-chat-and-attachments) [Operator decisions](../../../plans/DONE-PLAN-planner-review-fixes.md#operator-decisions-asked-2026-11-04)

Kickoff-and-drafts adds the draft store with rename, set aside, continue and discard, plus the New work kind control and the Unfinished shelf. [3. kickoff-and-drafts](../../../plans/DONE-PLAN-planner-review-fixes.md#3-kickoff-and-drafts)

Home-and-shell moves the limits into the footer, removes the share tunnel, rebuilds Home and the header, and makes changes a page. [4. home-and-shell](../../../plans/DONE-PLAN-planner-review-fixes.md#4-home-and-shell)

Integration verification rebuilds the bundle and binary, walks the 15 issues in the browser, and commits the bundle last. [Integration Verification](../../../plans/DONE-PLAN-planner-review-fixes.md#integration-verification)

Before loom run, the operator commits the plan, the briefs and the regenerated summaries, because loom run refuses an uncommitted plan or brief bundle. [Before loom run](../../../plans/DONE-PLAN-planner-review-fixes.md#before-loom-run)

This plan must not run at the same time as its six sibling plans, because whichever runs second starts from the merged tree. [Sibling plans](../../../plans/DONE-PLAN-planner-review-fixes.md#sibling-plans)
