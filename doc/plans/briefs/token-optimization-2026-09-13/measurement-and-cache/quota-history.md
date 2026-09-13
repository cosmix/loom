# Worker brief — quota history (Terra)

Target plan: `PLAN-token-optimization-2026-09-13.md`.
Final brief path: `doc/plans/briefs/token-optimization-2026-09-13/measurement-and-cache/quota-history.md`.

You own bounded, local provider-quota history. Work as a
`loom-codex-forwarder` using `gpt-5.6-terra` at `xhigh` effort. Do not spawn
subagents, run git commands, read `CLAUDE.md`, alter status/web contracts, or
edit provider-ledger files. Read this brief in full before acting. Start at
`write_provider` in `loom/src/quota/cache.rs`, `finish_poll` in
`loom/src/quota/poller.rs`, and `read_snapshot` in `loom/src/quota/mod.rs`.

## Territory and non-goals

You exclusively own every source and test file in `loom/src/quota/`, including
the new `loom/src/quota/history.rs` and its test module. Do not edit
`commands/status/**`, `web/**`, `commands/usage/**`, daemon wiring, or the
current live-snapshot model exposed to the TUI/web. The existing snapshot files
remain `<work_root>/quota/<provider>.json`; `read_snapshot` and its status
consumers must retain their current behavior and shape.

The provider-ledger worker will later call the public reader below after your
change is merged. It owns that join and every user-visible usage report. Your
history contains provider quota state only; do not infer API request counts,
tokens, price, quality, or latency from it.

## Existing behavior to preserve

`ProviderQuota` is the current poll snapshot: `observed_at`, up to two
`QuotaWindow` values, optional plan, optional error. `cache::write_provider`
serializes and atomically replaces the provider snapshot after symlink checks;
`record_failure` preserves the last good windows while writing an error.
`poller::finish_poll` is the single successful-poll path that calls
`write_provider`. Claude and Codex poll on independent schedules. Status reads
only `quota::read_snapshot`; it must not begin reading history.

The current cache sanitizes percentages, retains at most FiveHour and SevenDay
windows, applies inline-safe text hygiene, and uses restrictive quota-directory
permissions. Reuse those semantics for history rather than accepting a second
looser persisted representation. A malformed, oversized, missing, or symlinked
history input must be non-fatal and must never take down the poller or status.

## New data model and public reader

Add `mod history;` privately or publicly as needed in `quota/mod.rs`, and
define these **new** types in `quota/history.rs`:

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuotaHistoryPoint {
    pub observed_at: i64,
    pub windows: Vec<QuotaWindow>,
    pub plan: Option<String>,
    pub continuity: HistoryContinuity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryContinuity { Initial, SameReset, Reset, Unknown }

pub struct QuotaHistoryRead {
    pub points: Vec<QuotaHistoryPoint>,
    pub diagnostics: QuotaHistoryDiagnostics,
}
pub struct QuotaHistoryDiagnostics {
    pub source: HistorySourceState,
    pub malformed_rows: usize,
    pub unsupported_schema_rows: usize,
    pub nonmonotonic_rows: usize,
}
pub enum HistorySourceState { Missing, Empty, Read, Corrupt, Unreadable, UnknownProvider }

pub fn read_history(
    work_root: &Path,
    provider: &str,
    since: i64,
    until: Option<i64>,
) -> QuotaHistoryRead;
```

`read_history` is the only cross-worker contract. It is read-only, returns
points sorted by strictly increasing `observed_at`, and includes `since <= t <=
until` when an upper bound exists. Its diagnostics distinguish an absent file,
an empty valid file, a partially readable file with rejected rows, a corrupt
file with no accepted rows, and an unsafe/unreadable file. Unknown provider
input is a diagnostic state, not an ordinary empty reading. It must not contact
a provider, spawn Codex, mutate cache/history, or treat no points as zero quota
use.

Persist successful observations only at
`<work_root>/quota/history/<provider>.jsonl`, where provider is exactly
`claude` or `codex`. Do not accept a caller-derived filename. Each line is a
versioned point record; include a `schema_version: 1` envelope so future
decoders can reject unsupported shapes rather than guess. Persist sanitized
FiveHour/SevenDay windows, sanitized plan, and no failure/error body. History
must never persist credentials, response bodies, commands, provider errors, or
untrusted paths.

## Point normalization and ambiguity policy

The history is a reset-aware point vector, not a fabricated consumption curve.

1. A point is eligible only after `cache::write_provider` has succeeded for the
   same successful poll. A failed poll and `record_failure` add no history row.
2. Use the poll's `observed_at`; reject nonpositive timestamps and do not add a
   second point with the same or older timestamp. An out-of-order source row is
   dropped by the reader/writer with an internal diagnostic, never reordered by
   inventing a time.
3. Compare each window kind with the preceding accepted point. A changed
   `resets_at` starts a `Reset` segment. A nondecreasing percentage under the
   same reset is `SameReset`. A lower percentage with the same reset, a missing
   reset boundary, or otherwise incomparable window state is `Unknown`.
   Preserve the raw sanitized observation with `Unknown`; do not clamp it up,
   interpolate it, call it a reset, or derive a token delta.
4. The first retained point is `Initial`. Equal consecutive sanitized vectors
   may be omitted to save space only when plan and continuity-relevant reset
   state are also equal. Never coalesce a changed plan, a reset, or an unknown
   point.
5. Reader normalization must make no monotonicity claim across an `Unknown` or
   `Reset` boundary. Consumers can safely segment on `continuity`; they must
   not assume a continuous series merely because timestamps are ordered.

Use a single policy for the two provider files. Do not make Codex and Claude
special cases beyond their existing parser-to-`ProviderQuota` differences.

## Retention, safety, and lifecycle

The combined history budget is exactly 30 UTC days and 16 MiB across the two
provider history files. On every successful append, lock or otherwise serialize
the read-prune-rewrite operation, retain only rows whose `observed_at` is within
the rolling 30-day cutoff, then evict oldest valid rows across both providers
until their total on-disk history bytes are at most 16 MiB. If a freshly
serialized record cannot fit after eviction, reject that history record
non-fatally and keep the already-bounded history unchanged. Never retain a
record that makes either the combined history or one history file exceed the
16 MiB total cap.

Use the existing quota directory permission and symlink-defense posture. A
rewrite must be crash-atomic like the current provider snapshot: readers see an
old valid file or a complete new file, never a torn partial. Treat JSONL lines
independently on read so one torn/malformed line does not hide earlier valid
history. Reject an oversized file before unbounded allocation. Do not delete or
rewrite the current `<provider>.json` snapshot as part of pruning.

Keep the existing cache update ordering: live snapshot first, then best-effort
history append. A history failure must not change `ProviderState` success/backoff
semantics, make a successful provider poll appear failed, or overwrite the
current snapshot's error field. It may be logged once through the existing
poller error style, without exposing untrusted error text in history.

## Integration steps

Name the new best-effort writer `history::record_successful_observation` and
call it from `poller::finish_poll` only after the live snapshot succeeds.

1. Add the history module/types and pure decoder/normalizer tests. Keep shared
   `QuotaWindow`/`WindowKind` in `model.rs`; do not widen `QuotaSnapshot` or
   `ProviderQuota` for history.
2. Add cache-facing helpers with safe fixed provider names. Preserve
   `write_provider`, `read_provider`, and `record_failure` public behavior.
   Share atomic-write/permission logic only after checking that the extracted
   helper preserves every existing symlink and fsync safeguard.
3. From `poller::finish_poll`, write the current snapshot exactly as today;
   only after that succeeds append the sanitized point. Swallow history errors
   after bounded logging and call `record_success` exactly as the current path
   does.
4. Re-export the precise `read_history` and `QuotaHistoryPoint` contract from
   `quota/mod.rs` for the later provider-ledger stage. Do not make usage depend
   on cache internals.

## Tests and completion bar

Extend `cache_tests.rs`, `poller_tests.rs`, and `model_tests.rs`; add
`history_tests.rs`. Cover both providers, success append, no append on failed
poll/cache-write failure, empty/missing/malformed/torn/oversized/symlinked
history, unsupported schema version, UTC inclusive reader bounds, sorted output,
duplicate timestamp rejection, repeated vector coalescing, plan change
preservation, FiveHour/SevenDay reset detection, same-reset decrease marked
`Unknown`, missing-reset ambiguity, 30-day pruning, 16 MiB cross-provider
eviction, newest-record oversize edge case, and proof that live snapshot read
and its serialized shape remain unchanged. The test fixture must assert the
later provider ledger can call `read_history` without network or process work.

Write the focused tests but do not run verification: the stage orchestrator is
the only verification runner and will report commands/results. Record material
persistence or normalization decisions with `loom memory`. Do not write
knowledge directly.
