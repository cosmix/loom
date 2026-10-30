---
kind: document
of: doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md
file: sha256:371a4c8cfa98ca3ff4877a1bdec5e85cbd87f5532ffa6468e2ef097c791018c0
cites:
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#before-loom-init": "sha256:1b27b2cf106d5472bbe793b0f563039250718d5742cf32229ebda18ec949d493"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#card-payload": "sha256:fe17d1a64f1e1b95ad9f243621b2e3ae94dd0480a0233acd3680b64aeecedc9f"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#chat-help": "sha256:99f4a999be7f3aa567648c8f18fcd391cca49013a9a6dd0919b00dd4c2f9b8f3"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#decisions": "sha256:ebb7e051f84697789d597a06fe58a532e8a5274ad2445ae8b25bed7cfa429f81"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#goals": "sha256:b56f6a0d55620c8fa4c23c95bb5db70d77feef7d30785f00939c5b0732d3c6b9"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#integration-verification": "sha256:d9d08c9952306e120cb6b0ac9edb5eea1a14d19122cf88a4806c94b36b9afe29"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#knowledge-distillation": "sha256:64e0541e471d74db1f6c4e4e719b18ca46dbeea76cb71c49864e170a25409577"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#overview": "sha256:d54cacbe26b8a3fa4bc5721ae259df55867b13ebe25f8e7ca01a9b5c791dbcba"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#shell-features-reading": "sha256:805baff7f0ae674bf08b859dc77c1dd2cdcc6e6e2805b4d7ed22eac82b393bec"
  "doc/plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#summary-engine": "sha256:1e5d471fc142cdff279c26d3610872c00dfef1d795a4be0185dc82b5e7040094"
---

# Repair the planner web app to the agreed design

## Overview

The planner redesign shipped a UI that looks nothing like its locked mockup and misbehaves in places. [Overview](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#overview)
This plan rebuilds every screen to the transcribed mockup and changes summary generation to haiku at high effort. [Overview](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#overview)

## Goals

Every screen must match the mockup, and the browser-tour defects listed under goals are fixed. [Goals](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#goals)
Users can switch between a feature's steps from the rail cells and the feature screen. [Goals](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#goals)
Summaries default to haiku/high, opening prompts are submitted reliably, and summaries can run on Codex. [Goals](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#goals)
Non-goals are the summary engine's hashing and sealing, the session sandbox and the access model. [Goals](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#goals)

## Before `loom init`

The renamed feature directories and this plan must be committed together with the brief bundle. [Before loom init](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#before-loom-init)

## Decisions

The home screen drops its chat panel, and the header becomes one app bar with the run strip, changed-since and summary status moved into the activity popover. [Decisions](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#decisions)
Document hues stay `--doc-1..8`, help type stays at 13px or more, and the pressure dialog states no minutes. [Decisions](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#decisions)

## Stages

The card-payload stage adds title, pitch, count and step-label fields and the screenshot script. [card-payload](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#card-payload)
The summary-engine stage adds haiku/high defaults, three new plan keys, the opening re-press fix and a Codex spawner. [summary-engine](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#summary-engine)
The shell, features and reading stages run concurrently over disjoint web files. [shell, features, reading](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#shell-features-reading)
The chat-help stage covers the chat panel, the pressure dialog and help. [chat-help](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#chat-help)
Integration verification runs the suites and a real-browser review in four themes. [Integration Verification](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#integration-verification)
Knowledge distillation records the planner-ui to feat-planner path corrections, the new header, rail, reading and help rules, the summary defaults, backend and opening fix, and a correction to the redesign mistake entry. [Knowledge Distillation](../../../plans/feat-planner/IN_PROGRESS-PLAN-feat-planner-12-ui-repair.md#knowledge-distillation)
