# loom-software-developer agent and doctrine

The frozen contract in `loom/tests/software_developer_contracts.rs` must pass. Never edit it.

## Why this agent exists

Engineer subagents start at about 36,700 tokens and reviewers at about 22,200. `skill_listing` is 7,751 tokens on every session with the `Skill` tool. Dropping `Skill`, `WebFetch`, `WebSearch` and `TodoWrite` from mechanical workers removes that listing. `Bash` stays because the subagent preamble requires `loom memory` commands.

## The agent file

Create `agents/loom-software-developer.md`, under 2,000 bytes:

```yaml
---
name: loom-software-developer
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

Replace the Haiku routing on each surface below. Re-grep with `rg -n 'model: haiku'` and `rg -n 'haiku' CLAUDE.md.template README.md skills/loom-orchestration/SKILL.md skills/loom-plan-writer/ loom/src/orchestrator/signals/` to catch anything missed.

| Surface | Today | Becomes |
| --- | --- | --- |
| `CLAUDE.md.template` | `` `loom-software-engineer` (`model: haiku`) `` row | `` `loom-software-developer` `` row |
| `README.md` | Same row | Same change |
| `README.md` | `5 specialized subagents` | `6 specialized subagents` |
| `README.md` | "Haiku for mechanical edits" and "luna-tier work to Haiku" | Stay accurate as tier names; name `loom-software-developer` where the row routes work |
| `skills/loom-orchestration/SKILL.md` | ``(`model: haiku` on loom-software-engineer)`` | `(loom-software-developer)` |
| `skills/loom-orchestration/SKILL.md` | `model: haiku for mechanical work` | `loom-software-developer for mechanical work` |
| `skills/loom-orchestration/SKILL.md`, `skills/loom-plan-writer/SKILL.md`, `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` (BLOCK-B) | ``HAIKU (`model: haiku` on loom-software-engineer)`` | `HAIKU (loom-software-developer)` |
| `loom/src/orchestrator/signals/format/codex.rs` | ``(`loom-software-engineer` with `model: haiku`)`` | `` (`loom-software-developer`) `` |
| `skills/loom-plan-writer/references/codex-implementers.md` | Haiku by `model` override | Name the new agent for Haiku work |
| `skills/loom-plan-writer/references/authoring-detail.md` and `skills/loom-plan-writer/SKILL.md` | Execution agent list | Add `loom-software-developer` for mechanical pieces |
| `loom-hooks/spawn-guard.sh` `TYPED_AGENTS` | List of typed agents | Add `loom-software-developer (haiku)` |

Constraints:

- **BLOCK-B must be byte-identical on all three surfaces.** `tests_doctrine.rs::block_b_agrees_across_every_surface` enforces it.
- **`spawn-guard.sh` is 401 lines, over Rule 17, and no ledger covers shell hooks.** Edit only the `TYPED_AGENTS` string in place, add no lines, and keep `loom-software-engineer (sonnet, default)` in it, because `hooks_spawn_guard_cases.rs` asserts that substring. Do not touch `loom/maintainability-baseline.txt`.
- **Size caps are pinned.** `tests_size.rs` caps `CLAUDE.md.template` at 20,138 bytes and the orchestration skill at 20,530. The skill is 20,528 today, and the three replacements plus the Declared-skills sentence below land on exactly 20,530: add nothing else to that file. Never raise a cap.
- **Do not edit knowledge files.** `doc/loom/knowledge/patterns/subagent-hierarchy.md:83` goes stale: record a `stale-knowledge:` memory and the distill stage fixes it.

## Declared skills

In `skills/loom-orchestration/SKILL.md`, replace `For Codex, quote applicable rules in the brief; Codex opens no skill files.` with ``For Codex and `loom-software-developer`, quote applicable rules in briefs; neither opens skills.``

## The worker-brief hook

`loom hook worker-brief` appends "Required skills for this stage — invoke: Skill(...)" to every non-Codex typed spawn in a stage that declares skills, so the mechanical worker would be told to use a tool it lacks. In `loom/src/commands/hook/worker_brief.rs`, omit `skills_line` when `payload.tool_input.subagent_type` is `loom-software-developer`, and add a test in `tests_worker_brief.rs` that the line is absent for it and present for `loom-software-engineer`.

## What stays the same

`model: haiku` on `loom-software-engineer` stays technically possible. Nothing enforces against it, and this plan adds no check. Sonnet and Opus engineers keep their tool lists.

## Check

Run `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::` and `--lib commands::hook::` once each. Report the result to the orchestrator.
