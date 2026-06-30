---
name: pressure
description: Pressure-test a loom plan and write a review for the plan's author. Trigger when the user invokes $pressure followed by a plan path. The argument after $pressure is the plan file path.
---

The user has provided a plan path as the argument after $pressure. Treat that argument as <PLAN>.

read doc/loom/knowledge. then read <PLAN> (the plan file) and use an agent team to explore the codebase. Your task is to pressure-test the plan: hunt for omissions, wrong assumptions, missing wiring, unhandled edge cases, and any step an executor could not complete from the plan alone. Validate every finding against the actual code before reporting it.

Write your report to a file named codex-<basename of <PLAN>> in the SAME directory as the plan (for example, when <PLAN> is doc/plans/PLAN-foo.md write doc/plans/codex-PLAN-foo.md). Overwrite that file if it already exists. Make every finding concrete: cite files and line numbers, and say what the plan should add or change. Provide a high-level summary here as well.

⚠️ Do NOT edit the plan itself. Your only written output is the review file — the plan's author reads your review and decides what to fold back into the plan.
