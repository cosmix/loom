# evidence-and-replay: shared pins

Stage `evidence-and-replay` of `doc/plans/token-optim/PLAN-03-usage-evidence.md` (metaplan plan 2, steps 3, 4, 6 and 7). It runs after this plan's `usage-accounting` stage merged: `src/commands/usage/args.rs`, `completeness.rs`, `codex_scope.rs` and `ProviderReport.completeness` exist. Every worker reads this file first, then its own brief. Paths are relative to `loom/` unless they start with `../` or `doc/`. Line numbers are from `6fee4ecd`; the `usage-accounting` stage moved `UsageArgs` out of `usage/mod.rs`, so re-read `mod.rs` and anchor by symbol.

## 1. What this stage changes

- `--compare` gains two declared rules: a per-provider net-total token rule with variance (`token_rule: "net-total"`) and a latency tolerance judged across pairs (`latency_tolerance_ms`). Without the declarations every existing verdict is unchanged (the fixture matrix in `tests/fixtures/token_optimization/comparison/matrix.json` and `tests/token_optimization_comparison.rs` stay green untouched).
- Quota continuity and `--compare` quota checks tolerate the 1 s `resets_at` jitter (300 s) and interval lengths within one poll interval (180 s).
- `loom usage --run-evidence <DECLARATION.json>` writes one run's `RunEvidence` for `--compare`.
- A test-only replay harness renders every stage signal of a frozen fixture and every worker query, and prints bytes per section.

## 2. Interfaces workers share (pinned; compile together at the end)

In `src/commands/usage/comparison_schema.rs` (W1):

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum TokenRule { #[default] Dominance, NetTotal }
// Intervention gains, after changed_dimensions:
#[serde(default)] pub(super) token_rule: TokenRule,
#[serde(default)] pub(super) latency_tolerance_ms: Option<u64>,
// ComparisonReport gains, after pairs:
#[serde(skip_serializing_if = "Option::is_none")] pub(super) net_total: Option<super::comparison_net::NetTotalSummary>,
#[serde(skip_serializing_if = "Option::is_none")] pub(super) latency: Option<super::comparison_latency::LatencySummary>,
```

`RunEvidence` and every type it holds (`ProviderAssignment`, `CheckEvidence`, `ReviewEvidence`, `FindingEvidence`, `FindingSeverity`, `EvidenceVerdict`, `AccountingProvenance`, `QuotaObservation`) gain `Serialize` next to `Deserialize`, with the same `rename_all`. Field names and order do not change.

In `src/commands/usage/comparison_net.rs` (W2, codex): `pub(super) struct NetTotalSummary`, `pub(super) struct ProviderNetTotal`, `pub(super) fn evaluate(pairs: &[&ComparisonPair], excluded_pairs: usize) -> NetTotalSummary` (spec in `w2-net-total.md`).

In `src/commands/usage/comparison_latency.rs` (W3, codex): `pub(super) struct LatencySummary`, `pub(super) struct MeasureSummary`, `pub(super) fn evaluate(pairs: &[&ComparisonPair], tolerance_ms: u64, reasons: &mut Vec<&'static str>) -> LatencySummary` (spec in `w3-latency.md`).

`src/commands/usage/mod.rs` (W4) declares `mod comparison_latency; mod comparison_net; mod run_evidence; mod run_evidence_stages;`.

## 3. JSON surface the contracts read (pinned)

- `--compare --json` report: `.net_total.verdict` (`"supported-candidate" | "inconclusive" | "rejected"`), `.net_total.reason_codes`, `.net_total.providers[]` with `provider`, `pairs`, `baseline_total`, `candidate_total`, `mean_reduction`, `standard_error`, `classification`; `.latency.verdict`, `.latency.tolerance_ms`, `.latency.critical_path` and `.latency.notification`, each with `state`, `pairs`, `mean_increase_ms`, `verdict`. Both keys are absent when the intervention declares neither rule.
- `loom usage --run-evidence <file> --project <dir> --since <rfc3339> --until <rfc3339> [--claude-root <dir>] [--codex-root <dir>]` prints one `RunEvidence` JSON object on stdout and exits 0.
- `loom usage --json` quota points: `.provider_ledger.quota_history.providers[].points[].continuity` (`initial`, `same-reset`, `reset`, `unknown`).

## 4. Rules for every worker

- Size limits: every `.rs` file under `src/` and `tests/` at most 400 lines, every function at most 50, measured after `cargo fmt`; keep a 10% margin. `loom/maintainability-baseline.txt` must not change (a ratchet file of this plan; it fails on growth and shrinkage of a listed entry; none of the files this stage touches is listed).
- `anyhow` errors with context; never echo artifact or declaration content into an error.
- Never run git. Never write under `.loom/`. Run only the check your brief names, once.
- Record mistakes, decisions and surprises with `loom memory note` / `loom memory decision` (a codex unit reports them in its final message). Never Claude Code auto-memory.

## 5. The gate the orchestrator runs (from `loom/`)

```bash
cargo build
cargo test --all-targets --no-fail-fast
cargo test --doc --workspace --all-features -- --test-threads=1
cargo clippy --all-targets -- -D warnings
cargo fmt --check
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps
cargo test --test signal_replay -- --ignored --nocapture
```
