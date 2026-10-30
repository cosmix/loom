# C6: README and loom-usage summary text

Codex unit (gpt-6.1-terra, effort high). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md` first; its Format rules and Public API sections are authoritative.

## Files owned

`README.md` and `skills/loom-usage/SKILL.md` only.

## Files to read

`doc/plans/briefs/feat-planner-04-summary-store/summary-store/w3-commands.md` section "Docs", `README.md` "Plan Commands" (plan 1 added loom plan check there).

## Entry points

The "Plan Commands" section of README.md.

## Steps

1. README.md "Plan Commands": add the four `loom plan summary` subcommands to the command block (seal with `[--force]`) and one short paragraph: drafts and claims live under `~/.loom/plan/<project-key>/`, the commands need no server, exit codes 0, 1 and 2, and seal refuses a summary another session claimed.
2. skills/loom-usage/SKILL.md, after the loom plan check paragraph: sessions rebuild summaries with `loom plan summary brief`, write the draft, and run `loom plan summary seal` until it is accepted; status lists what is out of date.
3. Proofread both.

## Done when

The orchestrator runs `rg -qF "loom plan summary" README.md` and `rg -qF "loom plan summary" skills/loom-usage/SKILL.md`.

## Traps

Keep the existing structure; add, do not rewrite.

## Rules for this unit

- Never run git. Never read or write any path under a .loom directory.
- Plain prose: no stock phrases, no marketing words, no emoji. Every fenced block names its language.
- Touch no file outside the Files owned list.
