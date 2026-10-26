# C10: loom-usage skill rows

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first; its Public API section is authoritative.

## Files owned

`skills/loom-usage/SKILL.md` only.

## Files to read

`doc/plans/briefs/plan-documents/checklist-check/w3-command.md` section "Docs", `skills/loom-usage/SKILL.md` around line 684 (the plan verify notes).

## Entry points

The plan verify material near line 684 of skills/loom-usage/SKILL.md.

## Steps

1. Add a short paragraph: planning sessions run `loom plan check` after editing planning documents; a non-zero exit names convention problems to fix (exit 1) or a missing repository (exit 2).
2. Keep the surrounding structure untouched.
3. Proofread.

## Done when

The orchestrator runs `rg -qF "loom plan check" skills/loom-usage/SKILL.md`.

## Traps

Plain prose. This file is installed into every user setup by dev-install; keep it short.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
