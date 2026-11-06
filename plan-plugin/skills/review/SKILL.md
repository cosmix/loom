---
name: review
description: Load when a planning session reviews a feature after a plan has run and records what it finds in the feature's findings ledger.
---

# Review the feature

Use the feature name given in the session's opening prompt. The feature's findings ledger is
`doc/plans/<feature>/findings.md`. Run reviews (`REVIEW-*.md`) stay the immutable evidence; the
ledger holds the findings and their triage state.

## Procedure

1. Read the ledger when it exists, so you know which findings are recorded and the highest
   `F-<n>`. Read the feature's product spec, its metaplan and its plans in every state, and the
   run reviews of its done plans.
2. Ask the user what they saw: running the feature, reading code, or reading a run review. One
   question at a time, in plain text. Take their account as the finding's substance.
3. Read the code and documents the account points at, read-only, to name what each finding
   concerns. Do not edit code, configuration, specs, plans or reviews.
4. For each finding, agree its wording with the user, then append one entry to the ledger.
5. Report what you recorded: each id, title and `About:` line.

## Ledger format

The file is plain Markdown. Create it with a `# Findings` heading when it is absent, and append
entries; never rewrite, renumber or reorder existing ones.

```markdown
# Findings

## F-1 Rebuild threshold ignores briefs

About: PLAN-feat-planner-03-summaries.md#stage:scheduler, section_04#rebuild-budget
Source: review 2026-11-05
Status: open

The rebuild budget counts plan summaries but not brief changes.
```

- An entry is a `## F-<n> <title>` heading: `n` is the next free number (the highest existing
  number plus one, at least 1) and the title is a short phrase on one line.
- `About:` names the most specific nodes the finding concerns, comma-separated: a spec heading
  (`section_04#rebuild-budget`) before a whole section (`section_04`), a plan stage
  (`PLAN-<slug>.md#stage:<id>`) before a plan, `vision`, or `metaplan#<entry anchor>`. Anchors are
  GitHub heading anchors. Name only nodes that exist.
- `Source:` is optional. When present, write `Source: review <YYYY-MM-DD>` with today's date.
  Never write a `Source:` line that names a `REVIEW-` file; the app writes those when it imports
  a run review.
- `Status:` is `open`. The other values are set later, never by this skill: `planned
  PLAN-<slug>.md`, `folded metaplan#<entry anchor>` and `dismissed: <reason>`.
- Then one text paragraph: what is wrong or missing, where it showed, and what the user expects.
  Keep it on one line. Never start it with `#`, a backtick, `~` or a backslash, and never with
  `About:`, `Source:` or `Status:`. The ledger parser reads a leading `#` as a heading, a leading
  backtick or `~` as a code fence, and those three keys as metadata lines, so the entry would
  split or change meaning. Begin with a word instead.

Leave the lines of existing entries as they are, including their `Status:`.

## Limits

Write only `doc/plans/<feature>/findings.md`. Never write a `Settled:` line, a spec, a plan, a
brief, a run review, `doc/loom/knowledge/` or `doc/loom/summaries/`. Never stage, commit, or
branch, and never run `loom init`, `loom run`, or `loom pressure`.

After any turn that edited the ledger, run `loom plan check` and fix what it reports about `findings.md`;
report any other problem to the user without editing it.
