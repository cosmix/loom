---
kind: document
of: doc/plans/DONE-PLAN-planner-issue-fixes.md
file: sha256:dc7f09a08dd64f04c20ad875706a661b15ccd7378406eff081fff786bd913bc3
cites:
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#1-backend-fixes-rust": "sha256:461ef17482c799338049816efe15a9dc0a1defd6acf08531d7fbf19bdd25b2e8"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#2-reading-and-summaries-web": "sha256:9fbc39b0a1007d966c09ada41fef930fdab83bfba0957bded31234f6bdbcef69"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#3-chat-and-activity-web": "sha256:afaf7d4888e683351e73ab938244e8f1d3066d24956bf609b204999930364c5b"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#after-the-run-operator-checks": "sha256:13a1799218b10eefcc1dafc7a95045b2aaf41b1876a7bf3106e56f1746ecb232"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#baselines-at-1e6b2795-main-checkout": "sha256:e2aff9da5dff61711f3c94b44d2aa9f90cd84efa31d2034b890549e11094aab7"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#environment-inventory": "sha256:06c7f8fc806d93c41ddc5ae4cf817d1dd1e93934b916f778b8eaed8b3b4b2b25"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#evidence-behind-the-diagnoses": "sha256:c38a63babf009db79bc5ef234629083cba87430ecbe0d68f8fee596c13f5557a"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#execution-diagram": "sha256:ed1d96e4141a85301382305a1615f35c3a45b3b901351aa02157501439d3285f"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#goals": "sha256:af1bce7429ab030342f62d3f300dab80e0189aaa839b90d703ed616aba68f03c"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#integration-verification": "sha256:562abf8790c2ef59e910455ab27cc7011b8d20b0af435c4189c5d0223662c11b"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#knowledge-distillation": "sha256:2ea23ac4324e27ee26fb6ab571599a2bb4b68b83cd1d9b7e4e2785161dbddca5"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#operator-decisions-asked-2026-11-03": "sha256:935fde14549c47e13f585e69dc299eb5ceea9414a9bf12a68e606b59ca8439e1"
  "doc/plans/DONE-PLAN-planner-issue-fixes.md#overview": "sha256:74ee207c5afee05521f629d069c3b1f7eda34e24060e8dcd0099678353f113cf"
---

# Planner issue fixes plan

The plan fixes all 18 planner issues the operator reported, with no new wire fields, no new theme system and no regenerated stored summaries. [Overview](../../../plans/DONE-PLAN-planner-issue-fixes.md#overview) [Goals](../../../plans/DONE-PLAN-planner-issue-fixes.md#goals)

The operator decided that a live chat's menu offers End chat only, that Changed since you looked is fixed and redesigned, and that the overview model returns to sonnet with a seal check that rejects restatement. [Operator decisions](../../../plans/DONE-PLAN-planner-issue-fixes.md#operator-decisions-asked-2026-11-03)

## Diagnoses

Issues #4 and #5 share one cause, because Claude Code 2.1.316 wraps a pasted prompt in a pasted_content tag that breaks the parser's text matching and the mapper's sent-item replacement. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-issue-fixes.md#evidence-behind-the-diagnoses)

Issue #16 is measured in the cost ledger, where every row carries the background trigger and none carries button, so a press either never starts its own session or is mis-attributed. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-issue-fixes.md#evidence-behind-the-diagnoses)

Measured over the stored files, 18 overview-to-member sentence pairs score at least 0.75 under word-set Dice similarity, and one pair at 0.78 still restates its member. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-issue-fixes.md#evidence-behind-the-diagnoses)

Issue #6 comes from the run strip counting data.merge.merged, which depends on git ancestry, while the dashboard counts each stage's merged flag. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-issue-fixes.md#evidence-behind-the-diagnoses)

Issue #14 comes from the activity popover listing the run and also mounting the run strip in its footer. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-issue-fixes.md#evidence-behind-the-diagnoses)

Issue #15 comes from changed-since diffing from the checkpoint commit every time, while acknowledge advances only the file's hash. [Evidence behind the diagnoses](../../../plans/DONE-PLAN-planner-issue-fixes.md#evidence-behind-the-diagnoses)

## Baselines

At commit 1e6b2795 the web typecheck, lint and format checks exit 0, vitest passes 3330 tests in 271 files under src/plan, and the Rust clippy and fmt checks exit 0. [Baselines at 1e6b2795](../../../plans/DONE-PLAN-planner-issue-fixes.md#baselines-at-1e6b2795-main-checkout)

## Stages

The backend-fixes stage runs beside the two web stages and owns every Rust file the issues touch, split across workers B1, B2 and B3. [1. backend-fixes (Rust)](../../../plans/DONE-PLAN-planner-issue-fixes.md#1-backend-fixes-rust) [Execution Diagram](../../../plans/DONE-PLAN-planner-issue-fixes.md#execution-diagram)

The reading-and-summaries stage covers issues #2, #8, #9, #10 and #18 through workers O2 and V1. [2. reading-and-summaries (web)](../../../plans/DONE-PLAN-planner-issue-fixes.md#2-reading-and-summaries-web)

The chat-and-activity stage covers issues #3, #4, #6, #7, #11, #12, #13, #14, #15 and #17, and it depends on the reading-and-summaries stage. [3. chat-and-activity (web)](../../../plans/DONE-PLAN-planner-issue-fixes.md#3-chat-and-activity-web)

Integration verification runs the full suites and the browser journeys, then walks the 18 issues one by one with evidence before it commits the rebuilt bundle last. [Integration Verification](../../../plans/DONE-PLAN-planner-issue-fixes.md#integration-verification)

Knowledge distillation rewrites the planner web shell and summary store knowledge and records the paste-wrapper lesson in the mistakes files. [Knowledge Distillation](../../../plans/DONE-PLAN-planner-issue-fixes.md#knowledge-distillation)

## After the run

The operator checks the New chat opening, the Rebuild start and the next sonnet overview rebuild, because a stage has no Claude login and so cannot run real chat, rebuild timing or an overview rebuild. [After the run (operator checks)](../../../plans/DONE-PLAN-planner-issue-fixes.md#after-the-run-operator-checks) [Environment inventory](../../../plans/DONE-PLAN-planner-issue-fixes.md#environment-inventory)
