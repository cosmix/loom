# W2 — usage window filtering, peak-context section, named worker ids

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first.

## Goal

`loom usage` measures the two things this plan is judged by, and `loom subagents watch` accepts
the worker ids the harness gives named agents. Evidence: report sections 4.9 and 4.10.

## Files you own (write)

- `loom/src/commands/usage/sections/lifecycle.rs`, `sections/agents.rs`, `sections/mod.rs`,
  `sections/peaks.rs` (new), and tests beside them
- `loom/src/commands/subagents/wait/model.rs`, `wait/identity.rs`, `wait/tests.rs`

Read-only: `commands/usage/transcript.rs`, `transcript_types.rs`, `sections/lengths.rs`,
`models/forward_receipt.rs`.

## Defect 1 — lifecycle and agents ignore the time window

Observed 2026-09-18: `loom usage --since 2026-08-28 --json` and `--since 2026-09-17 --json` both
report `lifecycle.subagent_transcripts: 505` with identical class counts, and the same
`agents.subagent_transcripts`. Cause: rows are window-filtered in `transcript.rs::scan_value`
(167-194), which leaves out-of-window transcripts present with zero entries, and
`lifecycle.rs::build` (41-57) and `agents.rs::build` (56-75) count transcripts, not entries.
Fix: both sections skip a transcript that has no in-window request. If the prompt-class
classification in `lifecycle.rs` reads the first user row, make sure that row is still available
for a transcript that straddles the window start; check how `scan_value` treats it. Add a test
module for `lifecycle.rs` (it has none) and extend `agents.rs` tests (`mod tests` at 335).

## Feature — peak resident context by scope

New section `peaks`, following the module convention in `sections/mod.rs::build` (35-55):
`pub fn build(&[Transcript]) -> Peaks`, `pub fn render(&Peaks)`, a field on the serialised
`Report` (18-33). For each transcript with in-window requests take the maximum
`TokenUsage::resident()` (`transcript_types.rs:24-28`; `lengths.rs:152,171` shows the per-request
pattern). Report per `Scope` (main, subagent): count, p50, p90, max, share above 250,000, share
above 400,000, and for subagents the share whose peak is below twice their first request's
resident size (the boot-dominated share). If `lengths.rs` or a sibling section already has a percentile helper, reuse it; otherwise add
one private function (nearest-rank on a sorted vector).

## Defect 2 — named worker ids

`WorkerSpec::from_str` (`wait/model.rs:35-50`) rejects ids failing `is_safe_id`
(`models/forward_receipt.rs:32-39`: ASCII alphanumerics plus `. _ -`). The harness gives a named
agent the id `<name>@session-<8 hex>` (observed: `census@session-a23f0a9e`), and that agent's
transcript file is `agent-a<name>-<16 hex>.jsonl` (observed:
`agent-aground-hooks-ee46a7e13ef80a9e.jsonl`); an unnamed agent's id `a1255b1022dc461fd` maps to
`agent-a1255b1022dc461fd.jsonl`. Do not loosen `is_safe_id`: it guards file names in about 30
places. In `WorkerSpec::from_str`, for the `claude` kind only, accept `<name>@session-<hex>` when
both halves pass `is_safe_id`, keep `<name>` as the lookup key and mark the spec as named. In
`wait/identity.rs` (path built at 289 as `agent-{id}.jsonl`) resolve a named spec by listing the
session's `subagents/` directory for `agent-a<name>-*.jsonl`; zero or several matches is exit 5
(unknown identity), never success.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib commands::usage` and
`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib commands::subagents::wait`
— each run once. Tests use fixture transcripts in a temp dir; never read `~/.claude`.
