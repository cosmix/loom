# C9: README plan check section

Codex unit (gpt-6.1-terra, effort high). Read doc/plans/briefs/plan-documents/common.md first; its Public API section is authoritative.

## Files owned

`README.md` only.

## Files to read

`doc/plans/briefs/plan-documents/checklist-check/w3-command.md` section "Docs", `README.md` lines 452-470 ("Plan Commands").

## Entry points

The "Plan Commands" section of README.md.

## Steps

1. Add `loom plan check [--json]` to the command block.
2. Add one paragraph: what it prints (the seven-step checklist, the current step, offers, convention problems), that it reads only files and needs no server, exit codes 0, 1 and 2, and that --json is the schema the planning app and sessions read.
3. Check the markdown renders as plain prose (no new headings beyond what exists).

## Done when

The orchestrator runs `rg -qF "loom plan check" README.md` and reads the paragraph.

## Traps

Plain prose: no stock phrases, no marketing words, no emoji. Every fenced block names its language.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Run compile checks only (cargo check --tests from loom/); the orchestrator runs the tests after you.
- Leave a 10 percent margin under the limits: files at most 360 lines, functions at most 45 lines.
- Double any literal brace inside format! strings. Put CLI placeholders in rustdoc inside backticks.
- Every public item gets a rustdoc comment. Touch no file outside the Files owned list.
- Heading detection always goes through `crate::planning::sections::sections_of` (fence aware);
  never scan hash lines yourself. Lines in public types are 0-based.
