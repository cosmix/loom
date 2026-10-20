# W1: declared verdict rules and quota tolerance

Tier: sonnet (`loom-software-engineer`). Read `common.md` in this directory first.

## Files you own

- `src/commands/usage/comparison_schema.rs`, `src/commands/usage/comparison_eval.rs`
- `src/commands/usage/comparison_rules_tests.rs` (new, declared from `comparison_eval.rs` with `#[cfg(test)] #[path = "comparison_rules_tests.rs"] mod rules_tests;`)
- `src/commands/usage/comparison_quota.rs`
- `src/commands/usage/comparison_token.rs` (visibility of `complete_accounting` only)
- `src/quota/mod.rs`, `src/quota/history.rs`, `src/quota/poller.rs`, `src/quota/history_store_tests.rs`, `src/quota/history_tests.rs`
- `src/tokens/quota_points.rs`

Read-only: `src/commands/usage/comparison_quality.rs`, `comparison_validation.rs`, `tests/token_optimization_comparison.rs`, `tests/fixtures/token_optimization/comparison/*.json`.

## Step 1: schema (`comparison_schema.rs`)

Add `TokenRule`, the two `Intervention` fields and the two `ComparisonReport` fields exactly as pinned in `common.md` section 2, and `Serialize` on `RunEvidence` and its nested types. `failure_report` and `aggregate_report` (`comparison_eval.rs:26-37`, `:208-226`) set both new report fields to `None` unless step 2 fills them.

## Step 2: wiring (`comparison_eval.rs`)

Today the overall verdict is the worst pair outcome (`aggregate`, `:228-245`), each pair needs quality, latency and subscription support (`overall_pair`, `:189-206`), and one pair 1 ms slower rejects (`compare_latency`, `:170-187`). Keep that path exactly when the intervention declares neither rule. Restructure `evaluate_pair` so it also returns the pair's quality outcome and whether the pair was structurally valid with complete accounting (`accounting_complete && accounting_provenance == Measured` on both runs, the test `comparison_token.rs::complete_accounting` applies; make that function `pub(super)` and reuse it).

- `latency_tolerance_ms: Some(t)`: per-pair `latency_verdict` is skipped (the pair's latency is `SupportedCandidate`, no latency reasons). After the pairs, call `comparison_latency::evaluate(&all_pairs, t, &mut report_reasons)` and store it in `latency`.
- `token_rule: NetTotal`: call `comparison_net::evaluate(&usable_pairs, excluded)` where `usable_pairs` are the structurally valid, complete-accounting pairs and `excluded` counts the rest; store it in `net_total`; append its `reason_codes` to the report reasons.
- Overall verdict. Dominance rule: worst of the pair verdicts (as today), then worse with `latency.verdict` when a tolerance is declared. Net-total rule: `Rejected` when any pair quality outcome is `Rejected`, any pair subscription outcome is `Rejected`, the latency outcome (declared summary, else the per-pair latency outcomes) is `Rejected`, or `net_total.verdict` is `Rejected`; `SupportedCandidate` when every pair quality outcome, the latency outcome and `net_total.verdict` are `SupportedCandidate`; otherwise `Inconclusive`. A subscription verdict that is only inconclusive does not hold back a net-total support: quota is reported in whole percents and cannot resolve a plausible saving (metaplan, "How a saving is shown").
- `token_proxy_verdict` and `subscription_verdict` keep their meaning (worst pair outcome). The report's `reason_codes` stay sorted and deduplicated (`sort_reasons`).
- Keep `comparison_eval.rs` under 360 lines and every function under 50.

## Step 3: quota tolerance

- `src/quota/mod.rs`: `pub const RESET_TOLERANCE_SECS: u64 = 300;` with a doc line: real history alternates `resets_at` by one second between polls. `src/tokens/quota_points.rs:12` drops its private `RESET_SLACK_SECS` and uses `crate::quota::RESET_TOLERANCE_SECS` (same value, same use at `:117`).
- `src/quota/poller.rs:29`: `POLL_INTERVAL` becomes `pub(crate)`.
- `src/quota/history.rs::continuity_from_windows` (`:202-230`): `(Some(left), Some(right)) if left.abs_diff(right) > RESET_TOLERANCE_SECS => reset = true`. Everything else in that match is unchanged.
- `src/quota/history_store_tests.rs:69-71` moves `resets_at` by 100 s and expects `Reset`: move it by 400 s instead (`500, 900, 1_300` style values), keeping every assertion line; add a test in `history_tests.rs` that a 1 s alternation reads `SameReset`.
- `comparison_quota.rs`: `reset_incomparable` (`:121-128`) compares `baseline.resets_at.abs_diff(candidate.resets_at) > RESET_TOLERANCE_SECS` instead of `!=`; the `reset_id`, continuity and end-before-reset checks stay. `compare_interval` (`:83-113`) reads `quota-interval-mismatch` when either duration is `None` or the two durations differ by more than `POLL_INTERVAL.as_secs()`.

## Tests (`comparison_rules_tests.rs`)

Build artifacts in code from `tests/fixtures/token_optimization/comparison/valid_improvement.json` (read with `include_str!("../../../tests/fixtures/token_optimization/comparison/valid_improvement.json")`). Cases: no declaration gives byte-identical JSON to today for `valid_improvement.json` (compare `serde_json::to_value` of the report against a copy built with both declarations absent); a net-total artifact with four reducing pairs is `supported-candidate` overall even though its pairs have no quota observations; a net-total artifact with one pair whose review failed is `rejected`; a declared tolerance of 5 ms with a +1 ms candidate is not rejected while the undeclared form still is; two quota observations whose `resets_at` differ by 1 s compare, by 301 s read `quota-reset-crossing`.

Run once at the end: `cargo test --lib commands::usage::comparison` and `cargo test --lib quota::`, and `cargo test --test token_optimization_comparison` (the existing matrix must stay green with no edit). Report the results and each changed file with its line count after `cargo fmt`.
