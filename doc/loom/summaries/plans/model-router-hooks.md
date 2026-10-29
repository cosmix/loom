---
kind: document
of: doc/plans/PLAN-model-router-hooks.md
file: sha256:5a98fdd84803080db21fbf8a1e45f29f92ce99b3230ecdfdba5e56a31df466d6
cites:
  "doc/plans/PLAN-model-router-hooks.md#execution-diagram": "sha256:e910b2d8896eb3d44cbcbdc09d68d5d21ab3169c96d1d71f148dd09bf2753f87"
  "doc/plans/PLAN-model-router-hooks.md#goals": "sha256:4676e4af7f793614f808659f5bd0260c50d79e222e8f4086a82320cb7e909bbd"
  "doc/plans/PLAN-model-router-hooks.md#overview": "sha256:a48615110f7338a16ffbbc2e27ebe8ffb2a6fa3bbf410cac1522cf52e10f2f10"
  "doc/plans/PLAN-model-router-hooks.md#security-posture-baked-into-every-stage": "sha256:7579a67355d72656e9c87a9c42ded1b8336e5bab4c7691b9a38d59f5c2930bee"
  "doc/plans/PLAN-model-router-hooks.md#stages": "sha256:3197c14e503d165c00ecee94d9911ff6add7fc61507e9707baf260db068a277e"
---

# Model Router Hooks Plan

## Overview

The plan adds an opt-in, fail-open routing system that uses a local OpenAI-compatible classifier to nudge Claude Code toward the right subagent and thinking effort. [Overview](../../../plans/PLAN-model-router-hooks.md#overview)

Two global hooks run on UserPromptSubmit and on PreToolUse for the subagent-spawn tool, and neither ever blocks or rewrites parameters. [Overview](../../../plans/PLAN-model-router-hooks.md#overview)

## Goals

The router stays disabled until an endpoint and model are configured, and every failure path exits 0 with no output. [Goals](../../../plans/PLAN-model-router-hooks.md#goals)

It routes between loom-software-engineer (Sonnet) and loom-senior-software-engineer (Opus) with a reasoning_effort recommendation biased by cost. [Goals](../../../plans/PLAN-model-router-hooks.md#goals)

## Security posture (baked into every stage)

Classifier output is untrusted, so injected context comes only from hard-coded templates keyed by parsed enums, with rationale as a truncated labeled note. [Security posture](../../../plans/PLAN-model-router-hooks.md#security-posture-baked-into-every-stage)

The hook uses Python stdlib only and never interpolates prompt or transcript text into a shell. [Security posture](../../../plans/PLAN-model-router-hooks.md#security-posture-baked-into-every-stage)

## Stages

The plan runs five worktree stages: knowledge-bootstrap, router-core, wiring, integration-verify, and knowledge-distill. [Execution Diagram](../../../plans/PLAN-model-router-hooks.md#execution-diagram)

Knowledge bootstrap verifies the subagent-spawn tool name, expected to be Task, and the global-hook install pipeline before implementation. [Stages](../../../plans/PLAN-model-router-hooks.md#stages)

Router core builds the hook script and the loom router CLI with two parallel subagents that share one config contract. [Stages](../../../plans/PLAN-model-router-hooks.md#stages)

Wiring registers model-router.py in constants.rs and hooks.rs on both hook events. [Stages](../../../plans/PLAN-model-router-hooks.md#stages)

Integration verification requires a stub-server end-to-end run, build, test, clippy, and code review. [Stages](../../../plans/PLAN-model-router-hooks.md#stages)

Knowledge distillation curates the routing invariants into knowledge and updates README and CONTRIBUTING. [Stages](../../../plans/PLAN-model-router-hooks.md#stages)
