# Brief: doctrine (bounded Codex exploration)

Tier: `loom-software-developer` (haiku, effort high): every sentence and file is settled below. Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/orchestrator/signals/mod.rs` (contract scaffold line only, already added), `loom/src/orchestrator/signals/tests_doctrine_blocks.rs`, `loom/src/orchestrator/signals/tests_doctrine.rs`, `skills/loom-orchestration/SKILL.md`, `skills/loom-plan-writer/SKILL.md`, `skills/loom-plan-writer/references/codex-implementers.md`, `loom/src/models/stage/implementers.rs`, `loom/src/models/stage/defaults.rs`, `CLAUDE.md.template`, `agents/loom-software-engineer.md`.

Frozen, read-only: `loom/src/orchestrator/signals/exploration_doctrine_contract_tests.rs`.

## The rule (use these exact sentences)

Replace, in BLOCK_B (`tests_doctrine_blocks.rs:57-58`) and its byte-identical copy (`skills/loom-orchestration/SKILL.md:174`):

```text
review, planning, exploration, debugging and iteration on failing tests
stay with Claude.
```

with the text the stage PUBLIC SURFACE gives ("Codex may take a bounded exploratory piece: ... stay with Claude."). Wrap lines at the same width as the surrounding block; the two copies stay byte-identical (`block_b_agrees_across_every_surface`, `tests_doctrine.rs:145`).

Then, with the same meaning in each file's own register:

- BLOCK_B line 63 / SKILL.md:180: "SONNET (loom-software-engineer): pieces that need exploration or the initial iteration on failing tests" becomes "pieces that need open-ended exploration across modules, debugging, or the initial iteration on failing tests".
- `skills/loom-plan-writer/SKILL.md:177` ("Claude fits exploration, fixture-backed test convergence, UI design and debugging."): Codex also fits a bounded exploratory piece inside the files it owns; Claude fits open-ended exploration, fixture-backed test convergence, UI design and debugging.
- `references/codex-implementers.md:7`, the first Claude bullet of "Per-stage lane rule" ("the change is unknown until someone explores the code"), and the checklist line 201: the same distinction (bounded, inside owned files, with the navigation kit vs open-ended or across modules).
- `implementers.rs:14,38`, `defaults.rs:105` doc comments; `CLAUDE.md.template` Rule 7 table row for `loom-software-engineer` ("Work needing exploration or iteration on failing tests" -> "Work needing open-ended exploration or iteration on failing tests"); `agents/loom-software-engineer.md:3` description likewise.

Do not change any codex model, effort or deadline text: the pins at `tests_doctrine.rs:164-186` and `:323-374` must keep passing. Check whether `CLAUDE.md.template` lines are in a frozen-lines list (`rg -n FROZEN_LINES loom/src`) and keep any pin consistent.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::` and `--lib models::stage::` pass with the frozen contract.
