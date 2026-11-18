---
name: review
description: Load when a planning session reviews a feature: verifying the findings ledger's unverified entries against the current documents and recording new findings with evidence.
---

# Review the feature

Use the feature name given in the session's opening prompt. The feature's findings ledger is
`doc/plans/<feature>/findings.md`. Run reviews (`REVIEW-*.md`) stay the immutable evidence; the
ledger holds the findings and where each stands.

A finding is worth recording only when it is true of the documents as they are now, says what
to change, and quotes the text it is about. The app checks the quote: a finding whose `Evidence:`
is not in a passage its `About:` names reads stale and is never addressed, and one that repeats
an earlier entry reads as a duplicate. Record fewer findings rather than unchecked ones.

## Procedure

1. Read the whole ledger, every status included, so you know what is recorded, addressed and
   dismissed, and the highest `F-<n>`. Read the feature's product spec, its metaplan, its plans in
   every state and the run reviews of its done plans.
2. Verify each open entry that has no `Evidence:` line (the app imports run-review items that
   way). Read the file and line it cites and the spec or plan text it concerns, then decide:
   - It still holds and changes what a spec or plan must say: edit the entry in place. Set
     `About:` to the most specific nodes, add `Evidence:` and `Severity:`, and you may replace
     its title with a short phrase. Keep its `Source:` line and its text paragraph.
   - The tree already does what it asks, it is not true, or it is a code-level note (naming, test
     style, formatting) that no spec or plan change would settle: set
     `Status: dismissed: <what you checked>`.
   - Another entry reports the same thing: set `Status: dismissed: duplicate of F-<n>`, keeping
     the clearer one.
3. Ask the user what they saw: running the feature, reading code, or reading a run review. One
   question at a time, in plain text. Take their account as the finding's substance.
4. Before adding a finding, check it adversarially. Find the exact passage it is about and copy
   the sentence that shows the problem. Argue the opposite once: if the quote does not show the
   problem, or the claim depends on "could", "might" or "consider", drop it. Search the ledger
   for the same issue in any status. An open one already covers it: add nothing. An addressed
   one that regressed: say so and ask the user before recording it again, with an `Evidence:`
   quote of the passage as it reads now; an entry that repeats the addressed one's quote reads as
   its duplicate. An unaddressed or dismissed one: say so; the user reopens it on the Findings
   page instead of recording it again.
5. Agree each new finding's wording with the user, then append one entry.
6. Run `loom plan check` and fix what it reports about `findings.md`
   (`findings-evidence-missing`, `about-unresolved`, `findings-bad-severity`).
7. Report the entries you verified, dismissed (with the reason) and added: id, title, `About:`.

## Ledger format

The file is plain Markdown. Create it with a `# Findings` heading when it is absent, and append
entries; never renumber or reorder existing ones.

```markdown
# Findings

## F-1 Rebuild threshold ignores briefs

About: section_04#rebuild-budget
Evidence: The rebuild budget counts plan summaries changed since the last rebuild.
Severity: high
Source: review 2026-11-05
Status: open

The rebuild budget counts plan summaries but not brief changes, so a brief edit never triggers a rebuild.
```

- An entry is a `## F-<n> <title>` heading: `n` is the next free number (the highest existing
  number plus one, at least 1) and the title is a short phrase on one line.
- `About:` names the most specific nodes the finding concerns, comma-separated: a spec heading
  (`section_04#rebuild-budget`) before a whole section (`section_04`), a plan stage
  (`PLAN-<slug>.md#stage:<id>`) before a plan, `vision`, or `metaplan#<entry anchor>`. Anchors are
  GitHub heading anchors. Name only nodes that exist, and name first the node the change belongs
  in.
- `Evidence:` is one line copied verbatim from the passage of an `About:` node, at least three
  words: the sentence that shows the problem, or the sentence that promises what is missing.
  Emphasis marks, quotation marks, case and line breaks are ignored when the app compares it; any
  other change makes it stale.
- `Severity:` is `high` (the feature would ship broken or contradict its spec), `medium` (a
  reader or builder would trip on something missing or under-specified) or `low` (wording,
  consistency).
- `Source:` is optional. When present, write `Source: review <YYYY-MM-DD>` with today's date.
  Never write a `Source:` line that names a `REVIEW-` file; the app writes those when it imports
  a run review.
- `Status:` is `open` or `dismissed: <reason>`. `addressed <tokens>` and `unaddressed: <reason>`
  are written by an address session, never by this skill.
- Then one text paragraph: what is wrong or missing, where it showed, and what should change.
  Keep it on one line. Never start it with `#`, a backtick, `~` or a backslash, and never with a
  metadata key (`About:`, `Evidence:`, `Severity:`, `Source:`, `Status:`). The ledger parser reads
  a leading `#` as a heading, a leading backtick or `~` as a code fence, and those keys as
  metadata lines, so the entry would split or change meaning. Begin with a word instead.

Leave addressed and dismissed entries as they are.

## Limits

Write only `doc/plans/<feature>/findings.md`. Never write a `Settled:` line, a spec, a plan, a
brief, a run review, `doc/loom/knowledge/` or `doc/loom/summaries/`. Never stage, commit, or
branch, and never run `loom init`, `loom run`, or `loom pressure`. Report any other problem
`loom plan check` shows to the user without editing it.
