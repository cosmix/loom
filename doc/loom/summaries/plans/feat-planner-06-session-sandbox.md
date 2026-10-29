---
kind: document
of: doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md
file: sha256:f95fcda0871fa88a9cd4685332c9df03fd14dff5e582e2d25535af38d0b673a9
cites:
  "doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions": "sha256:8b8b65c03da7cd873cb2682d68360c716c57a525bfa0abe17d9fb3f6fdeffd46"
  "doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md#environment-inventory": "sha256:9d0f19d38075a06bc75ca2bae5ed7b3dfe3c72d365c6c6f6170b7cf1de09daad"
  "doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md#goals": "sha256:0a291e355536a3cab4578ff78d5a061d4b898cd0a89d38ef68f6eab35d5c2776"
  "doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md#integration-verification": "sha256:6089279240ff596eed42a8ee2d04f30abaf79918dfabfad9f5ced404cb928cd2"
  "doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md#knowledge-distillation": "sha256:537478f6443d0a63d6ca1faa4e21d49382f96258beab7b777bc04c46073a3633"
  "doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md#overview": "sha256:ed2d55ba05890d28377ea9a9405c1a0d819235f91d1c04a73e4830b40cc4a530"
  "doc/plans/DONE-PLAN-feat-planner-06-session-sandbox.md#stages": "sha256:2a53d5b2d5a79bd5a86b8d36ae17d955be94074cef6cae5321f8feb5d32491c6"
---

# Planning session sandbox

## Overview

Each planning session that `loom plan` starts (authoring, summary and pressure) runs under a settings file loom writes for it and a Rust path guard that Claude Code calls before every file-tool use. [overview](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#overview)

The plan builds both pieces so they can be tested before any session exists. [overview](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#overview)

## Goals

One builder produces the settings for all three session kinds, and every file it writes must pass an assertion that refuses bypass mode, unsandboxed escape and network access. [goals](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#goals)

The guard never allows a call; it denies or stays silent, so Claude Code's permission rules and the holder's prompt still apply. [goals](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#goals)

## Decisions

The builder sits beside the sandbox settings code and does not call the general settings generator, which would grant cache writes, orchestration reads and plan config to a planning session. [decisions](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

The guard's record is `guard.json`, written 0600 in the session directory, and the session cannot read or write it because `~/.loom/plan` is denied in every layer. [decisions](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

Summary sessions deny every `doc/` entry except `loom`, and every `doc/loom/` entry except `summaries`, so only drafts and seal remain writable. [decisions](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

Reads are matched on the lexical path and its canonical form, and any `/proc` or `/dev/fd` path is denied. [decisions](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

The hook command is `loom plan hook EVENT --session-dir DIR`, and an unknown or mismatched event exits 0 silently. [decisions](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

The guard prints only denies, because an allow answer would skip Claude Code's permission rules for that call. [decisions](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

The confinement test is an integration test that skips with its reason where `bwrap`, `socat` or `srt` is missing. [decisions](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

## Environment inventory

`srt` (`@anthropic-ai/sandbox-runtime` 0.0.78), `bwrap` and `socat` must be on the host PATH before `loom run`. [environment-inventory](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#environment-inventory)

## Stages

The planning-sandbox stage builds the settings builder, the guard, the hook command and the confinement test in five waves of Claude and Codex workers. [stages](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#stages)

Integration verification runs the full gate, a security review against section 12 and an operator run of the confinement test on the host. [integration-verification](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#integration-verification)

Knowledge distillation records the planning sandbox as a tier-2 topic linked from the architecture file. [knowledge-distillation](../../../plans/DONE-PLAN-feat-planner-06-session-sandbox.md#knowledge-distillation)
