# Brief: denial-classification (sandbox denials are environment failures)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/verify/environment.rs` (new), `loom/src/verify/environment_tests.rs` (new), `loom/src/verify/mod.rs`, `loom/src/commands/plan/verify/dry_run_classify.rs`, `loom/src/verify/criteria/criterion_eval.rs`, `loom/src/verify/impact_tests/runs.rs`, `loom/src/verify/impact_tests/runs_tests.rs`.

Frozen, read-only: `loom/src/verify/environment_contract_tests.rs`.

## Entry points

- `ENVIRONMENT_MARKERS` and `classify_with` in `commands/plan/verify/dry_run_classify.rs:33-90` (dry run only today).
- Acceptance criteria evaluation: `verify/criteria/criterion_eval.rs` (confined run at :169; find where a failed criterion's message is built with `loom map --outline`).
- Impact selections: `verify/impact_tests/runs.rs:85-135` (judging a run: exit 127 note, capture, `classify(summary)`), `Run::Inconclusive` (used at :58-62 for a missing environment).

## Change

1. `verify/environment.rs`: `pub const ENVIRONMENT_MARKERS` (moved from `dry_run_classify.rs`, which now imports it) and `pub fn sandbox_denial(output: &str) -> Option<String>`: the first line (trimmed, at most 300 bytes, cut on a char boundary) that contains, case-insensitively, `operation not permitted`, `eperm`, `sandbox_apply`, `deny(1)`, `couldntreadcurrentdirectory` or `bwrap:`. Ordinary assertion output returns None.
2. Criteria: a failing criterion whose stdout or stderr has `sandbox_denial(..)` Some gets its failure message prefixed `environment failure (sandbox denial: <line>)` and a hint naming `loom stage dispute-criteria <stage> --criterion-index <n>`. It still fails.
3. Impact runs: a failing selection whose output has a denial returns `Run::Inconclusive(format!("environment failure: {line}; the full suite runs in integration-verify"))`, which the report already turns into a note. Apply it only to runs that failed; a passing run is untouched. A build failure (`BuildFailed`) is not reclassified.
4. `verify/mod.rs`: `pub mod environment;` plus the test module lines.

## Tests

`environment_tests.rs`: line selection, the 300-byte cut on a multi-byte character, case folding. `runs_tests.rs`: a failing selection with `EPERM: operation not permitted, kill` in its output is a note; one with `AssertionError` stays a failure. The dry-run classifier test (`commands::plan::verify::dry_run::dry_run_tests::classify_separates_environment_from_red`) keeps passing.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib verify::environment`, `--lib verify::impact_tests::runs`, `--lib commands::plan::verify::` and `--lib verify::criteria::` pass with the frozen contract.

## Traps

- `impact-acceptance` owns `report.rs`, `failure.rs` and `accepted.rs`; `base-provision` owns `base.rs`. Do not edit them.
- Capture the marker strings from the issue text, not memory: bun prints `error: CouldntReadCurrentDirectory`; Node prints `Error: kill EPERM` and `EPERM: operation not permitted`.
