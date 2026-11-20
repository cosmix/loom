# Terra worker brief — read-only forward lifecycle and poll discipline

## Outcome and ownership

Terra owns only new `loom/src/commands/subagents/forward_jobs.rs`, changes to
`loom/src/commands/subagents/{mod.rs,classify.rs,render.rs,table.rs,summary.rs}` and their
tests, `loom-hooks/poll-guard.sh` and its tests, plus
`agents/loom-codex-forwarder.md` and
`loom/src/orchestrator/signals/format/codex.rs`. Do not change `ledger.rs` or
any Sol-owned model/hook/usage file.

Terra adds its new poll-guard* hook test registrations to
`loom-hooks/tests/run-all.sh` in wave 2, only after Sol's unit has actually
settled (ownership transfer per common.md). The stage YAML `files` list omits
`run-all.sh`; the plan-level `loom-hooks/**` grant covers the write, and
`loom stage amend` cannot change `files`.

The adapter is read-only. It must never invoke Sol's writer, persist an
observation, run Codex/companion, cancel, retry, scan provider directories, or
read unrelated background tasks. It can reconstruct ephemeral observations
from the same exact transcript/marker contract for status only.

`summary.rs::with_last` and `empty` construct every `SubagentSummary`; update
both if lifecycle metadata is added. Enumerate all `SubagentState` matches and
summary constructors before adding a variant/field. Keep ordinary final-report
harvesting separate from success of an underlying forwarded job.

## Reader state model

1. Read `ForwardReceipt` and join by its deterministic `receipt_id`, which
   derives only parent session, agent, tool-use, stage, and Loom session. Never
   join by model, effort, timestamp, task filename, or newest ordering.

2. Validate the exact private backend locator before reading the exact backend
   record. No locator, unsafe path, malformed record, mismatched ID, or absent
   marker means `forward-unknown`; it is not evidence that the worker stopped.

3. Overlay after `SubagentStop`'s existing transcript override
   (`classify.rs:256-291`): only a non-forwarder with no expected forwarding
   tool use leaves old state unchanged. A known forwarder or validated forwarding
   tool use with no receipt is expected-but-unknown and cannot settle. Exact
   queued/running is `forward-wait`; exact completed plus valid terminal
   evidence and transcript done is `done`; failed/canceled is
   `forward-failed`; unknown/timed-out/missing state is non-settling
   `forward-unknown`. An empty transcript directory or wrapper completion does
   not override an expected receipt.

   Before any empty-directory/no-session success path, load the bounded expected
   receipt index scoped to the requested stage/session. Persisted expectations
   survive a daemon restart; unknown or active entries still block settlement.
   Enumerating this scoped Loom receipt index is allowed; enumerating provider
   job/rollout directories or selecting recency-based evidence is forbidden.
   A failed/canceled worker makes watch finish nonzero (1), not ordinary success.

4. Add `loom subagents wait --receipt <id> --timeout <secs> [--json]`. Read
   only the named receipt/backend. Exit 0 succeeded, 1 failed/canceled, 2
   queued/running/unknown at deadline. Report only receipt ID, backend ID, and
   state. No cancellation or recovery side effects.

5. Keep `watch`'s current two-second interval and debounce/tool-wait rules
   (`render.rs:14,175-212`). It settles only when transcripts and expected
   receipts settle; unknown holds until its existing timeout (2). Preserve
   default human `list` output/exit 0; add compact forward state only for a
   known forwarder and full IDs only to JSON.

## Poll guard and doctrine

Count only semantic `loom subagents list` with a fixed tested set of read-only
display pipelines. Reuse the tokenizer. Do not count generic loom, wait,
watch, harvest, redirections/state-changing forms, or quoted task-prompt text.
Keep the existing third/fourth warning and fifth denial
(`poll-guard.sh:132-155`). When a known exact receipt exists, guide to one
named wait; otherwise say unresolved and require explicit recovery. Internal
two-second watch polling is not repeated Bash polling.

Doctrine preserves one foreground Bash tool call at 600000ms. Its wrapper
internally launches and waits for one exact job; a natural harness background
acknowledgement is an optional visibility aid, never a completion claim. Remove
newest-job/rollout recovery and any retry language.

## Focused tests

Test no-write adapter behaviour, deterministic-ID joins, same-model sibling
isolation, safe locator rejection, active receipt blocking watch after
SubagentStop, all named-wait exits, foreground inline marker and natural
background fallback, unknown/empty-dir/debounce regressions, two-second watch
cadence, list compatibility, and semantic list-pipeline thresholds. The
orchestrator owns verification.
