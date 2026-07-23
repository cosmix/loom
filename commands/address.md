---
description: Address the pressure-test review for a plan
argument-hint: [plan-path]
---

read the review of your plan. It is at codex-<basename of $1> in the SAME directory as the plan (for example, when $1 is doc/plans/PLAN-foo.md the review is doc/plans/codex-PLAN-foo.md). Read $1 (the plan file) alongside it.

⚠️ IF THAT REVIEW FILE DOES NOT EXIST, STOP. It means the reviewer failed to produce one. Reply saying exactly that and change NOTHING — not the plan, not planning-omissions.md. Do NOT substitute a review of your own: this command's only job is to fold in someone else's findings, and an unreviewed edit pass on the plan is worse than no pass at all.

Work through each point in the review. Validate each point against the ACTUAL code and the sibling plans BEFORE accepting it — reviews are sometimes written against a stale draft (line references drift; a point may already be folded in) and are sometimes wrong. If a point is valid, fix the plan to address it. If a point is not valid or already addressed, explain why in your reply — not in the plan.

When an accepted fix touches a contract another plan provides or consumes (symbol names, signatures, file ownership, seams), fold the fix in AND state the required sibling-plan amendment explicitly in the plan as a blocking note — never silently diverge from a sibling.

After folding all fixes, run two closing sweeps: (1) self-consistency — rg each changed claim across the whole plan file and reconcile prose ⇄ YAML (a half-applied correction is worse than none); (2) coverage — every capability the updated prose promises must appear in a stage's artifacts:/wiring:/acceptance; a fix that lands only in prose is invisible to loom's gates.

⚠️ THE PLAN MUST STAY SELF-CONTAINED: fold every accepted fix directly into the plan file, because the executor reads only the plan. Do not move required detail into external notes.

Record any omissions, mistakes, or bad assumptions this review surfaced about your original plan in planning-omissions.md at the root of the repo. If the file is still there, append to it.
