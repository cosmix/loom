---
description: Address the pressure-test review for a plan
argument-hint: [plan-path]
---
read the review of your plan. It is at codex-<basename of $1> in the SAME directory as the plan (for example, when $1 is doc/plans/PLAN-foo.md the review is doc/plans/codex-PLAN-foo.md). Read $1 (the plan file) alongside it.

Work through each point in the review. If a point is valid, fix the plan to address it. If a point is not valid, explain why in your reply — not in the plan.

⚠️ THE PLAN MUST STAY SELF-CONTAINED: fold every accepted fix directly into the plan file, because the executor reads only the plan. Do not move required detail into external notes.

Record any omissions, mistakes, or bad assumptions this review surfaced about your original plan in planning-omissions.md at the root of the repo. If the file is still there, append to it.
