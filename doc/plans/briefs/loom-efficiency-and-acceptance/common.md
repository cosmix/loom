# Common brief — loom efficiency and acceptance

Read this before your own brief. It holds the decisions and contracts that cross worker and
stage boundaries. Evidence for every item: `doc/REPORT-loom-improvement-findings-2026-09-18.md`.

## Operator decisions (2026-09-18) — settled, do not reopen

1. No context budget on a stage's main session. Task size is governed by subagent granularity:
   a subagent should typically finish under about 400,000 tokens, and every spawn pays a boot
   cost (median first request about 28,000 tokens), so splitting too finely is waste as well.
2. Opus stays the default main-session tier for standard stages; the operator overrides per stage.
3. Delegation is a cost decision, tokens times model tier. The main agent makes a very small
   change itself when a spawn would cost more. A main session on fable delegates even those.
4. `CLAUDE.md.template` is installed as the global instructions file and is read in every
   session, including sessions outside any loom plan. No guidance may be lost: text moves to the
   surface where it applies, and every moved paragraph is listed in the mover's report.
5. `commit-filter.sh` stays a hard block on AI attribution. Only its message text and Rule 9 change.
6. No codex lane in this plan. Every worker is a haiku, sonnet or opus subagent.

## Numbers used in more than one place (one value each)

| Name | Value | Used by |
| --- | --- | --- |
| Small-change test, lines | at most 20 changed lines | BLOCK-B point 1, file-guard advisory |
| Small-change test, files | at most 2 files already read this session | BLOCK-B point 1, file-guard advisory (warns on the 3rd distinct file) |
| Subagent typical completion | under about 400,000 tokens | BLOCK-B point 3, plan-writer rubric |
| Subagent boot cost | about 28,000 tokens | BLOCK-B point 1, plan-writer rubric |
| Template size ceiling | 20,480 bytes | `tests_size.rs`, doctrine stage acceptance |
| Tier-2 knowledge file limit | 400 lines | `fs/knowledge/catalog/size.rs` |
| Tier-2 knowledge section limit | 80 lines | `fs/knowledge/catalog/size.rs` |
| Sibling-read advisory floor | files above 200 lines | read receipts |

## Cross-stage contracts

**Subagent preamble file (hook-guards produces, doctrine-surfaces pins).**
Path `loom-hooks/_subagent-preamble.txt`. Line 1 is exactly the current `PREAMBLE_LINE` of
`loom-hooks/spawn-guard.sh` (the sentence beginning `CLAUDE.md is already in your context`). The
file contains BLOCK-A and BLOCK-D byte-for-byte as defined in
`loom/src/orchestrator/signals/tests_doctrine_blocks.rs`. It is registered in
`loom/src/fs/permissions/constants.rs` like the other sourced hook files so it installs beside
`spawn-guard.sh`.

**Declared skills (plan-verification produces, doctrine-surfaces documents).**
Stage field `skills: [<skill-name>, ...]`, optional, default empty. Names are full skill names
(`loom-rust`). `loom plan verify` reports an unknown name as an error when the skill index loads,
and a warning when it cannot be loaded.

**Knowledge check baseline (knowledge-hygiene produces, doctrine-surfaces documents).**
Flags `loom knowledge check --baseline <file>` and `--write-baseline <file>`. File format: one
line per structural issue, the issue's stable sort key as `order.rs` already defines it, `#`
comments and blank lines ignored. With `--strict --baseline F` the command fails only on
structural issues absent from F. A missing F is an empty baseline. Issues present in F and gone
from the tree print one "baseline can be tightened" line and do not fail.
Canonical path in plans: `doc/loom/knowledge/check-baseline.txt`.

**Memory grouping (knowledge-hygiene produces, doctrine-surfaces documents).**
`loom memory pending --group` prints pending entries grouped as `corrections` (text starts
`stale-knowledge:`; target parsed as `<file>#<heading>`), `mistakes` (`mistake:`), `decisions`,
`other`. `--json` carries the same grouping.

**Hook session ledgers (hook-guards, shared by H1 to H5).**
`_loom_ledger_file <kind> <agent_id> <fallback_session_id>` in `loom-hooks/_read_discipline.sh`
(135-154) prints the ledger path: `$LOOM_WORK_DIR/hooks/<kind>/<LOOM_SESSION_ID>/<agent_id>.tsv`
in a stage, a per-session file under `${TMPDIR:-/tmp}/loom-<kind>/` outside one. H5 owns that
file and keeps the name, the three arguments and the one-path-on-stdout output unchanged; it
only changes the out-of-stage layout to one directory per session with one file per agent. H1 to
H4 source the file and call the function with new kinds (`skills`, `spawns`, `edits`, `tools`,
`bigout`); they do not edit it and must not assume the out-of-stage layout. `poll-guard.sh:82`
shows a caller. `loom_is_subagent "$INPUT_JSON"` (`_common.sh:1503`) tells main agent from
subagent.

## BLOCK-B replacement text (doctrine-surfaces only)

BLOCK-B is pinned byte-for-byte by `tests_doctrine.rs`. Points 4 and 5 keep their current text.
Points 1 to 3 become:

```text
1. DELEGATION IS A COST DECISION: TOKENS TIMES MODEL TIER (hard stop 6). A
   stage's main agent decomposes the work, briefs subagents, verifies and
   commits. A spawn costs a written brief, the subagent's boot (about 28,000
   tokens before it reads anything) and a harvest turn. The main agent makes a
   change itself only when ALL of these hold: at most 20 changed lines, in at
   most 2 files it has already read this session, no further exploration, and
   one command proves it. Anything larger is delegated. A main session running
   FABLE delegates even those: a cheaper tier can do them, and every fable turn
   costs more than the spawn.
2. INVESTIGATION ENDS IN A BRIEF OR IN A SMALL CHANGE. The moment you finish
   reading the code and know what the fix is, apply point 1's test. If it
   fails, you are at the delegation boundary: write the understanding down
   (file:line, root cause, the change to make, signatures, patterns to match,
   acceptance) and spawn. The diagnosis being yours does not make a large
   change yours.
3. EVERYTHING BEYOND POINT 1 IS DELEGATED, to as FEW subagents as the work
   allows, at the CHEAPEST tier that can do the piece. Size each assignment so
   the subagent typically finishes under about 400,000 tokens, and never split
   below what that needs: every extra spawn pays the boot cost again. Pick PER
   SUBAGENT by what that piece needs, never once for the whole stage, and
   default downward: HAIKU (`model: haiku` on loom-software-engineer) for
   mechanical edits such as a rename or a config value; codex gpt-5.6-luna for
   boilerplate, scaffolding, and simple unit tests; SONNET
   (loom-software-engineer) or codex gpt-5.6-terra for common implementation and
   integration tests — this is the default lane and most work belongs here; OPUS
   (loom-senior-software-engineer) for mainstream architecture and algorithm
   implementation; FABLE only for visual/UI design, a bug that survived a
   delegated fix attempt, or extremely challenging algorithmic design. Codex
   tiers (effort xhigh, via loom-codex-forwarder) exist only on stages listing
   codex in implementers AND when the codex CLI + plugin are installed;
   otherwise that work goes to sonnet (loom warns at startup when a stage lists
   codex it cannot use). Verification NEVER delegates - the orchestrator
   verifies and commits. Spawn BY AGENT TYPE.
```

`tests_doctrine.rs` asserts BLOCK-B contains the live codex constants from `loom/src/codex.rs`;
the text above keeps both model names and `xhigh`.

## Rules for every worker

- Files stay under 400 lines and functions under 50 (`loom/tests/maintainability.rs` enforces
  this against `loom/maintainability-baseline.txt`). Split a module before it crosses.
- Hook scripts must pass `bash scripts/check-hook-syntax.sh` and stay portable: read
  `doc/loom/knowledge/mistakes/hooks-shell-portability.md` headings before editing one.
- A hook that inspects a Bash command uses the shared tokenizer and
  `strip_embedded_content` from `loom-hooks/_common.sh`
  (`doc/loom/knowledge/patterns/hook-content-stripping.md`). No new raw-regex command matchers.
- Tests never spawn a process that can outlive the test, and never touch the live `.loom/work`.
- Edit repo files only. Never edit installed copies under `~/.claude/`.
- No mention of any AI system in code, comments, docs or test names.
