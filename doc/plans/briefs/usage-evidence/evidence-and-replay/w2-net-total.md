# W2 (codex unit): the per-provider net-total verdict

Tier: codex terra (`loom-codex-forwarder`, `--model gpt-5.6-terra --effort xhigh`). One file plus its test. Paths are relative to `loom/`.

## Owns

- `src/commands/usage/comparison_net.rs` (new)
- `src/commands/usage/comparison_net_tests.rs` (new)

Read: `src/commands/usage/comparison_schema.rs` (`ComparisonPair`, `RunEvidence.provider_tokens: BTreeMap<Provider, ProviderTokenVector>`, `Outcome` with `Serialize` and kebab-case names), `src/commands/usage/provider_types.rs` (`Provider`, `ProviderTokenVector` with `resident_input_tokens: Option<u64>` and `output_tokens: Option<u64>`). Another worker declares `mod comparison_net;` in `src/commands/usage/mod.rs` and calls `evaluate`; another adds the report field. Do not edit those files.

## Interface (pinned, write it exactly)

```rust
use serde::Serialize;
use super::comparison_schema::{ComparisonPair, Outcome};
use super::provider_types::Provider;

#[derive(Debug, Serialize)]
pub(super) struct NetTotalSummary {
    pub(super) verdict: Outcome,
    pub(super) reason_codes: Vec<&'static str>,
    pub(super) providers: Vec<ProviderNetTotal>,
}

#[derive(Debug, Serialize)]
pub(super) struct ProviderNetTotal {
    pub(super) provider: Provider,
    pub(super) pairs: usize,
    pub(super) baseline_total: u64,
    pub(super) candidate_total: u64,
    pub(super) mean_reduction: Option<f64>,
    pub(super) standard_error: Option<f64>,
    pub(super) classification: &'static str,
}

pub(super) fn evaluate(pairs: &[&ComparisonPair], excluded_pairs: usize) -> NetTotalSummary
```

## Rules (each one decided; implement as written)

1. `excluded_pairs > 0`: verdict `Inconclusive`, reason `net-total-excluded-pairs`, providers empty. No pairs at all: `Inconclusive`, reason `insufficient-pairs`.
2. When any pair has different provider key sets in its two runs: `Inconclusive`, reason `cross-provider-transfer`, providers empty.
3. The net of one run for one provider is `resident_input_tokens + output_tokens` (checked add). A missing dimension in any run gives that provider classification `missing-dimension`, verdict `Inconclusive`, reason `missing-token-dimension`. Providers are never summed with each other.
4. Per provider, over its n pairs: d_i = baseline net minus candidate net (compute in `i128`, then `f64`). mean = sum(d) / n. With n < 3: classification `insufficient-pairs`, `mean_reduction` and `standard_error` are `None`, verdict `Inconclusive`, reason `insufficient-pairs`. Otherwise variance = sum((d - mean)^2) / (n - 1) (the sample variance) and standard_error = sqrt(variance / n).
5. Classification for n >= 3: `reduction` when mean > 0 and mean > 2 *standard_error; `regression` when mean < 0 and -mean > 2* standard_error; otherwise `within-noise`.
6. Verdict, after the checks above passed: every provider `reduction` gives `SupportedCandidate` with no reason; some provider `reduction` and some provider `regression` gives `Inconclusive` with `cross-provider-transfer`; some `regression` and no `reduction` gives `Rejected` with `net-total-regression`; anything else gives `Inconclusive` with `net-total-within-noise`.
7. `providers` is sorted by provider (the `Ord` on `Provider`); `baseline_total` and `candidate_total` are the sums of the nets (saturating).

## Tests (`comparison_net_tests.rs`, declared from the new file with `#[cfg(test)] #[path = "comparison_net_tests.rs"] mod tests;`)

Build `ComparisonPair` values by deserializing JSON (`serde_json::from_value`) shaped like `tests/fixtures/token_optimization/comparison/valid_improvement.json`, reading that file with `include_str!("../../../tests/fixtures/token_optimization/comparison/valid_improvement.json")` and editing `provider_tokens` per pair. Cases:

- four pairs with Claude candidate nets 10, 12, 9 and 11 percent below the baseline: `reduction`, `SupportedCandidate`;
- four identical pairs (baseline equals candidate): `within-noise`, `Inconclusive`, `net-total-within-noise`;
- one pair: `insufficient-pairs`;
- four pairs where Claude falls and Codex rises by more than twice its standard error: `cross-provider-transfer`;
- four pairs where Claude rises: `Rejected`, `net-total-regression`;
- a Claude vector without `output_tokens`: `missing-token-dimension`;
- known numbers: d = [10, 20, 30, 40] gives mean 25 and standard error sqrt(166.666.../4) within 1e-9.

## Done means

Your two files compile against the interfaces pinned in common.md. Run no build, test or formatter: the Claude workers build the same crate in this worktree at the same time, so a cargo command can wait on their build lock past the 540-second forward deadline, and the orchestrator compiles right after you. If you meet a file you do not own that does not fit the pinned interfaces, report it and do not touch that file. The orchestrator runs `cargo test --lib commands::usage::comparison_net::` after you return. File under 250 lines, every function under 50 lines, after `cargo fmt`. No git, no path under `.loom/`.
