# W1b: refine, retro, knowledge and summaries skills, and the two summary agents

Stage `guidance-engine` of `doc/plans/PLAN-plan-guidance.md`. Tier: sonnet (`loom-software-engineer`).
You write Markdown only. You never run git, never commit, never spawn subagents.

## Files you own (write only these)

- `plan-plugin/skills/refine/SKILL.md`
- `plan-plugin/skills/retro/SKILL.md`
- `plan-plugin/skills/knowledge/SKILL.md`
- `plan-plugin/skills/summaries/SKILL.md`
- `plan-plugin/agents/summary.md`
- `plan-plugin/agents/overview.md`

Paths are repo-relative (from the stage's `working_dir` `loom/` they are `../plan-plugin/...`).
W1a writes the other five skills; W4 writes the Rust that renders these files and rewrites the
agents' `model:` and `effort:` lines at render time.

## Read first (ranges, not whole files)

- `doc/product/section_09.md` lines 255-280 (refine, retro, knowledge), 353-385 ("Summaries
  after a turn", "Summary sessions"), 331-351 ("Write limits").
- `doc/product/section_04.md` lines 213-306 ("Sealing a summary", "Who rebuilds what").
- `commands/distill.md` (27 lines): the routing list (lines 12-21), "Corrections first" (line 8)
  and "Current truth only" (line 10).
- `agents/loom-senior-software-engineer.md` lines 1-8 (agent frontmatter form).
- The summary CLI as plan 4 (`doc/plans/PLAN-plan-summary-store.md`, stage `summary-cli`)
  ships it: `loom plan summary status --json`, `loom plan summary brief <id>... [--note <text>]`
  (claims the ids and prints the brief, which names the draft file to write),
  `loom plan summary seal <id>` (exit 1 with `<draft path>:<line>: <message>` per problem),
  `loom plan summary release <id>...`. Use exactly these spellings.

## Skills

Frontmatter exactly `name:` (the directory name) and `description:` (one sentence). A level-1
title, then the procedure. Second person, imperative, no emojis, no mention of Claude or AI.
Each file under 150 lines. Each states the paths it writes and that it never stages, commits,
branches, or runs `loom init`, `loom run` or `loom pressure`.

- `refine` (ends with the turn-end duty paragraph W1a's brief gives, with `loom plan check` and
  `loom plan summary status --json` spelled exactly): section 09 "`loom-plan:refine`": read the document and the documents it binds or is
  bound by (a section's vision; a plan's metaplan entry and the sections in its `Implements:`
  line); make the asked change; carry it into every document stating the same thing in the
  same turn, or record the conflict under `## Open questions` in the document that cannot
  change; keep headings summaries cite unless the change is about the heading. Locked files
  (`IN_PROGRESS-`, `DONE-`, `RETIRED-`, `doc/plans/archive/`, a locked plan's brief bundle,
  `REVIEW-*.md`) are refused with the reason and the alternative (a new metaplan entry or a spec
  refinement).
- `retro`: section 09 "`loom-plan:retro`": read each done plan's `REVIEW-<plan file stem>.md`, the
  sections in its `Implements:` line and `doc/loom/knowledge/INDEX.md`; walk the user through
  what was built differently from the spec; record accepted divergences as decisions in the
  affected section, follow-ups as open questions or as proposed entries for a new metaplan
  round; never edit done plans or run reviews.
- `knowledge`: carry `commands/distill.md`'s routing list, its "Corrections first" rule (
  `loom knowledge replace-section <file> "<heading>" "<body>"` for an existing claim) and its
  "Current truth only" rule, with these changes: the user's message names what to record, so
  never run `loom memory pending` and never `loom memory resolve` (write exactly the line
  ``Do not run `loom memory pending` or `loom memory resolve`.``, which W4's test looks for); every write goes through
  `loom knowledge update`, `replace-section` or `annotate`, never a file tool. Copy the routing
  list's six bullets verbatim from `commands/distill.md` lines 14-19; W4's unit test checks
  each bullet line appears byte-identical in this skill (one text, two surfaces:
  `doc/loom/knowledge/patterns/doctrine-cross-surface.md`).
- `summaries`: the background summary session's procedure (section 09 "Summary sessions",
  section 04 "Background"): rebuild only the ids in the opening message; work leaves first, one
  `loom-plan:summary` subagent per document summary in parallel, then the overviews yourself
  with `loom plan summary brief <id>` and `seal`; never ask the user anything (nobody is
  there); never edit a source document; release (`loom plan summary release <id>`) a claim you
  cannot seal; stop when every id is sealed or released.

## Agents

`agents/summary.md` (document summaries) and `agents/overview.md` (overviews). Frontmatter:

```yaml
---
name: summary            # or overview
description: <one sentence>
tools: Read, Write, Edit, Bash, Glob, Grep
model: haiku             # overview: sonnet
effort: medium           # overview: high
---
```

The `model:` and `effort:` lines must each be a single line in exactly that form, because the
renderer replaces them (W4); the values above are the built-in defaults (section 09, "Models and
effort"). Body: the whole procedure for the ids it is given (section 09 "Summaries after a
turn", "The two agent definitions hold the whole procedure"): run `loom plan summary brief
<id>`, read only what the brief names, write only the draft file the brief names, run `seal`
and fix each reported problem until it is accepted, never edit a source document or any file
under `doc/`, release a claim it cannot seal, and report the ids sealed and released.

## Done when

Six files exist as above; each agent file has exactly one `model:` line and one `effort:` line
in its frontmatter. Report anything in the spec you could not place.
