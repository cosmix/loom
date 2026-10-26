# W1a: the discussion and step skills of the `loom-plan` plugin

Stage `guidance-engine` of `doc/plans/PLAN-feat-planner-08-guidance.md`. Tier: sonnet (`loom-software-engineer`).
You write Markdown only. You never run git, never commit, never spawn subagents.

## Files you own (write only these)

- `plan-plugin/skills/brainstorming/SKILL.md`
- `plan-plugin/skills/vision/SKILL.md`
- `plan-plugin/skills/section/SKILL.md`
- `plan-plugin/skills/metaplan/SKILL.md`
- `plan-plugin/skills/plan/SKILL.md`

Paths are repo-relative (the stage's `working_dir` is `loom/`, so from there they are
`../plan-plugin/...`). `plan-plugin/.claude-plugin/plugin.json` already exists (the orchestrator
wrote it); do not touch it. W1b writes the other skills and the two agents; W3 edits
`skills/loom-plan-writer/`; W4 writes the Rust that embeds and renders these files.

## Read first (ranges, not whole files)

- `doc/product/section_09.md` lines 58-105 ("The plugin"), 147-254 ("Steps and their skills":
  brainstorming, vision, section, metaplan, plan), 331-351 ("Write limits").
- `doc/product/section_03.md` lines 239-251 ("What the writing sessions keep to"), 153-157
  ("Product-to-plan links": the `Implements:` line).
- `doc/product/section_01.md` lines 44-82 (vision table and metaplan entry detection rules).
- Precedent for skill frontmatter: `skills/loom-plan-writer/SKILL.md` lines 1-5.

## Every skill file

Frontmatter, exactly these two keys:

```yaml
---
name: <directory name>
description: <one sentence: when a planning session loads this skill>
---
```

Then a level-1 title and the procedure. Second person, imperative, plain words. No emojis. No
mention of Claude, AI or any assistant. Plugin skills are namespaced: refer to siblings as
`loom-plan:<name>` (for example `loom-plan:brainstorming`). Keep each file under 200 lines.

Every skill states, near the top, the paths it writes and that it writes nothing else, never
stages, commits or branches, and never runs `loom init`, `loom run` or `loom pressure`
(section 09, "Write limits"). Do not restate style or tooling rules a `CLAUDE.md` might carry
(section 09, "The user's CLAUDE.md files").

## `brainstorming`: adapted from superpowers (MIT)

1. The upstream text is committed beside this brief, fetched at authoring time from commit
   `2ce8aa15d05bd57213e6f746f953aafb7ded7a8b`: `doc/plans/briefs/feat-planner-08-guidance/upstream/brainstorming-SKILL.md`
   (294 lines) and `doc/plans/briefs/feat-planner-08-guidance/upstream/superpowers-LICENSE` (21 lines);
   `doc/plans/briefs/feat-planner-08-guidance/upstream/SOURCE.md` holds their sha256. Read both in full.
   Adapt from them only; never from memory, never from the network.
2. Keep the upstream procedure (what you know before asking, look before you ask, one
   question at a time, choices where the answer space is small, two or three approaches with
   trade-offs and a recommendation before a direction settles, play the understanding back).
3. Change exactly these things (section 09, "The plugin"):
   - It writes into `doc/product/` and `doc/plans/` at section 03's paths, given by the step
     skill that loaded it; remove every `docs/superpowers/` path.
   - It makes no commits; remove every commit instruction.
   - It ends when the discussion settles; remove the hand-off to `superpowers:writing-plans`
     and every instruction to invoke another skill afterwards. Decisions are written into the
     target document as each one settles (section 09 "Recording as you go"), never only at the
     end.
   - Drop the "Visual Companion" and any companion-file reference (`visual-companion.md`,
     `builder-check-prompt.md`): the plugin ships no such files and the session has no browser
     of its own.
   - Questions are asked in plain text, one per message, never with a question tool (the
     session's settings deny it); the user reads the chat in a browser.
4. License: after the frontmatter and title, a section `## License` holding the FULL text of
   `upstream/superpowers-LICENSE` verbatim (copy the file's bytes; its first lines are
   `MIT License` and `Copyright (c) 2025 Jesse Vincent`), plus
   one line above it: `Adapted from the brainstorming skill of the superpowers plugin
   (https://github.com/obra/superpowers, commit 2ce8aa15).` W4's contract test checks the
   rendered copy contains that file's whole text as one block.
5. The adapted file contains none of: `docs/superpowers`, `writing-plans`, `git commit`. W4's
   unit test checks this.

## `vision`, `section`, `metaplan`, `plan`

Write each from its subsection of section 09 ("`loom-plan:vision`" ... "`loom-plan:plan`"), keeping
every rule stated there, including:

- `vision`: target `doc/product/index.md`; runs its discussion through `loom-plan:brainstorming`;
  the default outline (Problem, Vision, Principles, Users, Goals, Non-goals, Anti-goals,
  Specification sections, Decisions); the `## Specification sections` table whose first column
  is the zero-padded number (the only part the app parses: say so); `## Decisions` naming the
  sections each decision binds; also handles "fix the vision's section list" and "add this
  unlisted section to the vision".
- `section`: target `doc/product/section_<nn>.md`; read the vision and every written section,
  `doc/loom/knowledge/INDEX.md` and only the topics touched, and the code a rule rests on;
  the output shape (title `# <nn> — <Title>`, back-link line, scope paragraph, topic headings,
  `path:line` citations, `(My call.)`, `## Open questions` with options and a recommendation,
  `## Out of scope for this section`).
- `metaplan`: target `doc/plans/index.md`, or `doc/plans/metaplan-<nn>.md` with the next free
  two-digit number for a later round; read the spec, existing metaplans and every plan in any
  state; entry heading form `## <n>. <Title> (`PLAN-<slug>.md`)` then three or four paragraphs;
  prose headings carry no plan file name; entry order is execution order; also handles "add
  this unlisted plan to a metaplan".
- `plan`: target `doc/plans/PLAN-<slug>.md`; the app conventions (title, then the
  `Implements:` line from the metaplan entry, confirmed with the user, `Implements: none` when
  it implements no section; scope from the metaplan entry, a scope change goes back into the
  entry in the same turn; briefs under `doc/plans/briefs/<slug>/` by full path;
  `loom plan verify <plan>` after writing with every error fixed); then load loom's
  `loom-plan-writer` skill, which owns plan authoring; subagents allowed per plan, agent teams
  not used in authoring.

Every step skill ends with the turn-end duty in one short paragraph: after any turn that edited
documents run `loom plan check` and fix what it reports, then rebuild summaries in two waves
(`loom plan summary status --json`; one `loom-plan:summary` subagent per stale, incomplete or
missing document summary, in parallel; then one `loom-plan:overview` subagent for the overviews
that cite them; spawn nothing when nothing is stale). Use these command spellings exactly.

## Done when

The five files exist with the frontmatter above, each under 200 lines, and `rg -n
'docs/superpowers|writing-plans|git commit' plan-plugin/skills/brainstorming/SKILL.md` prints
nothing. Report the upstream commit the files came from and anything in the upstream text you could
not place under the rules above.
