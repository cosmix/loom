# loom-mechanical-engineer agent and doctrine

The frozen contract in `loom/tests/mechanical_worker_contracts.rs` must pass. Never edit it.

## Why this agent exists

Engineer subagents start at about 36,700 tokens and reviewers at about 22,200. `skill_listing` is 7,751 tokens on every session with the `Skill` tool. Dropping `Skill`, `WebFetch`, `WebSearch` and `TodoWrite` from mechanical workers removes that listing. `Bash` stays, even though it brings `sandbox_instructions`, because the subagent preamble requires `loom memory` commands.

## The agent file

Create `agents/loom-mechanical-engineer.md`, under 2,000 bytes:

```yaml
---
name: loom-mechanical-engineer
description: Mechanical edits, boilerplate, scaffolding and simple unit tests from a brief that settles every decision. Cheapest implementation tier.
tools: Read, Edit, Write, Glob, Grep, Bash
model: haiku
maxTurns: 1000
---
```

The body keeps only what this role needs:

- It is a leaf: it never spawns subagents, and touches only its assigned files.
- The brief settles every decision. If it does not, stop and report the open decision rather than choosing.
- It has no `Skill` tool. The rules it needs are quoted in its brief.
- Use existing patterns. No stubs. Files under 400 lines, functions under 50.
- Never run `git commit`, `loom stage commit` or `loom stage complete`.
- On a context ceiling, stop and report what is done and what remains.

Do not copy the `Skills to Leverage`, `Approach` or `Self-Review` sections from `agents/loom-software-engineer.md`. Assets embed automatically from `agents/` through `loom/build/assets.rs`.

## Doctrine surfaces

Replace the Haiku routing on each surface below. Re-grep with `rg -n 'model: haiku'` and `rg -n 'haiku' CLAUDE.md.template skills/loom-orchestration/SKILL.md skills/loom-plan-writer/ loom/src/orchestrator/signals/` to catch anything missed.

| Surface | Today | Becomes |
| --- | --- | --- |
| `CLAUDE.md.template:113` | `` `loom-software-engineer` (`model: haiku`) `` row | `` `loom-mechanical-engineer` `` row |
| `README.md:1089` | Same row | Same change |
| `skills/loom-orchestration/SKILL.md:102-103` | ``(`model: haiku` on loom-software-engineer)`` | `(loom-mechanical-engineer)` |
| `skills/loom-orchestration/SKILL.md:121` | `model: haiku for mechanical work` | `loom-mechanical-engineer for mechanical work` |
| `skills/loom-orchestration/SKILL.md:162`, `skills/loom-plan-writer/SKILL.md:171`, `loom/src/orchestrator/signals/tests_doctrine_blocks.rs:43` (BLOCK-B) | ``HAIKU (`model: haiku` on loom-software-engineer)`` | `HAIKU (loom-mechanical-engineer)` |
| `loom/src/orchestrator/signals/format/codex.rs:63` | ``(`loom-software-engineer` with `model: haiku`)`` | `` (`loom-mechanical-engineer`) `` |
| `skills/loom-plan-writer/references/codex-implementers.md:12` | Haiku by `model` override | Name the new agent for Haiku work |
| `skills/loom-plan-writer/references/authoring-detail.md:88` and `skills/loom-plan-writer/SKILL.md:274` | Execution agent list | Add `loom-mechanical-engineer` for mechanical pieces |
| `loom-hooks/spawn-guard.sh:215` `TYPED_AGENTS` | List of typed agents | Add `loom-mechanical-engineer (haiku)` |

Constraints:

- **BLOCK-B must be byte-identical on all three surfaces.** `tests_doctrine.rs::block_b_agrees_across_every_surface` enforces it.
- **`spawn-guard.sh` is over the line limit and in the exact ledger.** Keep its line count unchanged and do not touch `loom/maintainability-baseline.txt`.
- **Size caps are pinned.** `tests_size.rs` caps `CLAUDE.md.template` at 20,138 bytes and the orchestration skill at 20,530. Trim doctrine before raising a cap.
- **Do not edit knowledge files.** `doc/loom/knowledge/patterns/subagent-hierarchy.md:83` goes stale: record a `stale-knowledge:` memory and the distill stage fixes it.

## Declared skills

Near `skills/loom-orchestration/SKILL.md:73`, where declared skills are passed to Claude briefs, add one sentence: `loom-mechanical-engineer` has no `Skill` tool, so quote applicable rules in its brief, as for Codex. Offset the bytes with the shorter wording above.

## What stays the same

`model: haiku` on `loom-software-engineer` stays technically possible. Nothing enforces against it, and this plan adds no check. Sonnet and Opus engineers keep their tool lists.

## Check

Run `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::tests_doctrine` once. Report the result to the orchestrator.
