---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md
file: sha256:f95fcda0871fa88a9cd4685332c9df03fd14dff5e582e2d25535af38d0b673a9
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions": "sha256:8b8b65c03da7cd873cb2682d68360c716c57a525bfa0abe17d9fb3f6fdeffd46"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#environment-inventory": "sha256:9d0f19d38075a06bc75ca2bae5ed7b3dfe3c72d365c6c6f6170b7cf1de09daad"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#goals": "sha256:0a291e355536a3cab4578ff78d5a061d4b898cd0a89d38ef68f6eab35d5c2776"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#integration-verification": "sha256:6089279240ff596eed42a8ee2d04f30abaf79918dfabfad9f5ced404cb928cd2"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#knowledge-distillation": "sha256:537478f6443d0a63d6ca1faa4e21d49382f96258beab7b777bc04c46073a3633"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#overview": "sha256:ed2d55ba05890d28377ea9a9405c1a0d819235f91d1c04a73e4830b40cc4a530"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#stages": "sha256:2a53d5b2d5a79bd5a86b8d36ae17d955be94074cef6cae5321f8feb5d32491c6"
---

# Planning session sandbox

## Sandbox

Every Claude Code session that loom plan starts runs under a settings file that loom writes for it, plus a Rust path guard that Claude Code calls before each file-tool use. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#overview)

The builder sits beside the stage settings code and calls none of its cache-write grants, orchestration reads or plan config, because a planning session must not hold them. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

Every file the builder writes must pass an assertion that rejects bypass mode, a disabled sandbox, unsandboxed escape and network access. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#goals)

Summary sessions write-deny every doc/ entry except loom and every doc/loom/ entry except summaries, so only drafts and seal remain writable. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

Every kind denies the stage capsule's tool denies, and authoring and summary sessions also deny SendMessage and ListAgents. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

The confinement test runs loom's own document commands inside a real sandbox and skips with its reason where bwrap, socat or srt is missing. [Environment inventory](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#environment-inventory)

## Guard

The guard's policy is recorded in guard.json, written 0600 beside settings.json, which the session can neither read nor write. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

The hook command is loom plan hook with an event name and --session-dir, and an unknown or mismatched event exits 0 silently. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

The guard never allows: it prints a deny or stays silent, so Claude Code's permission rules and the holder's prompt still apply. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#goals)

Reads are matched on the lexical path and its canonical form, and any /proc or /dev/fd path is denied, so a symlink under doc/ cannot reach a secret. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#decisions)

## Delivery

The work is one standard stage, planning-sandbox, built by three Claude workers and eleven Codex units in five waves. [Stages](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#stages)

Integration verification runs the full gate, a security review, a smoke run of the hook and an operator run of the confinement test. [Integration Verification](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#integration-verification)

Knowledge distillation records the planning sandbox as a tier-2 topic linked from the architecture file. [Knowledge Distillation](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-06-session-sandbox.md#knowledge-distillation)
