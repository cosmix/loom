---
description: Pressure-test a plan with an agent team
argument-hint: [plan-path]
---
read doc/loom/knowledge. then read $1 (the plan file) and use an agent team to explore the codebase. Your task is to pressure-test the plan: hunt for omissions, wrong assumptions, missing wiring, unhandled edge cases, and any step an executor could not complete from the plan alone. Validate every finding against the actual code BEFORE acting on it.

Update the plan in place with the validated findings.

⚠️ THE PLAN MUST STAY SELF-CONTAINED. A downstream agent executes this plan reading ONLY the plan file — it sees none of this conversation and none of your scratch files. Fold every fix directly into the plan; never push required detail into a separate document the executor will not read.

Record any omissions, mistakes, or bad assumptions you find in your original plan in planning-omissions.md at the root of the repo. If the file is still there, append to it. Do NOT read planning-omissions.md first: form your critique from the plan and the code, not from a prior round's notes, so each pass is an independent perspective.
