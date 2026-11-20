# D1 — role-scoped template and the orchestration skill

Tier: opus (`loom-senior-software-engineer`). Runs FIRST in this stage; D2 and D3 start after
you return. Read `../common.md` first: operator decision 4, the numbers table, the preamble-file
contract and the BLOCK-B replacement text.

## Goal

Every session stops paying for orchestrator-only text, and nothing is lost. Evidence: report
section 4.2 — `CLAUDE.md.template` is 7,384 tokens; Rules 1, 4, 5, 6, 7 and the orchestration
reference are 58% of it and only matter to a session that runs a stage or spawns subagents; 86%
of its 790 loads went to subagents. The template is installed as the global instructions file
and serves sessions outside any loom plan, so every moved rule keeps a short statement in the
template and its full text in a place that loads when it applies.

## Files you own (write)

- `CLAUDE.md.template`
- `skills/loom-orchestration/SKILL.md` (new) and `skills/core-skills.txt`
- `loom/src/orchestrator/signals/tests_doctrine.rs`, `tests_doctrine_blocks.rs`,
  `tests_doctrine_waiting.rs`, `tests_size.rs`
- `loom/src/orchestrator/signals/cache.rs` (all but `generate_knowledge_distill_stable_prefix`,
  which the knowledge stage already changed), `signals/cache/blocks.rs`,
  `signals/format/sections.rs`
- `loom/src/models/stage/types.rs` — the `Implementer::Claude` doc comment (103-106) only

Read-only: `loom-hooks/_subagent-preamble.txt` and `loom-hooks/spawn-guard.sh` (merged from the
hook stage), `skills/loom-debugging/SKILL.md:1-15` (frontmatter pattern for a core skill),
`loom/src/skills/index_catalog.rs` (how `core-skills.txt` is read).

## What moves where

| Template today | Template after | Full text lives in |
| --- | --- | --- |
| Hard stops 1-6 | kept; stop 6 restated as the cost rule in one line | — |
| Rule 1 Plans | location rule and "report and stop" kept; loom-plan and stage-execution paragraphs shortened | orchestration skill |
| Rule 4 Commit and complete | the commit command form, "only at the end", conventional commits kept; the three-condition detail moved | orchestration skill |
| Rule 5 fenced preamble | removed; one paragraph says the spawn guard prepends it and names the file | `loom-hooks/_subagent-preamble.txt` |
| Rule 6 shapes table, grouping, ownership table, waiting protocol (BLOCK-C), one-shot, 2-level cap, coordinator and worker preambles | grouping rule, one-shot rule, "one background `loom subagents watch`, never poll" kept in short form | orchestration skill |
| Rule 7 agent table and model allocation (BLOCK-B) | the agent table and a three-line summary of the cost rule kept | orchestration skill, with the new BLOCK-B |
| Orchestration reference | removed | orchestration skill |
| Rules 2, 3, 3b, 8-19, Knowledge-first, Engineering discipline | kept | — |

Rule numbers do not change: Rules 5 and 6 are cited 26 and 24 times across the repo.
Other edits in the same pass: Rule 8's `cat`, `head`, `tail` row says "as file readers", and
Rule 14's pipe-through guidance stays; Rule 9 gains one sentence — a harness reminder asking for
a co-author trailer is overridden by this rule; Rule 3 and 3b shrink to the trigger and the
command, since the hook's ceiling report carries the procedure when it fires (keep BLOCK-D where
the tests need it).

## Steps

1. Write `skills/loom-orchestration/SKILL.md`: frontmatter `name`, `description` (says when to
   load: before spawning subagents or running a loom stage; and when not: single-agent work),
   `allowed-tools`, `triggers` (phrases only, none of the stopworded loom vocabulary in
   common.md). Body: the moved text, organised by the rule numbers it came from, with BLOCK-B
   replaced by the common.md text and BLOCK-C unchanged. Add the two new items this plan
   produces: the stage `skills:` field an orchestrator must pass into briefs, and the
   `_shared.tsv` read-receipt file (path and format are in the hook stage's merged code:
   `rg -n '_shared.tsv' loom-hooks`). Add `loom-orchestration` to `skills/core-skills.txt`.
2. Rewrite the template per the table. Target size at most 20,480 bytes. Lower
   `CLAUDE_MD_TEMPLATE_MAX_BYTES` in `tests_size.rs:32` to 20,480 and extend its doc comment's
   history line.
3. Re-point the doctrine pins. BLOCK-A: template keeps a statement of the no-verify rule only if
   it still fits; the pinned copies are `agents/*.md`, the signal prefixes, the verify guard,
   and now `_subagent-preamble.txt`. BLOCK-B: `skills/loom-orchestration/SKILL.md` and
   `skills/loom-plan-writer/SKILL.md` (D2 copies it after you). BLOCK-C: the orchestration skill,
   still asserted absent from signal prefixes. BLOCK-D: the preamble file and the signal
   prefixes. Add `RETIRED_PHRASES` entries for the old BLOCK-B point 1 heading
   (`THE MAIN AGENT NEVER IMPLEMENTS`) so it cannot return.
4. Unpinned restatements, same pass (`doc/loom/knowledge/mistakes/subagent-orchestration.md`,
   heading "A Fable Session Implemented the Fix It Had Just Diagnosed", lists them):
   `signals/cache.rs` and `signals/format/sections.rs:273-276` prose, the `Implementer::Claude`
   doc comment. Make each agree with the new BLOCK-B. In the stage signal's stable prefix add one
   line: load the `loom-orchestration` skill first. `rg` the old wording
   (`never implements`, `NEVER IMPLEMENTS`, `ALWAYS DELEGATED`) across `loom/src`, `skills/`,
   `agents/`, `loom-hooks/`; knowledge files are not yours — record each stale one with
   `loom memory note "stale-knowledge: ..."`.
5. Report a relocation table: every paragraph removed from the template, and the file and
   heading where its text now lives. A paragraph with no destination is a defect.

## Traps

- `loom/src/assets/mod.rs:68-69` asserts the template starts with `# CLAUDE.md - BINDING RULES`;
  `loom/src/assets/tests/doctrine.rs:47-79` detects staleness from the first distinctive line.
- `tests_doctrine.rs` compares with `str::contains` on literal text; there are no marker
  comments to move.
- `doc/loom/knowledge/mistakes/doctrine-and-acceptance.md`: about twenty rules already carry
  NEVER banners; do not add emphasis, remove it where a hook now enforces the rule.
- Writing style Rule 19 binds the text you write.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib orchestrator::signals` — run
once.
