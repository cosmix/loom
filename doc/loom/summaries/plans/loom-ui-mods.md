---
kind: document
of: doc/plans/PLAN-loom-ui-mods.md
file: sha256:b7297e85c3d71a4b1b14b46b89956975960c00b5b59ccbd53a9aec3c8f830bd7
cites:
  "doc/plans/PLAN-loom-ui-mods.md#2-loom-ui-standard": "sha256:7867f7872e1e9432c7ac8ab9535e55c4523cf8a1b59fd4b837794b2161154685"
  "doc/plans/PLAN-loom-ui-mods.md#3-integration-verification": "sha256:5594311eb650f5f8b7fb422354b534f3fd43f282645e389b3df96355ec988c92"
  "doc/plans/PLAN-loom-ui-mods.md#4-knowledge-distillation": "sha256:0c63dccabb37591a8f8d28d7c0c798c2f1cfabf05bfc395b774439b1dccb8ead"
  "doc/plans/PLAN-loom-ui-mods.md#before-loom-run": "sha256:bddbec7c4d1dd1a19870e5c3d44c8ad5c436351ed3f962f56490bedb1716f0e3"
  "doc/plans/PLAN-loom-ui-mods.md#decisions": "sha256:1adf0682a9d9dbd02231964968562b8e6be97bb6ac2006b4c706098765bbb483"
  "doc/plans/PLAN-loom-ui-mods.md#execution-diagram": "sha256:5c2ecd2ab02a95636dddfecd2298ba274368bafc0a1522e8938184681adda713"
  "doc/plans/PLAN-loom-ui-mods.md#goals": "sha256:c6ef1aa8f550f1efb640e955baac882f94d98112e3ffdc45a9151beb40d902c5"
  "doc/plans/PLAN-loom-ui-mods.md#overview": "sha256:f0f58f1f60260c8e190af5721f36c6d460e1e2de6a0d06b3055e75e6f933a78a"
  "doc/plans/PLAN-loom-ui-mods.md#sibling-plans": "sha256:8faa1db0499bdd9ef63a29538cc0c80bef9483a5b701744a94542bf74ba233c6"
---

# Loom UI mods plan

## Scope

The plan adds a read-only Claude Code plugin, `loom-ui`, that shows stage, context, subagent and guard-denial state inside a session. [Overview](../../../plans/PLAN-loom-ui-mods.md#overview)

Outside a stage the stage band, context gauge and `/loom` pane stay hidden, while the subagent band and `/loom-agents` pane appear in every session. [Goals](../../../plans/PLAN-loom-ui-mods.md#goals)

Desktop notifications, buttons that act on loom and any write to `~/.claude/settings.json` are out of scope. [Goals](../../../plans/PLAN-loom-ui-mods.md#goals)

## Design

One plugin carries four `userConfig` toggles, all on by default. [Decisions](../../../plans/PLAN-loom-ui-mods.md#decisions)

The plugin reads state only through the `hook ui-snapshot` subcommand of the loom binary, and a surface script fails on any hook, call or environment read outside an allowlist. [Decisions](../../../plans/PLAN-loom-ui-mods.md#decisions)

The Rust command proves stage membership from session, stage and worktree records, because the presence of `LOOM_*` variables proves nothing. [Decisions](../../../plans/PLAN-loom-ui-mods.md#decisions)

Install places the plugin folder under `~/.claude/skills/loom-ui`, where Claude Code loads and enables it, and rewrites only files whose bytes changed. [Decisions](../../../plans/PLAN-loom-ui-mods.md#decisions)

Subagent context comes from the usage on each `turn.step`, so it needs no session-writable ledger. [Decisions](../../../plans/PLAN-loom-ui-mods.md#decisions)

The JSON schema is pinned by one fixture that the Rust contracts and the plugin tests both read. [Decisions](../../../plans/PLAN-loom-ui-mods.md#decisions)

## Plan structure

Before `loom run`, the operator commits the plan, the briefs and the knowledge corrections on main. [Before loom run](../../../plans/PLAN-loom-ui-mods.md#before-loom-run)

Sibling plans share `dispatch*.rs` and hook files with this one, so they must not run at the same time. [Sibling plans](../../../plans/PLAN-loom-ui-mods.md#sibling-plans)

The execution diagram runs knowledge-bootstrap, then loom-ui, then integration-verify, then knowledge-distill. [Execution Diagram](../../../plans/PLAN-loom-ui-mods.md#execution-diagram)

The loom-ui stage runs in `loom/` with W1 and W2 in parallel, then W3 once both have returned. [2. loom-ui (standard)](../../../plans/PLAN-loom-ui-mods.md#2-loom-ui-standard)

Integration verification reruns the full suite, the plugin validate and test runs and the install smoke script, then runs security, architecture and test reviews. [3. Integration Verification](../../../plans/PLAN-loom-ui-mods.md#3-integration-verification)

Knowledge distillation records the plugin facts and the skills-dir mechanism, and documents loom-ui in `README.md`. [4. Knowledge Distillation](../../../plans/PLAN-loom-ui-mods.md#4-knowledge-distillation)
