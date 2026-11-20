# Live token insights: shared context for every worker

Read this file in full, then your own brief. Where this file and a brief disagree, this file wins. Knowledge to read first: `doc/loom/knowledge/architecture/token-flow-sources.md` (whole file, 70 lines).

## 1. What the plan delivers

1. Plan-wide token totals per (model, effort): a table and a pie chart by model (efforts summed) in the web Token flow panel, and a "by model" block with share bars in the TUI `t` pane.
2. The web mix row "uncached input" explains itself (it is correct: Claude Code puts a cache breakpoint at the end of every prompt, so `input_tokens` is about 2 per API call).
3. Token statistics refresh every 5 s instead of every 30 s, with each tracker pass cheaper than today's.
4. The web stage dialog's four token chips each carry a sparkline of their own statistic.
5. The web header TOKENS rail drops its 15-minute sparkline (the Token flow panel's chart shows the same data larger).
6. The web terminal and terminal-history views in the stage dialog get a back-arrow button that does what Esc does in view mode: return to the stage details.

## 2. Wire model changes (Rust `loom/src/tokens/model.rs`, zod `web/src/api/schema.ts`)

New struct, re-exported through `pub use model::*` in `loom/src/tokens/mod.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub effort: Option<String>,
    pub total: TokenUsage,
}
```

New fields (no `#[serde(default)]`; a `flow.json` without them fails to parse and `read_flow` returns `None` until the tracker's next pass rewrites it, which takes at most 5 s):

- `StageFlow.hit_series: Vec<Option<u8>>` — placed after `series`. Exactly `STAGE_SERIES_LEN` (120) values, aligned with `series` (oldest first, ending at the current bucket). Value per bucket: `None` when `cache_read + input + cache_write` is 0, else `round_half_up(cache_read * 100 / (cache_read + input + cache_write))` capped at 100, computed in u64 with `saturating_mul`.
- `TokenFlow.models: Vec<ModelUsage>` — placed after `stages`. Plan-wide totals per (model, effort) over every agent of every stage, sorted by `total.fresh()` descending, then `model` ascending, then `effort` ascending with `None` last. At most `MAX_MODEL_ROWS` entries: when there are more, the entries from position `MAX_MODEL_ROWS - 1` onward are merged into one `("other", None)` entry placed last, so the entries always sum to `plan.total` field by field.

New constants in `model.rs`:

```rust
/// Distinct (model, effort) keys one agent keeps; usage for a further key goes to `OTHER_MODEL`.
pub const MAX_MODEL_KEYS: usize = 8;
/// Entries of `TokenFlow.models`; the rest are merged into one `OTHER_MODEL` entry.
pub const MAX_MODEL_ROWS: usize = 24;
/// The model of usage read before any line named one.
pub const UNKNOWN_MODEL: &str = "unknown";
/// The key that absorbs usage past `MAX_MODEL_KEYS` or `MAX_MODEL_ROWS`.
pub const OTHER_MODEL: &str = "other";
```

zod (all `.strict()`, import from `@/api/zod` as the file already does):

- `modelUsageSchema = z.object({ model: z.string(), effort: z.string().nullable(), total: tokenUsageSchema }).strict()`
- `stageFlowSchema` gains `hit_series: z.array(z.number().int().min(0).max(100).nullable())`
- `tokenFlowSchema` gains `models: z.array(modelUsageSchema)`
- export `type ModelUsage = z.infer<typeof modelUsageSchema>` beside the other token types.

JSON field order follows the Rust struct field order (the fixture test compares `serde_json::Value`s, so order does not matter for it, but keep the fixture readable in that order).

## 3. Accounting rules (token-core W1)

- **Effort source.** Claude: the assistant line's top-level `perTurnEffort` string, else its top-level `effort` string, else none. Codex: `turn_context.payload.effort`. Both pass through `inline_safe` (`crate::context::untrusted::inline_safe`) as `model` does today.
- **Attribution per line.** Every usage delta is booked to the (model, effort) current at that line. Claude: the line's own model when it is present and not `SYNTHETIC_MODEL`, else the agent's last model, else `UNKNOWN_MODEL`; the line's own effort when present, else the agent's last effort, else `None`. Codex: the model and effort of the last `turn_context` read, else `UNKNOWN_MODEL` and `None`.
- **Per-agent keys.** `AgentState.by_model: Vec<ModelUsage>` holds at most `MAX_MODEL_KEYS` keys plus one `(OTHER_MODEL, None)` key that absorbs every further key. Invariant at all times: the `by_model` totals sum field by field to `AgentState.total`. `restart` clears it with everything else.
- **Checkpoint.** `CHECKPOINT_VERSION` goes from 1 to 2. An existing version-1 checkpoint is then refused by the existing version check (`checkpoint is version 1; every transcript is read from the start`) and every transcript is read once from offset 0. This project adds no migrations.
- **Rate.** `rate` in `flow_build.rs` becomes `sum_field * 60 / span_secs` per field, with `span_secs = (RATE_BUCKETS - 1) * BUCKET_SECS + (now - now_bucket * BUCKET_SECS) + 1`, a value from 271 to 300. At the last second of a bucket the span is 300 s and the rate equals today's `sum / 5`. The old formula drops about 10% each time a 30 s bucket starts, which a 5 s refresh would show as a sawtooth.

## 4. Tracker cadence and the pass memo (token-core W2)

- `TRACKER_INTERVAL_SECS` = 5.
- A thread-local pass memo, keyed by the canonical `work_root`, holds: the in-memory `Checkpoint` and the `(inode, len, mtime_nanos)` stamp of `checkpoint.json` after the memo's last load or save, plus the hash of the JSON last saved; the hash of the last written flow JSON with `observed_at` zeroed, the `observed_at` written and the stamp of `flow.json` after that write; the last `quota_left` value with the `now` it was computed at and the stamps of the quota files it read.
- **Checkpoint load:** reuse the memo's checkpoint when the file's current stamp equals the memo's; otherwise load from disk as today.
- **Checkpoint save:** skip when the compact JSON hash equals the last saved hash. An over-cap or failed save clears the checkpoint memo, so the next pass loads from disk exactly as today.
- **Flow write:** skip when the hash equals the last written hash, `0 <= now - last_observed_at < FLOW_HEARTBEAT_SECS` (60) and `flow.json`'s stamp equals the memo's. A failed write clears the flow memo. While tokens flow, the rate changes every second, so the flow is written every pass; while idle it is written once a minute.
- **Quota left:** reuse the memo's value when `0 <= now - computed_at < QUOTA_LEFT_REFRESH_SECS` (30) and the stamps of every quota file it read are unchanged; otherwise recompute. Quota history files hold up to 30 days (`MAX_HISTORY_BYTES` 16 MiB); today they are read and parsed every pass.
- `scan_pass(work_root: &Path, codex_sessions_root: Option<&Path>, now: i64) -> anyhow::Result<TokenFlow>` keeps its signature; the returned flow is always the flow the pass built, whether or not it was written.

## 5. Fixture values (web `web/src/api/fixtures/snapshot.json` and Rust `fixture_snapshot()` in `loom/src/commands/status/web/model_tests.rs`)

The fixture's single stage flow `server` has three agents: main `claude-opus-5-5`, subagent `claude-sonnet-5-5`, codex `gpt-6.1-sol`. Plan total: input 120000, cache_write 1800000, cache_read 26000000, output 310000 (fresh 2230000, shown `2.23M`).

`status.tokens.models`, in this order:

| model | effort | input | cache_write | cache_read | output | fresh | shown |
| --- | --- | --- | --- | --- | --- | --- | --- |
| claude-sonnet-5-5 | medium | 50000 | 900000 | 13000000 | 160000 | 1110000 | 1.11M |
| claude-opus-5-5 | xhigh | 30000 | 450000 | 7000000 | 80000 | 560000 | 560k |
| gpt-6.1-sol | xhigh | 30000 | 300000 | 4000000 | 50000 | 380000 | 380k |
| claude-opus-5-5 | high | 10000 | 150000 | 2000000 | 20000 | 180000 | 180k |

Sums equal the plan total. Shares by model (efforts summed): sonnet 49.78% (`50%`), opus 33.18% (`33%`), sol 17.04% (`17%`). Cache reads shown: sonnet `13.00M`, opus xhigh `7.00M`, sol `4.00M`, opus high `2.00M`. `status.tokens.stages[0].hit_series` is 120 values of 94.

## 6. Web rendering decisions (web-insights)

- **Token flow panel** (`token-flow-panel.tsx`): the `Models` rate list is replaced by `<TokenModels flow={flow} />` from a new `web/src/components/token-models.tsx`. It renders a sub-heading row ("By model", right side "fresh so far"), a donut pie (`<svg role="img" aria-label="fresh tokens by model">`, one `<circle>` slice per model carrying `data-model="<model>"`, efforts summed), a legend `<ul aria-label="models">` whose rows show a colour swatch, the model, its share as a whole percent and its current fresh rate `"<compact>/m"` from `modelRates` (the old list's information), and a table `<table aria-label="token totals by model and effort">` with header cells Model, Effort, Fresh, Cache read, one row per `flow.models` entry grouped by model in pie order, the model cell spanning its effort rows (`rowSpan`), an effort of `null` shown as `—`. Numbers use `compactTokens`. The block renders nothing when `flow.models` is empty or sums to 0 fresh.
- **Model colours:** `MODEL_PALETTE = ["var(--tone-completed)", "var(--tone-contract)", "var(--tone-warning)", "var(--tone-merged)", "var(--tone-queued)", "var(--tone-executing)"]`, cycled in pie order. It reverses the stage chart's order so the first stage and the first model differ.
- **Uncached input hint:** the `MIX` entry for `input` carries `hint`: `Input tokens neither read from nor written to the prompt cache. Claude Code caches almost every prompt, so this stays at a few tokens per API call.` The `<dt>` gets `title={hint}` and a lucide `InfoIcon` (`aria-hidden`, `size-3`) after the label.
- **Header rail** (`header-lines.tsx` `TokenLine`): the "last 15 min" cell and its `Sparkline` go; three cells remain (fresh / min, plan total, cache hit). The `<ol>` becomes `grid-cols-3` on phones and `sm:grid-cols-[repeat(3,auto)]`; `TokenCell` drops its two `max-sm:` wrap rules (there is no second row any more).
- **Stage dialog chips** (`stage-tokens.tsx`): each `Chip` takes a `trend: { values: number[]; label: string }` and renders `<Sparkline values width={96} height={20} label className="text-(--tone-executing)" />` under its value. Values are the last `TREND_POINTS` = 60 buckets (30 min), left-padded with `lastValues`:
  - fresh rate: `rollingRate(stage.series, flow.bucket_secs)`; label `fresh rate, last 30 min`
  - fresh so far: `cumulativeSeries(fresh(stage.total), stage.series)`; label `fresh so far, last 30 min`
  - cache hit: `carryForward(stage.hit_series)`; label `cache hit per 30 s, last 30 min`
  - agents live: `liveCounts(stage.agents, stage.series.length)`; label `agents live, last 30 min`
- **Back button:** `TerminalView` and `HistoryView` take `onBack?: () => void`. When given, the header's left column starts with a row holding `<button type="button" aria-label="Back to details" title="Back to details (Esc)">` (lucide `ArrowLeftIcon`, `size-4`) and the existing eyebrow. `StageModal` passes `onBack={toDetails}` to both; the page frame (`routes/terminal.tsx`) passes nothing and keeps its own link. The button works in view and in control mode: leaving unmounts `TerminalView`, which closes its socket as the close button does today.
- **Performance:** no memo machinery on the web side. The web broadcaster sends a frame only when the body changed (`Broadcaster::publish` compares the body without `generated_at`), the tracker writes an unchanged flow at most once a minute, and every derived series is linear in at most 120 values.

## 7. Traps

- `web/src/api/schema.ts` token schemas are `.strict()`: a Rust field the schema lacks fails the parse, and `parseSnapshot` then drops `status.tokens` from the frame, so every token surface disappears.
- `loom/src/commands/status/web/model_tests.rs` `fixture_matches_serde_output` compares the Rust serializer with `web/src/api/fixtures/snapshot.json` exactly.
- `web/dist` is committed and embedded by `loom/build/assets.rs`; cargo never runs bun. The web stage rebuilds it with `bun run build`.
- Files cap at 400 lines and functions at 50 (`cargo test --test maintainability` for Rust). `tokens_pane_tests.rs` is at 374 lines: new pane tests go in a new file.
- Never `Math.max(...values)` on a series: use `maxOf` from `web/src/lib/tokens.ts`.
- `vitest -t` matches the full name, which joins `describe` and `it` titles with ` > `: contract tests are top-level `it()` calls.
