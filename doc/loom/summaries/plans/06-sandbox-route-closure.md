---
kind: document
of: doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md
file: sha256:a4ab169a157eb1c5609eec50831375e13dbbddf95bdf8aa9a59cc8970ebda9b7
cites:
  "doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#1-route-closure-standard": "sha256:00f1ad791482562458036865d02ebfde3a4088fc57861a74e4c2bde6c83f389f"
  "doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#2-integration-verify": "sha256:92fa4fd588a99aabcdcd993b830bd2fcbff8db92094b5d2c52539045f9f0a3bd"
  "doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#3-knowledge-distill": "sha256:9eddcc0ce5b426e4a857a92aae3c62556d18ee53a2275dcf0c0bc89272048ff9"
  "doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#after-the-plan-operator": "sha256:f9ed8b70856b8d53459672e25ebaa33145815653cc86a06c8df3ea6c586780c7"
  "doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins": "sha256:78c63173705c9798b2a59d905277fe7ac605df1729bf722d3c4f8307a1a3e574"
  "doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#goals": "sha256:db743ad3c33e7dac1807ba01dc7a1feeebe39d02166d45a41da89d0bdea0417a"
  "doc/plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#overview": "sha256:d8d2915196284e0fbdd8073c97ab17007b60cb746f850c98d9311d58c629c047"
---

# Sandbox Route Closure

## Overview

A stage agent's Bash runs inside Claude Code's bubblewrap sandbox, yet four routes stay open: out-of-sandbox tools, credential files the read-deny list omits, sibling worktrees' git administrative directories, and manual mode [Overview](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#overview)

The plan closes these routes in the session capsule, proves each deny on the emitted capsule and under bubblewrap, and leaves the live credentialed canary as a manual release checklist [Overview](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#overview)

## Capsule Changes

Every capsule denies LSP, RemoteTrigger, CronCreate and PushNotification by bare tool name, and denies Workflow except in the Stage session of an ultracode stage [Goals](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

Every capsule read-denies nine credential files as plain paths, and `~/.codex/auth.json` unless the stage licenses the codex lane [Goals](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

Every capsule write-denies the administrative directory of each sibling worktree whole, and keeps the session's own directory, `refs/` and the object store writable [Goals](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

Both manual-mode print sites gain a line saying the session runs without the wrapper's confinement [Goals](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

Ref denies, common-directory denies, sibling worktree read denies and any `Read(` permission rule are non-goals [Goals](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

## Deviations

The metaplan's blanket Workflow deny is narrowed because the tree licenses Workflow for ultracode stages, so a blanket deny would break a shipped feature [Deviations](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

Sibling admin directories are denied whole and lose their separate `config.worktree` denies, because bubblewrap aborts when a mount point lies beneath a read-only bind [Deviations](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

Ref denies stay out because commits are now daemon-owned, but no one has measured a `refs/` deny under the current grant [Deviations](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

The optional read tail is not built, since the main checkout stays readable by design and no gate runs a live sandbox to catch a wrong re-allow [Deviations](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

## Stages

The route-closure stage is standard and runs on Claude, with one Codex unit for the new manual-start module [Stages](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#1-route-closure-standard)

The integration-verify stage runs the full pre-push gate and the bubblewrap probe as an acceptance host job, but it cannot observe the new denies live [Stages](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#2-integration-verify)

The knowledge-distill stage applies stale-knowledge notes first, then rewrites the sandbox knowledge sections to current truth [Stages](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#3-knowledge-distill)

## Operator Steps

After the plan, the operator runs `./dev-install.sh` and the release checklist, which is the only live proof of these denies [After the plan](../../../plans/optim-token/archive/DONE-PLAN-06-sandbox-route-closure.md#after-the-plan-operator)
