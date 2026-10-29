---
kind: document
of: doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md
file: sha256:a4ab169a157eb1c5609eec50831375e13dbbddf95bdf8aa9a59cc8970ebda9b7
cites:
  "doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md#1-route-closure-standard": "sha256:00f1ad791482562458036865d02ebfde3a4088fc57861a74e4c2bde6c83f389f"
  "doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md#after-the-plan-operator": "sha256:f9ed8b70856b8d53459672e25ebaa33145815653cc86a06c8df3ea6c586780c7"
  "doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md#coverage-of-metaplan-section-7": "sha256:af5bce54d622551cf57009b71223de27beca21f604cc65aba9f41022f5534798"
  "doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins": "sha256:78c63173705c9798b2a59d905277fe7ac605df1729bf722d3c4f8307a1a3e574"
  "doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md#goals": "sha256:db743ad3c33e7dac1807ba01dc7a1feeebe39d02166d45a41da89d0bdea0417a"
  "doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md#overview": "sha256:d8d2915196284e0fbdd8073c97ab17007b60cb746f850c98d9311d58c629c047"
  "doc/plans/archive/DONE-PLAN-06-sandbox-route-closure.md#what-the-tree-shows-verified-2026-10-19-at-6fee4ecd-re-verified-2026-10-21-at-50aa8336-and-against-host-verifications-host-jobs-worktree": "sha256:a77ce6f81ff6d3504687cbe5567418b6466190dae8ba0393d9ca1b1b8616c184"
---

# Sandbox Route Closure

## Routes and closures

The plan closes four routes that let a stage agent's Bash escape the bubblewrap capsule: out-of-sandbox tools, unlisted credential files, sibling worktree administrative directories and unconfined manual-mode starts. [Overview](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#overview)

Every capsule denies LSP, RemoteTrigger, CronCreate and PushNotification by bare tool name in permissions.deny, and denies Workflow except for the Stage session of an ultracode stage. [Goals](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

Workflow is licensed for ultracode stages, so a blanket deny would break a shipped feature. [Deviations](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

Tool denies go into permissions.deny because --disallowedTools is variadic and would swallow the positional prompt. [What the tree shows](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#what-the-tree-shows-verified-2026-10-19-at-6fee4ecd-re-verified-2026-10-21-at-50aa8336-and-against-host-verifications-host-jobs-worktree)

Only the refactoring skill's LSP line changes, because no agent definition or plan-writer file names LSP. [Deviations](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

Every capsule read-denies nine credential files as plain paths, and ~/.codex/auth.json unless the stage licenses the codex lane. [Goals](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

The mandatory read-deny list becomes a function of the codex license, because a const alias cannot hold a conditional entry. [Deviations](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

Every capsule write-denies the whole administrative directory of each sibling worktree, leaving the session's own directory, refs and the object store writable. [Goals](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#goals)

Bubblewrap aborts when a deny's mount point lies beneath a read-only bind, so a sibling denied whole loses its separate config.worktree deny. [Deviations](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#deviations-from-the-metaplan-the-tree-wins)

## Proof and operations

The session's own administrative directory is the one whose gitdir names its worktree, and a worktree with zero or two registry claimants refuses the spawn. [What the tree shows](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#what-the-tree-shows-verified-2026-10-19-at-6fee4ecd-re-verified-2026-10-21-at-50aa8336-and-against-host-verifications-host-jobs-worktree)

Both manual-mode print sites gain a line saying the session runs without the wrapper's confinement. [Stages](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#1-route-closure-standard)

Proof comes from unit tests on the emitted capsule, two frozen contracts, and a bubblewrap probe run as a host job. [Coverage of metaplan section 7](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#coverage-of-metaplan-section-7)

The probe asserts that a denied file refuses reads rather than reading empty, because bubblewrap binds non-dev paths nodev and the open fails with EACCES. [What the tree shows](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#what-the-tree-shows-verified-2026-10-19-at-6fee4ecd-re-verified-2026-10-21-at-50aa8336-and-against-host-verifications-host-jobs-worktree)

The live, credentialed canary is a manual release checklist, run before the next release because no gate runs a live Claude Code sandbox. [After the plan](../../../plans/archive/DONE-PLAN-06-sandbox-route-closure.md#after-the-plan-operator)
