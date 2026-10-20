# W3 (codex unit): latency judged across pairs with a declared tolerance

Tier: codex terra (`loom-codex-forwarder`, `--model gpt-5.6-terra --effort xhigh`). One file plus its test. Paths are relative to `loom/`.

## Owns

- `src/commands/usage/comparison_latency.rs` (new)
- `src/commands/usage/comparison_latency_tests.rs` (new)

Read: `src/commands/usage/comparison_schema.rs` (`ComparisonPair`, `RunEvidence.critical_path_ms: Option<u64>`, `RunEvidence.notification_latency_ms: Option<u64>`, `Outcome`), `src/commands/usage/comparison_eval.rs` lines 154-187 (`latency_verdict`, `compare_latency`: today one pair 1 ms slower rejects). Another worker declares `mod comparison_latency;` and calls `evaluate` only when the artifact declares `latency_tolerance_ms`. Do not edit those files.

## Interface (pinned, write it exactly)

```rust
use serde::Serialize;
use super::comparison_schema::{ComparisonPair, Outcome};

#[derive(Debug, Serialize)]
pub(super) struct LatencySummary {
    pub(super) verdict: Outcome,
    pub(super) tolerance_ms: u64,
    pub(super) critical_path: MeasureSummary,
    pub(super) notification: MeasureSummary,
}

#[derive(Debug, Serialize)]
pub(super) struct MeasureSummary {
    pub(super) state: &'static str,
    pub(super) pairs: usize,
    pub(super) mean_increase_ms: Option<f64>,
    pub(super) verdict: Outcome,
}

pub(super) fn evaluate(
    pairs: &[&ComparisonPair],
    tolerance_ms: u64,
    reasons: &mut Vec<&'static str>,
) -> LatencySummary
```

## Rules (each one decided; implement as written)

1. Judge each measure (critical path, notification) separately over all pairs.
2. Both values present in every pair: `state` = `judged`, `pairs` = n, `mean_increase_ms` = mean of (candidate minus baseline) computed in `i128` then `f64`. Verdict `Rejected` when the mean is greater than `tolerance_ms`, pushing `critical-path-regression` or `notification-latency-regression`; otherwise `SupportedCandidate`. A single slow pair does not reject by itself: the mean decides.
3. Both values absent in every run of every pair: `state` = `unmeasured`, `pairs` = 0, `mean_increase_ms` = `None`. For the notification measure the verdict is `SupportedCandidate` and the reason `notification-latency-unmeasured` is pushed (no producer of notification latency exists yet; the report says so instead of blocking). For the critical path the verdict is `Inconclusive` with `missing-latency-evidence`.
4. Present in some runs only: `state` = `partial`, verdict `Inconclusive`, reason `missing-latency-evidence`.
5. No pairs: both measures `unmeasured`, overall `Inconclusive` with `missing-latency-evidence`.
6. `verdict` of the summary is the worse of the two measure verdicts, using `Outcome::worse` (`comparison_schema.rs` lines 154-161).

## Tests (`comparison_latency_tests.rs`, declared from the new file with `#[cfg(test)] #[path = "comparison_latency_tests.rs"] mod tests;`)

Build pairs from `include_str!("../../../tests/fixtures/token_optimization/comparison/valid_improvement.json")` (its candidate is 100 ms faster on the critical path and 10 ms faster on notification). Cases:

- tolerance 0, two pairs with critical-path increases +1 and -3: mean -1, not rejected;
- tolerance 0, two pairs with +5 and +3: rejected with `critical-path-regression`;
- tolerance 10, increases +12 and +6: mean 9, supported;
- notification null in every run: `unmeasured`, the summary still `SupportedCandidate` when the critical path is supported, and `notification-latency-unmeasured` pushed;
- notification null in one run only: `partial`, `Inconclusive`, `missing-latency-evidence`.

## Done means

Your two files compile against the interfaces pinned in common.md. Run no build, test or formatter: the Claude workers build the same crate in this worktree at the same time, so a cargo command can wait on their build lock past the 540-second forward deadline, and the orchestrator compiles right after you. If you meet a file you do not own that does not fit the pinned interfaces, report it and do not touch that file. The orchestrator runs `cargo test --lib commands::usage::comparison_latency::` after you return. File under 220 lines, every function under 50 lines, after `cargo fmt`. No git, no path under `.loom/`.
