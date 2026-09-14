---
aliases:
- loom usage
- read receipts
- forward receipts
- subagents wait
- worker brief
sources:
- loom/src/commands/usage/mod.rs
- loom/src/commands/usage/provider_types.rs
- loom/src/commands/usage/provider_normalization.rs
- loom/src/commands/usage/transcript_merge.rs
- loom/src/commands/usage/claude_provider.rs
- loom/src/commands/usage/codex_provider.rs
- loom/src/commands/usage/receipt_provider.rs
- loom/src/commands/usage/forward_join.rs
- loom/src/commands/usage/comparison.rs
- loom/src/commands/usage/comparison_validation.rs
- loom/src/models/execution_receipt.rs
- loom/src/quota/history.rs
- loom/src/verify/criteria/cache_contract.rs
- loom/src/models/forward_receipt.rs
- loom/src/commands/hook/forward_receipt.rs
- loom/src/commands/subagents/forward_jobs_wait.rs
- loom/src/commands/subagents/classify_forward.rs
- loom/src/context/read_receipts.rs
- loom/src/context/read_receipts/storage.rs
- loom/src/context/read_receipts/transcript.rs
- loom/src/commands/hook/worker_brief.rs
- loom/src/commands/hook/pre_compact.rs
- loom/src/plan/schema/structural_checks/worker_table.rs
- loom/src/orchestrator/signals/tests_size.rs
- loom/src/orchestrator/signals/cache.rs
- loom/src/orchestrator/signals/format/codex.rs
- loom-hooks/post-tool-use.sh
- loom-hooks/read-guard.sh
- loom-hooks/_read_discipline.sh
- loom-hooks/spawn-guard.sh
- loom-hooks/subagent-start.sh
- loom-hooks/pre-compact.sh
- loom-hooks/poll-guard.sh
- loom-hooks/codex-forward-guard.sh
- agents/loom-codex-forwarder.md
verified: 499b09b6297aeee4896a66df3da86d00f652a618
---
# Token Accounting And Receipts

> Usage ledger, --compare, criterion cache, receipts, exact waits

## Scope and Evidence

What PLAN-token-optimization-2026-09-13 built to measure consumption and to stop paying for
repeated work: the provider ledger behind `loom usage`, the offline comparison, the certified
criterion cache, and three receipt lifecycles (forward, read, worker brief). Every line cited here
was re-read at the revision in this page's `verified` frontmatter. That is a source check, not a
runtime proof: two read-receipt assumptions about Claude Code's transcript are still unmeasured
(last section). The PATH `loom` can predate these commands; the clap definitions in source are
authoritative.

## Hook Phases

| Event (hook) | Delegate | Effect |
| --- | --- | --- |
| PreToolUse:Read (`read-guard.sh`) | `read-receipt --check`, then `--prepare` | rule 2 asks for proven deliveries; then a pending intent is written |
| PostToolUse (`post-tool-use.sh`) | `read-receipt --complete` | turns the intent into a receipt once the result is correlated |
| PostToolUse (`post-tool-use.sh`) | `forward-receipt --transcript` | after a Bash call containing `codex-forward.sh task`; bound 5 s |
| PostToolUse (`post-tool-use.sh`) | `context-ceilings` | ceiling pair, cached per session; bound 3 s |
| PreToolUse:Task/Agent (`spawn-guard.sh`) | `worker-brief` | issues a nonce-bound brief after the untyped-spawn gate passes |
| SubagentStart (`subagent-start.sh`) | `worker-brief --bind-agent` | binds the issued brief to the started child |
| PreCompact (`pre-compact.sh`) | `pre-compact` | rotates the session's read-receipt epoch |

Registrations: `loom/src/fs/permissions/hooks/config.rs` (global guards) and the session
`HookEvent` set. Every delegate runs through `loom_run_bounded` and every failure is a silent
no-op: `post-tool-use.sh` discards the delegates' stderr, so a delegate that never writes looks
exactly like one that has nothing to write (see the forward-receipt tokenizer bug in
[Tests That Cannot Fail](../mistakes/tests-that-cannot-fail.md)).

## Provider Ledger (`loom usage`)

`loom usage` normalizes each provider's telemetry into `NormalizedEvent` rows and a
`ProviderLedger` (`loom/src/commands/usage/provider_types.rs`, schema 1). Rules:

- **Providers stay separate.** `Provider` is Claude or Codex; `--provider` defaults to claude.
  Claude and Codex vectors are never added together or translated into each other's accounting.
- **Unknown is not zero.** Every `ProviderTokenVector` field is an `Option`; a missing dimension
  stays missing through totals (`MeasuredTotal`).
- **Provenance.** Only `MeasuredCanonical` counts as measured (`ProvenanceStatus::is_measured`).
  Rows sharing a request/response id with identical timestamp and tokens keep the first as
  canonical and mark the rest `DuplicateExact`; any disagreement marks the whole group
  `AmbiguousConflict` (`provider_normalization.rs::classify_by_id`). Other states: `Synthetic`,
  `ZeroUsage`, `UnknownUsage`, `FallbackCoverage`.
- **Claude streams.** Duplicate rows of one streamed message merge to the latest whole usage
  vector by `(timestamp, line_ordinal)` (`transcript_merge.rs:10-31`). When the 5m/1h cache-write
  split contradicts the cache-creation total, the split is nulled rather than guessed; the legacy
  `TokenUsage` keeps the reconciled value (`claude_provider.rs::claude_tokens`).
- **Codex.** `CodexDirect` rows come from per-turn `last_token_usage`; `CodexFallback` rows derive
  from cumulative `total_token_usage` and are coverage evidence only, tagged `FallbackCoverage` and
  reported as `fallback_coverage_totals` (`codex_provider.rs`).
- **Execution receipts** (`--receipts-root`, `loom/src/models/execution_receipt.rs`, schema 1) are
  a protocol for an external producer; nothing in the tree writes them. A 5m/1h split mismatch
  marks the split unknown instead of rejecting the receipt (`receipt_provider.rs`).
- **Attribution.** `Attribution.stage_state` is `Known`, `Unknown` or `NotApplicable`. New
  attribution never infers a stage from a project slug.
- **Forward join.** `--forward-receipts-root` must be the selected project's own state root, even
  without `--project`, and conflicts with `--all`; it attaches forward receipts to forwarder
  transcripts and Codex threads (`forward_join.rs`). Join failures are counted in diagnostics.
- **Time.** `--since` and `--until` bound event timestamps, not file mtimes.
- **Quota history** (`loom/src/quota/history.rs`). The poller appends a point after each successful
  poll (`record_successful_observation`, best effort); `read_history` never mutates the cache and
  reports `malformed`, `unsupported_schema` and `nonmonotonic` row counts plus a source state.
  Continuity between two points compares both windows: a changed `resets_at` is `Reset`; a missing
  window on one side, a missing reset time, or a lower `used_percent` under the same reset is
  `Unknown`, and `Unknown` wins over `Reset`.

## Offline Comparison (`loom usage --compare`)

The artifact contract, verdicts and canary protocol live in `doc/token-optimization-evaluation.md`.
Implementation choices behind it:

- Exit 0 supported, 1 rejected (an observed quality, latency or consumption regression), 2
  inconclusive. A duplicate run id is a data-integrity failure, so it is inconclusive, never a
  rejection (`comparison_validation.rs`).
- Comparison mode rejects explicit selection flags. `UsageArgs` keeps them as `Option` and applies
  defaults at the use site, so `has_explicit_selection` (`usage/mod.rs:147-160`) is a pure function
  of what clap parsed; an argv rescan against a hand-kept flag list would drift silently.
- The artifact reader is local to `comparison.rs` (`take(limit + 1)` plus a post-read recheck,
  16 MiB). `fs/safe_read.rs::read_bounded` cannot serve it: it takes a root plus a relative path
  and opens every component no-follow, so an operator path through a symlinked `/tmp` would be
  refused.

## Criterion Cache Contract

`loom/src/verify/criteria/cache_contract.rs` defines `CriterionContract`, a lossless description
of everything that changes a verdict (command, simple or extended kind, expected exit, output
predicates, empty-stderr flag, timeout, confinement, input fingerprint). Only a certified, fully
evaluated pass is stored (`CACHE_RECORD_VERSION` 2, 4 KiB diagnostic tails); unknown eligibility
misses. The key hashes `PATH` and the resolved executable identity, so any test that asserts a hit,
a miss or key stability must be `#[serial]` against tests that rewrite `PATH`.

## Forward Receipt Lifecycle and Exact Wait Authority

- **Identity.** `ForwardIdentity` (`loom/src/models/forward_receipt.rs:79-129`) is parent session,
  agent id, tool-use id, stage id and loom session; the receipt id is the SHA-256 hex of those
  fields under the `loom.forward-receipt.v1` tag.
- **File.** `.loom/work/subagents/<stage>/forward-receipts.jsonl`, one JSON observation per line,
  folded into a `ForwardReceipt`. States: `queued`, `running`, `succeeded`, `failed`, `canceled`,
  `unknown`; the first three after `running` are terminal. Backends: `companion`, `direct`.
- **Writer.** `loom hook forward-receipt --transcript <path>`
  (`loom/src/commands/hook/forward_receipt.rs`) reads the forwarder transcript and the wrapper's
  markers, and appends under its own sidecar lock with an `O_NOFOLLOW` regular-file append, since
  `fs::locking` opens targets with a plain `File::open` that follows symlinks. Its argv parser
  matches `codex-forward-guard.sh`'s authorization boundary exactly (the 8-word wrapper argv, the
  same quoting rules): a reader wider than the guard would record forwards the guard never allowed.
- **Wait.** `loom subagents wait --receipt <64 hex> [--timeout <secs>] [--json]`
  (`loom/src/commands/subagents/forward_jobs_wait.rs`) polls every 2 s, default timeout 300 s, and
  never starts, cancels or retries anything. The stage comes from `LOOM_STAGE_ID`, else from a scan
  of `subagents/*/forward-receipts.jsonl` for the id. Exit 0 succeeded, 1 failed or canceled, 2
  queued, running, unknown or timed out.
- **Who waits (owned-waits contract, 2026-09-13, supersedes the untimed-blob-scan and bare-timeout
  forms below).** The orchestrator, only, and only through one bound `loom subagents watch --worker
  claude:<agent-id> --worker codex:<unit-id> --timeout 3600` — one `--worker` per worker, `--session`
  naming only the Claude parent UUID. The forwarder makes ONE foreground wrapper call; if the harness
  backgrounds it, the forwarder makes no further tool call and ends its turn
  (`agents/loom-codex-forwarder.md:54-62`). `codex-forward-guard.sh` authorizes only the exact
  wrapper argv and one forward per forwarder transcript; `has_prior_forwarding_call` allows the call
  when that transcript is missing or unreadable, so the harness writing the transcript after
  PreToolUse cannot block every first forward. The orchestrator's doctrine
  (`orchestrator/signals/format/codex.rs:146`) names one `--worker codex:<unit-id>` per forwarded
  unit; the bare `--timeout <secs>`-only form with no `--worker` is REJECTED
  (`loom/src/commands/subagents/mod.rs`), and `codex-companion.mjs status --all` is never used. Exit
  2 (deadline passed) is not proof any worker died — see
  [Subagent Hierarchy](../patterns/subagent-hierarchy.md).
- **Status overlay.** For a `loom-codex-forwarder`, the daemon-reconciled Codex lifecycle outcome
  alone decides `SubagentState`: `Active`->`ForwardWait`, `Unknown`->`ForwardUnknown`,
  `Succeeded`->`Done`, `Failed`->`Failed`, `Cancelled`->`Cancelled`
  (`loom/src/commands/subagents/classify_forward.rs:30`); the legacy receipt/marker overlay applied
  after structural transcript classification survives only as the diagnostic `forward` field, and
  does not decide state. A damaged receipt index is an unresolved observation, never a partial
  success. Poll-guard counts repeated `loom subagents list` and names the exact wait when receipts
  exist (`poll-guard.sh`).

## Read Receipt Lifecycle

`loom/src/context/read_receipts.rs` proves that a `Read` result was delivered before a repeat read
is warned or denied. The shell attempt TSV is no longer a repeat qualifier.

1. **Prepare** (PreToolUse). An intent records the identity (session, agent, normalized path,
   range), the source generation and a nonce.
2. **Complete** (PostToolUse). The receipt exists only when the `Read` tool_use and its tool_result
   sit in adjacent rows of the bounded transcript tail (`read_receipts/transcript.rs:33-37`) and the
   source generation is unchanged. It records a text or media class, the result hash and size, and
   up to 8 distinct proven tool-use ids (`MAX_PROVEN_TOOL_USES`).
3. **Check.** `--check` returns the number of proven deliveries only for a schema-2, text-class
   receipt whose identity and generation are current; media, persisted or uncorrelatable results
   never justify reuse. Rule 2 (`_read_discipline.sh`): a full read proven once warns, proven
   twice or more denies; a range read proven twice or more warns.
4. **Reset.** The generation includes a per-session epoch that `loom hook pre-compact` rotates, so
   compaction makes every earlier receipt ineligible; a failed rotation is a no-op.

Storage is `$LOOM_WORK_DIR/hooks/reads/<session>/receipts` inside a stage, else a private
`$TMPDIR/loom-reads/uid-<uid>/session-<sha256>/receipts` (created 0700 and validated). Bounds:
64 KiB per receipt, 128 KiB per record, 64 directory entries. A payload without `cwd` parses to
nothing, so every mode silently no-ops: fixtures must carry the common hook fields. Bash-side
`cat`/`head`/`tail`/`sed` repeats never yield a receipt and no longer escalate under rule 2
(accepted behavior change; `poll-guard.sh` calls the discipline check without a payload).

## Worker Brief Lifecycle

`loom/src/commands/hook/worker_brief.rs` gives each typed Task/Agent worker a scoped retrieval
brief. `spawn-guard.sh` runs `loom hook worker-brief` only after its untyped-spawn gate passes.
The delegate builds a query from the stage, the worker description and paths the prompt declares,
calls `retrieve_for_stage` once per spawn, drops plan documents from the pack (required items
removed that way are reported as unmet), and returns `{nonce, brief}` capped at the retrieval
payload limit. It stores a `<key>.pending` delivery record under a directory lock, keyed by
`worker_recipient_id(stage, parent session, nonce)`. `spawn-guard.sh` accepts only an object with
exactly `brief` and a 32-hex `nonce`. On SubagentStart, `subagent-start.sh` passes the child
transcript (`*/subagents/agent-<id>.jsonl`, regular file, not a symlink) to
`worker-brief --bind-agent`, which finds the nonce marker and renames the record to `<key>.json`
with the agent as recipient. Worker-bound records have no compaction reset; no caller re-checks
them today.

A Rust hook writer must resolve the state root through `WorkDir` the way `context::delivery` does:
inside a worktree `.loom/work` is a symlink, and loom's safe writers open roots `O_NOFOLLOW`, so a
writer that joins the raw env path silently writes nothing.

`plan/schema/structural_checks/worker_table.rs` adds advisory warnings for an optional worker table
in a stage description (a path claimed twice, a path outside the stage's `files`). Real plan tables
are YAML-indented and have no Markdown divider row; the parser accepts that shape.

## Signal Caps and the IV Canonical Verifier

The plan changed no prompt budgets. `loom/src/orchestrator/signals/tests_size.rs` pins
`CLAUDE_MD_TEMPLATE_MAX_BYTES` 29,696, `STABLE_PREFIX_MAX_BYTES` 7,168,
`STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` 10,240 and `PLAN_OVERVIEW_MAX_BYTES` 4,096, which
mirrors `MAX_PLAN_OVERVIEW_BYTES` in `orchestrator/signals/generate/context.rs:98`. Trim doctrine
before raising a cap.

The IV stable prefix (`INTEGRATION_VERIFY_OVERRIDE`, `orchestrator/signals/cache.rs:112-123`) names
ONE canonical verifier that runs the complete suite per immutable tree, environment and criterion
contract; other reviewers run only targeted discriminating checks, which never substitute for the
gate. After a review fix the owner re-evaluates invalidated checks, and only the repaired criteria
cache decides reuse.

## Open Runtime Uncertainties

- Receipts correlate only ADJACENT tool_use and tool_result rows. If Claude Code writes parallel
  `Read` calls and their results as separate non-adjacent rows, parallel reads never form a receipt:
  fail-closed, but the repeat-read saving then covers serial reads only.
- Whether Claude Code writes the tool_result row before PostToolUse hooks run is unproven. If it
  does not, `--complete` never correlates and repeat escalation never fires; the forward-receipt
  writer reads the transcript at the same point.

Both need sampling of real transcripts before any savings claim; see the canary protocol in
`doc/token-optimization-evaluation.md`.
