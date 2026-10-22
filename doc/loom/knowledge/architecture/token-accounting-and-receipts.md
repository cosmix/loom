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
- loom-hooks/pre-compact.sh
- loom-hooks/poll-guard.sh
- loom-hooks/codex-forward-guard.sh
- agents/loom-codex-forwarder.md
- loom-hooks/_read_ledger.sh
- loom-hooks/knowledge-orient.sh
verified: c8c167bb2dbba1c5ec65b7b9c61130873a249da2
---
# Token Accounting And Receipts

> Usage ledger, compare, cache

## Scope and Evidence

The token-accounting surface: the provider ledger behind `loom usage`, the offline comparison and
run-evidence producer, the certified criterion cache, and two delivery lifecycles (forward
receipt, worker brief). Every line cited here was re-read at the revision in this page's
`verified` frontmatter. That is a source check, not a runtime proof: the open runtime question is
in the last section. The PATH `loom` can predate these commands; the clap definitions in source
are authoritative.

## Hook Phases

| Event (hook) | Delegate | Effect |
| --- | --- | --- |
| PreToolUse:Read (`read-guard.sh`) | none (shell only) | rule 1 line-count advice and the sibling-read advisory from `_read_ledger.sh`; no Rust delegate runs |
| PostToolUse (`post-tool-use.sh`) | `forward-receipt` | after a Bash call containing `codex-forward.sh task`; the PostToolUse payload arrives on stdin, no flags; bound 5 s |
| PostToolUse (`post-tool-use.sh`) | `context-ceilings` | ceiling pair, cached per session; bound 3 s |
| PreToolUse:Task/Agent (`spawn-guard.sh`) | `worker-brief` | files the delivery record and returns the brief after the untyped-spawn gate passes; Codex-bound types are skipped; bound 5 s |
| PreCompact (`pre-compact.sh`) | `pre-compact` | stage sessions: deletes the session's prompt delivery record; bound 3 s |
| SessionStart, source `compact` (`knowledge-orient.sh`) | `pre-compact` | ordinary sessions, which have no PreCompact hook: same reset; bound 3 s |

Registrations: `loom/src/fs/permissions/hooks/config.rs` (global guards) and the session
`HookEvent` set. Every delegate runs through `loom_run_bounded` and every failure is a silent
no-op: `post-tool-use.sh` discards the delegates' stderr, so a delegate that never writes looks
exactly like one that has nothing to write (see the forward-receipt tokenizer bug in
[Tests That Cannot Fail](../mistakes/tests-that-cannot-fail.md) and the worker-brief bound in
[Worker Briefs Vanished Under a Fixed Hook Bound](../mistakes/hooks-shell-portability.md#worker-briefs-vanished-under-a-fixed-hook-bound-2026-10-20)).

## Provider Ledger (`loom usage`)

`loom usage` normalizes each provider's telemetry into `NormalizedEvent` rows and a
`ProviderLedger` (`loom/src/commands/usage/provider_types.rs`, schema 1). The neutral
`provider_data` module owns shared session/Codex inventory and Claude normalization;
CLI, status and live token discovery consume it without core-to-command dependencies. The
clap surface is `UsageArgs` in `commands/usage/args.rs`. Rules:

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
  vector by `(timestamp, line_ordinal)` (`provider_data::supersedes`, consumed by
  `transcript_merge.rs` and the live tracker). Nested cache-write counters supply an absent
  flat total through the shared `provider_data::claude` parser. When the 5m/1h cache-write
  split contradicts the cache-creation total, the split is nulled rather than guessed; the legacy
  `TokenUsage` keeps the reconciled value (`claude_provider.rs::claude_tokens`).
- **Codex.** `CodexDirect` rows normalize `token_usage_record` `payload.usage`. `CodexFallback` rows
  normalize `token_count` `last_token_usage` and use the last/total pair only to suppress consecutive duplicate
  snapshots (`loom/src/commands/usage/codex_provider.rs:128-208`). Fallback rows remain coverage evidence, tagged
  `FallbackCoverage` and reported as `fallback_coverage_totals`; they are not derived cumulative deltas.
- **Execution receipts** (`--receipts-root`, `loom/src/models/execution_receipt.rs`, schema 1) are
  a protocol for an external producer; nothing in the tree writes them. A 5m/1h split mismatch
  marks the split unknown instead of rejecting the receipt (`receipt_provider.rs`).
- **Attribution.** `Attribution.stage_state` is `Known`, `Unknown` or `NotApplicable`. New
  attribution never infers a stage from a project slug. The requested model of a subagent row
  comes from `spawn_join.rs`, which pairs hook spawn requests to Claude lifecycle starts by
  agent type and second-resolution timestamps; `ambiguous` marks a pair it cannot decide.
- **Project transcripts.** `--project` (default: this repository) reads the repository's slug,
  its live `.worktrees/*` slugs and the slugs of worktrees loom has since removed
  (`<slug>--worktrees-<stage>` under the Claude root; the separator keeps a sibling project
  `<slug>-other` out, `discovery.rs`).
- **Completeness and Codex scope.** Each provider reads `measured` or `partial` with reason codes,
  including the evidence-loss reasons for unparseable lines, unreadable session records and
  damaged forward receipts. `fallback_coverage_rows` is a coverage count, never a reason. A Codex
  row counts for a project only through the threads its forward receipts name; without readable
  receipts the scope is `Unavailable`, and `--all` is unscoped. Scope and session-record
  reading use only the current `.loom/work`, so a window before the last `loom init` reads 0
  Codex tokens. Reasons, windows and the session-record read:
  [Usage Evidence](usage-evidence.md).
- **Forward join.** `--forward-receipts-root` must be the selected project's own state root, even
  without `--project`, and conflicts with `--all`; it attaches forward receipts to forwarder
  transcripts and Codex threads (`forward_join.rs`). Join failures are counted in diagnostics.
- **Time.** `--since` takes a duration (`7d`), an ISO date or an RFC3339 instant; `--until` is an
  inclusive RFC3339 instant (`time_range.rs`). Both bound event timestamps, not file mtimes.
- **Run evidence** (`--run-evidence <declaration.json> --project <dir> --since <rfc3339> --until
  <rfc3339>`) writes the `RunEvidence` that `--compare` pairs, measuring only the stages that
  started inside the window, refusing in-scope usage from an undeclared provider and mapping
  review severities to critical, high, medium and low
  ([details](usage-evidence.md#run-evidence)).
- **Quota history** (`loom/src/quota/history.rs`). The poller appends a point after each successful
  poll (`record_successful_observation`, best effort); `read_history` never mutates the cache and
  reports `malformed`, `unsupported_schema` and `nonmonotonic` row counts plus a source state.
  Continuity between two points compares both windows: a `resets_at` more than
  `RESET_TOLERANCE_SECS` (300) apart is `Reset`; a missing window on one side, a missing reset
  time, or a lower `used_percent` under the same reset is `Unknown`, and `Unknown` wins over
  `Reset` ([quota-poller.md](quota-poller.md#history-consumers)).

## Offline Comparison (`loom usage --compare`)

`loom usage --compare <artifact.json> [--json]` evaluates a version-1 paired experiment offline. It
discovers no transcripts, polls no provider, and changes no model, cache policy or stage; it never
rolls anything back.

**Artifact contract** (`commands/usage/comparison_schema.rs`, `deny_unknown_fields`):
`schema_version: 1`, an `intervention` manifest and `pairs`. The manifest names the intervention,
the baseline and candidate implementation revisions, the changed dimensions
(`implementation-revision`, `provider-assignment`, `model-assignment`, `effort-assignment`), and
optionally `token_rule` and `latency_tolerance_ms`; any unlisted workload, input, environment,
contract, model or effort difference makes a pair incomparable. Each pair holds an opaque
`work_unit_id` plus `baseline` and `candidate` runs. A run
records workload and fixture ids, source-input revision and digest, environment and
acceptance-contract ids, implementation revision, provider/model/effort assignments, acceptance and
semantic-completion evidence, required checks, independent review dimensions, unresolved findings,
omissions, retry/fix counts, both latency measures, accounting provenance, provider-native token
vectors and optional quota intervals. Identifiers are bounded and may not contain path separators,
so prompts, output, paths, secrets, prices and account identity stay out of the artifact.
Artifacts over 16 MiB are refused.

**Token-proxy verdict** (`comparison_token.rs`): raw normalized dimensions compared within each
provider. Claude and Codex vectors stay separate, composite totals are not added to their
components, Codex reasoning is not added to output, and Codex tokens never pass through Claude
S1/S2/S3 weights. With no `token_rule`, an improvement needs no increase in any comparable
dimension and a strict decrease in at least one; a provider transfer, a cache-category trade-off,
a missing dimension or an unchanged vector is inconclusive.

**Declared net-total rule** (`"token_rule": "net-total"`, `comparison_net.rs`): per provider, a
pair's net is `resident_input_tokens + output_tokens`, and the rule judges the mean
baseline-minus-candidate difference over all usable pairs against 2 standard errors (sample
variance, `n - 1`): `reduction`, `regression` or `within-noise`. A provider with fewer than 3
pairs is `insufficient-pairs`, one with an absent dimension is `missing-dimension`
(`missing-token-dimension`), and any pair whose two runs name different providers is
`cross-provider-transfer`; all are inconclusive. A regression rejects (`net-total-regression`),
noise is inconclusive (`net-total-within-noise`). The rule replaces dominance for tokens only;
report-level reason codes still list pair-level dominance reasons next to a supported verdict.
Quality and the quota check keep their per-pair veto: any rejected pair quality or rejected
subscription verdict rejects the report, while an inconclusive subscription verdict does not
hold back net-total support (quota resolves whole percents only).

**Latency.** Without `latency_tolerance_ms`, each pair compares critical-path and notification
latency strictly (any increase rejects, an absent value is `missing-latency-evidence`). With it,
`comparison_latency.rs` judges the mean increase across pairs against the tolerance for each
measure: over it rejects (`critical-path-regression`, `notification-latency-regression`). A
measure absent from every pair is unmeasured: critical path is `missing-latency-evidence`
(inconclusive); notification is reported as `notification-latency-unmeasured` and does not
block support. A measure present in some pairs only is `missing-latency-evidence`. Produced
run evidence always carries `notification_latency_ms: null`, so it needs the declared tolerance
to reach `supported-candidate`.

**Subscription verdict**: quota observations only, never token weights or API prices. Support
needs matching provider/window sets and reset identities, same-reset continuity, known precision,
no unrelated concurrent use, no worse consumption in any interval, and a reduction larger than the
combined measurement uncertainty in at least one. Two `resets_at` values within
`RESET_TOLERANCE_SECS` (300 s) are one reset, and interval lengths match when they differ by at
most one poll interval (`POLL_INTERVAL`, 180 s; `comparison_quota.rs`). A token-proxy improvement
never becomes a subscription verdict on its own; the overall verdict also needs complete quality
and latency evidence. With no declared rule, one regression in any pair rejects the candidate (no
averaging); "no averaging" holds only for an artifact that declares neither `token_rule` nor
`latency_tolerance_ms`.

**Exit codes**: 0 supported, 1 rejected (an observed quality, latency or consumption regression), 2
inconclusive, including malformed or oversized artifacts (reason code in `--json`).

Implementation choices behind it:

- A duplicate run id is a data-integrity failure, so it is inconclusive, never a rejection
  (`comparison_validation.rs`).
- Comparison mode rejects explicit selection flags (transcript selection, provider, root, project,
  stage, plan, window); `--json` stays allowed. `UsageArgs` keeps them as `Option` and applies
  defaults at the use site, so `has_explicit_selection` (`usage/args.rs`) is a pure function
  of what clap parsed; an argv rescan against a hand-kept flag list would drift silently.
- The artifact reader is local to `comparison.rs` (`take(limit + 1)` plus a post-read recheck,
  16 MiB). `fs/safe_read.rs::read_bounded` cannot serve it: it takes a root plus a relative path
  and opens every component no-follow, so an operator path through a symlinked `/tmp` would be
  refused.

The evidence that feeds it is produced by `--run-evidence` ([Live Paired Trials](usage-evidence.md#live-paired-trials)).

## Criterion Cache Contract

`loom/src/verify/criteria/cache_contract.rs` defines `CriterionContract`, a lossless description
of everything that changes a verdict (command, simple or extended kind, expected exit, output
predicates, empty-stderr flag, timeout, confinement, input fingerprint). Only a certified, fully
evaluated pass is stored (`CACHE_RECORD_VERSION` 4, 4 KiB diagnostic tails); unknown eligibility
misses. The key hashes `PATH` and the resolved executable identity, so any test that asserts a hit,
a miss or key stability must be `#[serial]` against tests that rewrite `PATH`.

## Forward Receipt Lifecycle and Exact Wait Authority

- **Identity.** `ForwardIdentity` (`loom/src/models/forward_receipt.rs`) is parent session,
  agent id, tool-use id, stage id and loom session; the receipt id is the SHA-256 hex of those
  fields under the `loom.forward-receipt.v1` tag.
- **File.** `.loom/work/subagents/<stage>/forward-receipts.jsonl`, one JSON observation per line,
  folded into a `ForwardReceipt`. States: `queued`, `running`, `succeeded`, `failed`, `canceled`,
  `timed_out` (a companion job the wrapper cancelled at its own deadline), `unknown`
  (`ForwardState`); `is_terminal()` is `succeeded`, `failed`, `canceled` or `timed_out`.
  Backends: `companion`, `direct`.
- **Writer.** `loom hook forward-receipt` (`loom/src/commands/hook/forward_receipt.rs`) reads the
  PostToolUse payload on stdin (bounded at 4 MiB; `post-tool-use.sh` pipes it after a Bash call
  containing `codex-forward.sh task`) and needs `LOOM_WORK_DIR`, `LOOM_STAGE_ID` and
  `LOOM_SESSION_ID`. `decode_payload` (`models/forward_receipt/payload.rs`) takes the identity
  and the wrapper's stdout markers from the payload; only forwarder subagents run forwards, so a
  payload without `agent_id` forms no receipt, and an absent `tool_response` decodes to no result
  and writes nothing. It recognises the forwarding call through `forward_argv`
  (`models/forward_receipt/transcript/argv.rs`), which accepts the three forms
  `codex-forward-guard.sh` authorizes: 8 words, 10 words (adds `--unit-id <unit>`) and the
  12-word form `emit_authorized_input` rewrites to (adds `--invocation-id inv-<32 hex>`), with
  word 0 checked by `is_forward_wrapper`. Every observation of one call carries the PostToolUse
  instant as `observed_at`, an end time; `same_fact` dedupes observations on every field but `observed_at`,
  so a hook retry appends nothing. A companion END marker with outcome
  `timed_out` records `timed_out` with exit 124. It appends under its own sidecar lock with an
  `O_NOFOLLOW` regular-file append, since `fs::locking` opens targets with a plain `File::open`
  that follows symlinks. `exact_forward_argv` (8 words only) is separate and still serves the
  transcript classifier in `commands/subagents/forward_jobs_transcript.rs`
  ([Delivery-Proof and Quota-Continuity Defects](../concerns/token-accounting-and-proof-defects.md#delivery-proof-and-quota-continuity-defects)).
  Stderr of the delegate is discarded by the shell. A damaged line in the receipts file stops all
  receipt formation for that stage (persist refuses to append) and fails every Codex wait on it.
- **Wait.** `loom subagents wait --receipt <64 hex> [--timeout <secs>] [--json]`
  (`loom/src/commands/subagents/forward_jobs_wait.rs`) polls every 2 s, default timeout 300 s, and
  never starts, cancels or retries anything. The stage comes from `LOOM_STAGE_ID`, else from a scan
  of `subagents/*/forward-receipts.jsonl` for the id. `wait_exit_code` (`forward_jobs_wait.rs:143`):
  exit 0 succeeded, 1 failed, canceled or timed out, 2 queued, running or unknown.
- **Who waits.** The orchestrator, only, and only through one bound `loom subagents watch --worker
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
  [\Subagent Hierarchy](../patterns/subagent-hierarchy.md).
- **Status overlay.** For a `loom-codex-forwarder`, the daemon-reconciled Codex lifecycle outcome
  alone decides `SubagentState`, via an intermediate `ForwardOverlay`
  (`loom/src/commands/subagents/forward_jobs.rs::marker_overlay`/`overlay_from_state`):
  `Queued`/`Running` -> `ForwardWait`; `Succeeded` -> `Done` if the transcript itself is done, else
  `ForwardWait`; `Failed`, `Canceled` and `TimedOut` all collapse to one `ForwardFailed`; `Unknown`
  -> `ForwardUnknown`. `classify_forward.rs::state_for` (`:79`) then maps that overlay 1:1 to
  `SubagentState::{Done,ForwardWait,ForwardFailed,ForwardUnknown}` — there is no direct
  `Failed->Failed`/`Cancelled->Cancelled` case. The legacy receipt/marker overlay applied after
  structural transcript classification survives only as the diagnostic `forward` field, and does not
  decide state. A damaged receipt index is an unresolved observation, never a partial success.
  Poll-guard counts repeated `loom subagents list` and names the exact wait when receipts exist
  (`poll-guard.sh`).

## Worker Brief Lifecycle

`loom/src/commands/hook/worker_brief.rs` gives each typed Task/Agent worker a scoped retrieval
brief. `spawn-guard.sh` runs `loom hook worker-brief` only after its untyped-spawn gate passes, and
skips it for Codex-bound types (`loom-codex-forwarder`, `codex:*`): Codex units read the brief from
loom's Codex `UserPromptSubmit` hook (`fs/permissions/codex_hooks.rs:114-121`). The delegate builds
a query from the stage, the worker description and paths the prompt declares (plus
`StageQuery::dependency_paths`), caps the query text at `WORKER_QUERY_MAX_BYTES` (2,048, cut with
`floor_char_boundary`), calls `retrieve_for_stage` once per spawn, drops plan documents from the
pack (required items removed that way are reported as unmet), and returns `{nonce, brief}`.
`issue` returns nothing when the pack is empty with no declared skills, when the encoded envelope
exceeds `max_payload_bytes` (16,384), or when `store_record` fails. On success `store_record` files
`<key>.json` at issue under a directory lock, keyed by
`worker_recipient_id(stage, parent session, nonce)`; no `.pending` file and no bind step exist, and
`subagent-start.sh` does not call the delegate. `spawn-guard.sh` bounds the delegate at 5 s, accepts
only an object with exactly `brief` and a 32-hex `nonce`, and appends the brief to every non-Codex
typed spawn. A delegate killed after `store_record` leaves a record for a brief it never carried;
that is accepted because a delivery record is an optimisation.

Worker records are loaded by `load_deliveries` (`context/delivery.rs:248-262`), so a dependent
stage's `dependency_chunk_ids` sees what its dependencies' workers were given. They suppress
nothing for the worker's own prompt hook: `delivered_to_session` matches only the session's prompt
recipient and the `LOOM_SESSION_ID` spawn record (`context/delivery/session.rs:104-126`), so a
suppression consumer for them is future work (see
[concerns](../concerns/token-accounting-and-proof-defects.md)).

**Why the bounds are what they are.** Retrieval time grows with query length, and
`spawn-guard.sh` kills the delegate at its bound with no brief and no record. Measured on the
recorded token-core spawn: a 2,048-byte query takes 1.66 s, about 15.6 KB takes 13 s, and a
12.1 KB envelope replay takes 7.8-9.6 s. The 2,048-byte cap and the 5 s bound keep the replay at
2.06 s on the release binary. A debug build runs retrieval about 4x slower (11.9 s), so time hook
delegates with a release build; see
[Worker Briefs Vanished Under a Fixed Hook Bound](../mistakes/hooks-shell-portability.md#worker-briefs-vanished-under-a-fixed-hook-bound-2026-10-20).

A Rust hook writer must resolve the state root through `WorkDir` the way `context::delivery` does:
inside a worktree `.loom/work` is a symlink, and loom's safe writers open roots `O_NOFOLLOW`, so a
writer that joins the raw env path silently writes nothing.

`plan/schema/structural_checks/worker_table.rs` adds advisory warnings for an optional worker table
in a stage description (a path claimed twice, a path outside the stage's `files`). Real plan tables
are YAML-indented and have no Markdown divider row; the parser accepts that shape.

## Open Runtime Uncertainties

- **Does a failed forward fire PostToolUse?** `codex-forward.sh` exits 124 on its unit deadline
  (`codex-forward.sh:363`), and the receipt writer runs only from `post-tool-use.sh` on
  PostToolUse; no PostToolUseFailure hook is registered (`fs/permissions/hooks/config.rs`). If
  Claude Code fires only PostToolUseFailure for a Bash call that exits non-zero, neither the
  `running` nor the `timed_out`/124 line forms (the companion `timed_out` path is reached only
  through unit tests) and that forward's Codex rows read foreign. Receipts for successful
  forwards form (checked through the real hook and binary at integration-verify). Needs a live
  check by the operator; a stage session has no network.

## Read Advisories and Reread Measurement

No Rust delegate, intent or receipt store backs a read, and no rule judges a same-agent repeat.
The measured saving at stake is small: identical same-agent rereads are 29 of 7,612 stage Read
results, about 0.4% of Read characters. Why the earlier receipt path never worked:
[Read Receipts Could Not Form](../mistakes/store-without-consumer.md#read-receipts-could-not-form-and-fixtures-wrote-the-pair-before-completion-2026-10-20).

`loom_read_discipline_check` (`loom-hooks/_read_discipline.sh`) keeps rules 0 and 0b (exemptions for
a skill's `SKILL.md` and `INDEX.md`), 1 (a whole read over `READ_GUARD_LINE_LIMIT` lines is
redirected to `loom map --outline`), 3 (an unscoped tier-1 knowledge read) and 4 (a whole-file read
a sibling agent of the session already requested of the unchanged file is advised once per agent
and path). The number 2 is deliberately unused. Rule 4 reads the sibling ledger written by
`_read_ledger.sh` (`_loom_read_sibling_request`); the advisory says the file was requested whole by
N agents, an attempt and not proof of delivery. The ledger is
`$LOOM_WORK_DIR/hooks/reads/<session>/<agent>.tsv` inside a stage and a private
`$TMPDIR/loom-reads/` directory outside one.

Rereads are measured after the fact: the `reads` section of `loom usage`
(`loom/src/commands/usage/sections/reads.rs`) counts native and shell reads from transcripts. If
that measurement shows same-agent rereads above 2% of Read characters, the revival design is
completion at the agent's next Read, from a transcript offset recorded at prepare time.

## Signal Caps and the IV Gate

`loom/src/orchestrator/signals/tests_size.rs` pins `CLAUDE_MD_TEMPLATE_MAX_BYTES` at 20,480, `STABLE_PREFIX_MAX_BYTES` at 8,192, `STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` at 10,240 and `PLAN_OVERVIEW_MAX_BYTES` at 4,096. The overview cap mirrors `MAX_PLAN_OVERVIEW_BYTES` in `orchestrator/signals/generate/context.rs:98`. Trim doctrine before raising a cap.

The IV stable prefix (`INTEGRATION_VERIFY_OVERRIDE`, `orchestrator/signals/cache.rs`) has the orchestrator run `loom check integration-verify` once; that check carries the complete suite. Reviewers run only targeted discriminating checks, which never substitute for that gate. After a review fix the orchestrator re-evaluates invalidated checks, and only the repaired criteria cache decides reuse.
