---
aliases:
- read gate
- read_gate
- deny-once
- read guard
- attribution
- read:* rows
sources:
- loom-hooks/_read_discipline.sh
- loom-hooks/_read_ledger.sh
- loom-hooks/_containment.sh
- loom-hooks/poll-guard.sh
- loom-hooks/read-guard.sh
- loom/src/map/views/markdown_outline.rs
- loom/src/commands/usage/sections/reads.rs
- loom/src/commands/usage/sections/attribution/analyze.rs
- loom/src/commands/usage/sections/attribution/classify.rs
- loom/src/commands/usage/sections/attribution/startup.rs
- loom/src/commands/usage/accounting.rs
verified: 3e22bf84e13e65479097f1a8206ca9bd0158ff61
---

# Read Gate
>
>

The read gate redirects a whole read of a large file to `loom map --outline`. Rule 1 of
`loom_read_discipline_check` (`loom-hooks/_read_discipline.sh`) is deny-once inside a live stage. No Rust
delegate, intent or receipt store backs a read; why the earlier receipt path never worked:
[Read Receipts Could Not Form](../mistakes/store-without-consumer.md#read-receipts-could-not-form-and-fixtures-wrote-the-pair-before-completion-2026-10-20).
The measured saving at stake is small: identical same-agent rereads were 29 of 7,612 stage Read results,
about 0.4% of Read characters, so the gate is judged by the operator's before/after comparison and is not
assumed to save.

## Rules of `loom_read_discipline_check`

Rules 0 and 0b exempt a skill's `SKILL.md` and `INDEX.md`. Rule 1 redirects a whole read over
`READ_GUARD_LINE_LIMIT` lines (400) to `loom map --outline`. Rule 3 warns an unscoped tier-1 knowledge read
and overrides rule 1. Rule 4 advises, once per agent and path, a whole-file read that a sibling agent of the
session already made of the unchanged file; the number 2 is deliberately unused. Rule 4 reads the sibling
ledger written by `_read_ledger.sh` (`_loom_read_sibling_request`) and says the file was requested whole by N
agents, an attempt and not proof of delivery. Its ledger is `$LOOM_WORK_DIR/hooks/reads/<session>/<agent>.tsv`
inside a stage and a private `$TMPDIR/loom-reads/` directory outside one.

## Rule 1 Is Deny-Once

`loom_read_deny_once` (`loom-hooks/_read_ledger.sh`) denies (exit 2) an agent's first whole read of a path
inside a live stage and lets that agent's next whole read of the path pass. The key is the agent and the
absolute path, so a `Read` and a Bash `cat ../x` share one denial. The repeat is checked before the outline
runs, so it never pays for `loom map --outline`.

"Live" is `_loom_read_gate_live`: `LOOM_STAGE_ID` and `LOOM_MAIN_AGENT_PID` set, the main agent a live
ancestor of the hook, and no `read_gate = false` under `[hooks]` in `$LOOM_WORK_DIR/config.toml`
(`loom_read_gate_enabled`, `_containment.sh`; only an unambiguous `false` turns it off, any doubt leaves
it on). The graph's coverage and `[hooks] deny_enabled` do not decide it. Outside a live stage, with the
switch off, when the denial cannot be recorded, or when the ledger is full, the message is a warning.
Control inputs (`.loom/`, `$LOOM_WORK_DIR`, `doc/plans/`) are exempt (`_loom_is_read_gate_exempt`). The
exemption tests the path as given, so a bare relative `cat` of a plan file from inside `doc/plans` is not exempt
and costs one denial.

The denial ledger is `$LOOM_WORK_DIR/hooks/read-denials/<session>/<agent>.tsv`, one row per denied call.
`loom clean` and `loom init --clean` drop the `read_gate` setting, `loom config` does not manage it, and
editing the project config file during a host job fails that job with `verification policy changed`. The switch
therefore goes into `.loom/work/config.toml` after `loom init` and before `loom run`.

## Bash Reads

`poll-guard.sh` applies the same rule to Bash `cat`, `head`, `tail` and `sed -n` forms that read a whole file.
Byte counts, follow flags, and ranges are not whole reads (`_loom_read_bound_for_head_tail`,
`_loom_read_bound_for_sed`). A command whose stdout goes to a pipe or a file never reaches the agent
(`_loom_read_stdout_captured`, which reads `LOOM_TOKEN_SEPS`). A `cat a b` naming two large files is denied
once per file in sequence. Forms the gate does not see are listed in
[`loom ask` and Read Gate Concerns](../concerns/loom-ask-and-read-gate.md).

## Markdown Outlines Bypass the Graph

`loom map --outline <markdown-file>` as the only view is rendered straight from the file by `markdown_outline`
(`loom/src/map/views/markdown_outline.rs`, `markdown_outline_output` in `loom/src/commands/map.rs`); no source
graph is loaded. A large Markdown file therefore always has an outline in its denial, capped at 200 rows in
the denial text (other languages are not capped). Combined with another view the request takes the graph
path, and an outline of source code needs the graph to cover the file.

## Interface and Test Pins

`loom_read_deny_once <path> <lines> <agent_id> <fallback_sid>` takes no message: the ledger is checked
first, so the message, which needs the outline, is built afterwards by `_loom_read_discipline_verdict1`
(into `LOOM_RD_V1_MSG`, without emitting it). Contracts for the Markdown outline pin a heading row by its
start line and title only (`L250-L<title>`), never the end line or scope: the range comes from
`section_end` and the scope from the enclosing titles, so a nested heading's parent may span the file.

## Measuring the Gate

The `reads` section of `loom usage` (`loom/src/commands/usage/sections/reads.rs`) counts native and shell
reads from transcripts. `same_transcript_rereads` counts every repeat of a path within one transcript, so a
denied attempt plus its retry count as a reread: with the gate on, a nonzero count is partly the gate's own
cost, about one request (roughly 15K units at 150K resident) per denial. The
[Attribution](#the-attribution-section-of-loom-usage) section's `read:*` rows are the other half of the comparison. The
operator compares both for 14 days before and after install and counts the rows under
`.loom/work/hooks/read-denials/`; the gate stays on only when the `read:*` cost falls by more than those
denied calls cost. If same-agent rereads stay above 2% of Read characters with the gate off, the revival
design is completion at the agent's next Read, from a transcript offset recorded at prepare time.

## The Attribution Section of `loom usage`

The section (`loom/src/commands/usage/sections/attribution/`) estimates where a session's S2 cost went, by what
entered the context. It is an estimate from transcripts, not a provider measurement; the
[Provider Ledger](token-accounting-and-receipts.md) rules decide what counts as measured. Its `read:*` rows are
the cost half of the read-gate comparison.

### The Cost Model

An item (a tool result, user message or attachment) admitted after `k` requests is written to the cache by
request `k + 1` and read by the `n - k - 1` requests after that, because one request's resident context is the
next request's cache read (`Costs::admission_s2`, `analyze.rs`). Anything admitted before the first request is
startup: already inside that request's resident context, so it is reported apart from the rows. The weights
(input, cache read, 5-minute and 1-hour cache write, output) are defined once in `accounting.rs` and
`analyze.rs` uses them. What the definitions leave unexplained is the residual; an excess is reported as
over-attribution (`over_attributed_s2`) and never hidden in another transcript's residual. Assistant replay is
an upper bound, because thinking blocks may not be replayed in later requests. `startup_excluded` counts
transcripts with requests whose start is not provably in range.

### Rows

`classify.rs::tool_result` names each tool result from the call that produced it: `read:skill`,
`read:knowledge`, `read:plan-or-signal`, `read:source-other`, `bash:build-test`, `bash:loom`, `bash:git`,
`bash:search`, `bash:file-read`, `bash:other`, `agent-report`, `skill-tool`, `edit-ack`, `search-tool`,
`other-tool`. Bash rows are keyed by the command's first program word (`tools::program_words`, shared with the
tools section's `raw_command_key`, so `VAR=x` prefixes and `/usr/bin/rg` reduce to the program). At most 24
rows are kept; the tail merges into one `other` row. The `signals/` arm of `read:plan-or-signal` also matches
source paths such as `loom/src/orchestrator/signals/**`, so in this repository those reads land in that row.

### Startup Per Agent Type

`startup.rs` reports, per agent type, the transcripts counted, the median resident context of the first
request, and the largest attachment kinds inside it. Only fresh starts (`true_fresh_start == Some(true)`) are
counted. The haiku worker (`loom-software-developer`) is judged on this median.

### Reading It

- Admission and startup reads are charged for all remaining requests, including after a `compact_boundary`, so
  rows are overstated in compacted sessions and the excess shows only once a transcript goes negative. Treat
  `read:*` rows as estimates when a session compacted.
- The `agents` section's `by_agent_model` rows carry `transcripts`; divide a row's usage by it to compare agent
  types per transcript. A task respawned at a higher tier counts against the cheaper type.
- Read-gate evaluation uses the `read:*` rows together with the `reads` section (see Measuring the Gate above).
