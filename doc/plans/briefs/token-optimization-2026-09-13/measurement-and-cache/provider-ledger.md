# Worker brief — provider ledger (Sol)

Target plan: `PLAN-token-optimization-2026-09-13.md`.
Final brief path: `doc/plans/briefs/token-optimization-2026-09-13/measurement-and-cache/provider-ledger.md`.

You own the complete provider-aware `loom usage` implementation. Work as a
`loom-codex-forwarder` using `gpt-5.6-sol` at `xhigh` effort. Do not spawn
subagents, run git commands, read `CLAUDE.md`, or edit quota-history sources.
Read this brief in full before touching code. Start navigation at `UsageArgs`
in `loom/src/commands/usage/mod.rs`, `parse` in
`loom/src/commands/usage/transcript.rs`, and `StartedAgentTypeIndex` in
`loom/src/commands/subagents/ledger.rs`.

Composition contract: add `normalize_provider_events` under the owned usage
module and call it from `mod.rs::execute` before report rendering. The same
consumer calls `quota::read_history` for available local quota roots. Keep
normalization and quota reads offline and independently fixture-testable.

## Territory

You exclusively own:

- every file under `loom/src/commands/usage/`, including every existing usage
  test module;
- `loom/src/models/mod.rs`;
- new `loom/src/models/execution_receipt.rs` and its test module;
- `loom/src/commands/subagents/ledger.rs` and
  `loom/src/commands/subagents/ledger_tests.rs`.

Read-only collaborators: `loom-hooks/subagent-start.sh` already writes
`agent_id`, `agent_type`, `stage_id`, `parent_session_id`, `loom_session_id`,
and `ts`; do not change that hook. The quota worker owns all `loom/src/quota/`
files and must land its history reader before you call it. Define a usage-only
receipt protocol only. `models/forward_receipt.rs` does not exist at `7d6a14ca`
and is a job-lifecycle deliverable, not this stage's; its absence here is
expected. Do not create, import, or alter it, and do not fabricate any runtime
receipt producer.

## Current seams that must remain understood

`UsageArgs` currently supports only `--since`; `execute` parses it and passes a
single timestamp through `DiscoveryOptions` and `transcript::parse`.
Discovery currently rejects old-mtime files with `is_recent` before transcript
event timestamps are read. `transcript::add_value` only excludes timestamps
older than `since`. This is an admission optimization, not an authoritative
event-time boundary, and cannot remain the source of truth for a frozen range.

`Request` has one `TokenUsage` value. `merge_request` merges content blocks but
retains the first nonzero usage vector, replacing it only if it was wholly zero.
Do not replace this with field-wise maxima: actual streamed input/cache fields
may vary and a field-wise maximum makes a vector no provider emitted.

`TokenUsage` and the legacy `sections::Report` feed all legacy sections,
including `Accounting::of` in `accounting.rs`. Preserve their serialized fields
and legacy report shape. The existing S1/S2/S3 formulas remain Claude-only;
never apply their Claude price-like weighting to Codex.

`StartedAgentTypeIndex` currently establishes type only from the authoritative
`(parent_session_id, agent_id)` relation. It deliberately refuses ambiguous
scoped rows and refuses legacy fallback where any scoped row exists. Preserve
those safety rules when carrying stage metadata.

## Public CLI and input-root contract

Extend `UsageArgs` with these new fields:

```rust
#[arg(long)]
pub until: Option<String>;
#[arg(long, value_enum, default_value_t = ProviderSelection::Claude)]
pub provider: ProviderSelection;
#[arg(long)]
pub claude_root: Option<PathBuf>;
#[arg(long)]
pub codex_root: Option<PathBuf>;
#[arg(long)]
pub receipts_root: Option<PathBuf>;
```

`ProviderSelection` is a **new** `clap::ValueEnum` with exactly `Claude`,
`Codex`, and `All`, serialized/kebab-cased as `claude`, `codex`, and `all`.
The default is `claude` for output compatibility. `--until` accepts a UTC RFC
3339 instant (including `Z`); reject a timezone-less instant, a malformed
bound, and `until < since` with a helpful error. The range is inclusive at both
ends: `since <= event_timestamp <= until`. Date-only `--since` keeps its
existing UTC-midnight behavior; duration `--since` is resolved once at command
start. A missing `--until` remains open-ended.

Introduce a **new internal** `TimeRange` in the usage module rather than
threading loose timestamps. It owns `since` and optional `until`, has an
`includes(DateTime<Utc>) -> bool` predicate, and is passed to discovery and all
parsers. Raw UTC event timestamps are always authoritative for `--since`, with
or without `--until`: scan every candidate JSONL under the selected root and
apply `TimeRange` to each event. Do not use filesystem mtime as an admission
filter or default fast path. Any file/row decoding failure is a diagnostic, not
an implicit out-of-range result.

Explicit roots are read-only test seams, not environment rewrites. With an
explicit root, do not read or write `HOME`, alter environment variables, or
fall back to a different root. Existing default Claude discovery still resolves
the ordinary Claude projects root when `--claude-root` is absent. Codex default
discovery must cover both `~/.codex/sessions` and
`~/.codex/archived_sessions`; an explicit `--codex-root` is the parent that
contains `sessions/` and optional `archived_sessions/`. `--receipts-root` is a
directory of only the new receipt protocol. Missing optional roots yield an
empty provider input plus a diagnostic, never a synthetic request.

## New receipt protocol — define only, do not produce it yet

Add `pub mod execution_receipt;` to `loom/src/models/mod.rs`. In the new module
define and test these **new** serde types, using explicit `schema_version`:

```rust
pub const EXECUTION_RECEIPT_SCHEMA_VERSION: u16 = 1;
pub struct ExecutionReceipt {
    pub schema_version: u16,
    pub provider: ReceiptProvider,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub request_id: Option<String>,
    pub usage: ReceiptUsage,
}
pub enum ReceiptProvider { Claude, Codex }
pub struct ReceiptUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub output_tokens: u64,
    pub thinking_output_tokens: Option<u64>,
    pub cache_write_5m_input_tokens: Option<u64>,
    pub cache_write_1h_input_tokens: Option<u64>,
}
```

Use a decoder that rejects an unsupported `schema_version`, missing provider or
timestamp, negative/non-integer token values, and thinking greater than output.
It must preserve absent optional fields as unknown, not coerce them to zero.
Keep receipt paths, malformed-receipt count, unsupported-version count, and
unattributable receipts as sanitized diagnostics only; never emit receipt body,
prompt, tool input, or an absolute transcript path in JSON output.

This receipt carries request usage only: it has no job, stage, Loom session, or
agent lifecycle identity. Do not use it to define or pre-empt the distinct new
`models/forward_receipt.rs` forward/job-stage protocol; that lifecycle protocol
is out of this worker's territory and must remain independently versioned.

## Provider normalizers and provenance

Create new provider-normalization types separate from legacy `TokenUsage` and
make the JSON report contain a new, explicitly versioned provider section (for
example `provider_ledger: { schema_version: 1, ... }`). Existing legacy JSON
keys must remain present with their current names; only corrected counts may
change. The default `--provider claude` keeps the current legacy sections and
adds the new section. `codex` and `all` expose the new provider section; do not
invent Codex S1/S2/S3 fields or pretend a Claude accounting model is valid for
Codex.

Each normalized row needs provider, event timestamp, source kind, optional
request identity, raw token vector, role/scope attribution, and an explicit
provenance status. Separate measured values from unknown and ambiguous values.
Report request/response counts separately from synthetic/zero rows; never call
synthetic transcript rows API requests. All provider totals must derive only
from rows marked measured and canonical.

Claude rules:

1. Collect all same-message observations before choosing a canonical vector.
   Keep content-block merging behavior for tools/text/thinking.
2. For a normal stream, choose the last complete usage observation ordered by
   event timestamp and physical line order as a whole vector. Record first,
   last, and changed-field discrepancy counters. This corrects first-nonzero
   output/cache undercount without field-wise-max fabrication.
3. Deduplicate `assistant.message.id` globally across selected files, not only
   inside one transcript. Exact copies collapse. If identical IDs have
   irreconcilable order/timestamp or a conflicting terminal vector, do not pick
   a made-up winner: keep an ambiguous diagnostic and exclude it from measured
   totals. Rows with no ID remain file-and-line scoped and are not globally
   merged.
4. Parse Claude `output_tokens_details.thinking_tokens` when present as an
   actual output subfield. Validate `thinking <= output`; invalid/missing data
   is unknown and does not change output. Preserve cache creation total and its
   5m/1h split reconciliation invariant.

Codex rules, grounded in the retained rollout schemas:

1. `token_usage_record.payload.usage` is a per-response vector. Canonicalize
   by global `response_id`; absent IDs use `(file, ordinal)` only. Never sum
   `turn_token_usage`, `thread_token_usage`, or `total_token_usage`.
2. Fallback data is `event_msg` where `payload.type == "token_count"`, using
   `payload.info.last_token_usage`. It has no response ID. Exclude a fallback
   snapshot only if its immediately preceding physical token-count record has
   both identical `last_token_usage` and identical `total_token_usage`; an
   equal last vector with changed cumulative total is a distinct observation.
3. Mixed direct/fallback files are not safely fusable absent an addressable
   response join. Select direct rows for measured totals, retain fallback rows
   as a separate diagnostic coverage vector, and label the omitted relation
   unknown. Do not claim complete accounting, fill gaps, or use field maxima.
4. `reasoning_output_tokens` is an output subfield and must not exceed output.
   `total_tokens` is diagnostic only when it disagrees with explicit input plus
   output; provider totals use explicit fields.

Receipt records do not supply stage attribution. Transcript attribution extends
the existing starts index:
return a **new** metadata result containing agent type plus optional `stage_id`
and `loom_session_id`, keyed by `(parent_session_id, agent_id)`. Conflicting,
empty, obsolete `session_id`, or legacy-only rows retain the current unknown
behavior. Never infer a stage from project slug, stage-name family, prompt text,
or a quoted `LOOM_STAGE_ID`.

## Quota history join

After the Terra worker lands it, consume this proposed **new** quota contract:

```rust
pub fn read_history(
    work_root: &Path,
    provider: &str,
    since: i64,
    until: Option<i64>,
) -> QuotaHistoryRead;
```

Import no private quota cache helpers. `read_history` is read-only and returns
validated history points plus explicit diagnostics. Join points to the provider
ledger by provider and UTC time range only. Expose history as a separate
optional provenance section; it is quota state, not a request token source, and
must not change token totals. Missing, empty, corrupt, and unavailable history
are distinct diagnostic states, never zero quota use.

## Required report content

For each provider, emit machine-readable measured totals for fresh input, cache
creation/read, cache-write TTL split when known, output, thinking/reasoning,
and resident input. Also emit model/project/scope/role/day/week groupings,
request-length and resident-context distributions, first-observed versus true
fresh-start flags, tool counts, stage attribution states, cache/new-input
turnover ratios (never call them unique-content amplification), and every
normalization exclusion/ambiguity count. Do not emit raw transcript text,
prompt/tool payloads, absolute paths, secrets, rates, money, causal savings,
or a quality/latency conclusion.

For Codex specifically, `resident_input` is measured `input_tokens` inclusive
of cached input and `fresh_input` is `input_tokens - cached_input_tokens` only
when both fields are valid and ordered. A cache-creation/cache-write field
absent from Codex telemetry is unknown, not measured zero; do not require a
zero cache-creation total to accept an otherwise measured response.

## Tests and completion bar

Extend the existing `discovery_tests.rs`, `transcript_tests.rs`, `usage_tests.rs`
and section tests; add focused tests beside new modules. Cover inclusive UTC
boundaries, malformed/naive/reversed bounds, explicit roots, mtime-old file
with in-range event, missing roots, legacy default JSON compatibility, global
ID copies, changing nonzero streamed usage, Claude thinking details, cache TTL
reconciliation, synthetic separation, direct Codex rows, fallback-only rows,
same-last/different-total fallback preservation, mixed-schema non-fusion,
receipt decoder failures, and starts-index ambiguity/stage metadata behavior.
Write the focused tests but do not run verification: the stage orchestrator is
the only verification runner and will report commands/results. Record material
decisions or schema surprises with `loom memory`; do not write knowledge
directly.
