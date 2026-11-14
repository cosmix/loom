---
name: address-findings
description: Load when a planning session addresses a feature's open, verified findings with a team, editing the specs and plans they are about and recording each outcome in the findings ledger.
---

# Address the findings

The opening names the feature, its ledger (`doc/plans/<feature>/findings.md`), the findings to
address (each `F-<n>` with the documents it may change), the teammate count and the write scope.
The app chose these findings because each is open and its `Evidence:` quote is in a passage its
`About:` names. The listed documents and the ledger are the complete write scope under `doc/`;
the planning hook refuses a write to any other path. Never try to work around a refusal.

## Lead

1. Read the ledger entries the opening names: `About:`, `Evidence:`, `Severity:` and the text.
   Read each passage they are about, the vision and its `## Decisions`, and the metaplan.
2. Recheck each finding against the passage before assigning it. One that no longer holds, or
   that only a product choice the user has not made can settle, is not assigned: record it in
   step 5 as `unaddressed`.
3. Create one team with the stated teammate count. Give each document exactly one owner, and
   give a teammate every finding whose first editable document it owns, with the finding's id,
   `About:`, `Evidence:`, severity and text. A finding that needs a change in a document another
   teammate owns stays with its first owner, who asks that owner with `SendMessage` for the
   companion change. With no teammates, address the findings yourself under the same rules.
4. Wait for every teammate's report. Each report names, per finding, the headings it changed or
   why it could not.
5. Write the ledger, alone: on each named finding set its `Status:` line, and change nothing else
   in the file.
   - `Status: addressed <tokens>`: the nodes the change touched, comma-separated, in the
     `About:` token forms (`section_04#rebuild-budget`, `PLAN-<slug>.md`, `vision`,
     `metaplan#<entry anchor>`); anchors name headings as they read after the change.
   - `Status: unaddressed: <reason>`: one line saying what is in the way, such as a locked plan,
     a product choice to make, or a finding that no longer holds. An unaddressed finding stays
     out of later runs until the user reopens it on the Findings page, so say in the reason what
     must change first.
6. Run `loom plan check` and fix what it reports about the documents in scope and the ledger.
7. Report one row per finding: id, outcome, the headings changed or the reason. Name each
   settled document the run edited ("section 04 was settled; it now reads revised") and every
   open question a teammate added.

## Teammates

Edit only your own documents, and only as far as your findings require: the passage the evidence
quotes, the stub or `Covers:` line that must stay true with it, and the cross-references the
change breaks. Keep settled decisions and the document's established terms. A choice the finding
leaves open goes under the document's `## Open questions` as a list item with the options and
your recommendation; it does not get decided by the edit. Never write or edit a `Settled:` line.
Never edit the ledger: report to the lead instead.

## Limits

Write only the documents the opening lists; only the lead writes the ledger. Never write
`doc/loom/knowledge/`, `doc/loom/summaries/`, a run review or a brief outside the scope. Never
stage, commit or branch, and never run `loom init`, `loom run` or `loom pressure`. The lead never
starts a second team. If the user says "Stop the team", end it; edits already made stay, and the
lead marks each finding it has no report for `unaddressed: the team was stopped`.
