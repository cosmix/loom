# Brief: impact-acceptance (accepted disputes match the full selection)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/verify/impact_tests/failure.rs`, `accepted.rs`, `observed.rs`, `observed_tests.rs`, `report.rs`, `report_tests.rs`, `loom/src/orchestrator/adjudication/apply_impact.rs`, `loom/src/orchestrator/adjudication/apply_impact_tests.rs`.

Frozen, read-only: `loom/src/verify/impact_tests/accepted_contract_tests.rs`.

## Root cause (verified)

`failure.rs:142` `MAX_RECORDED_TESTS = 64`; `ImpactFailure::bounded()` (114-133) keeps the first 64 names. The observed record, the dispute evidence and `AcceptedFailure::of` (`accepted.rs:58-65`) all carry the bounded list. `AcceptedImpact::standing` (`accepted.rs:81-105`) compares `BTreeSet(record.tests) != BTreeSet(tests)` where `tests` is the full unbounded list from `report.rs:40` (`failed.tests()`), so a selection over 64 tests never matches and the same failure is judged again after every accepted dispute (issue #40: 721 targets, three accepted disputes, no effect).

## Change

1. `ImpactFailure.selection_digest: String`: lowercase hex sha256 of the sorted, deduplicated full test list joined by `\n`. Set it wherever an `ImpactFailure` is built from a full list (find every constructor with `loom map --find-all ImpactFailure`); `bounded()` keeps the digest and truncates only `tests`. Use the crate's existing sha256 dependency.
2. `AcceptedFailure.selection_digest`, copied in `of`. `standing(id, kind, tests)` computes the digest of `tests` and matches id, kind and digest; a different digest is `Changed` with a message saying the selection changed (N tests then, M now). The 64 names stay for display.
3. `observed.rs`: the observed record carries the digest through `record_observed`/`load_observed`.
4. `apply_impact.rs`: copies the digest from the evidence; its module comment (lines 3-4) says the gate notes an accepted failure "while it gets no worse", which is wrong: rewrite it to "while it fails exactly as accepted over the same selection".
5. No compatibility path for records without a digest: the project is unreleased; a record without one does not match.

## Tests

- `report_tests.rs`: `an_accepted_selection_that_grew_a_test_is_judged_again` keeps asserting a re-judge (the digest differs). Add: an accepted failure over 100 tests is a note on an identical rerun. Tests that build accepted records by hand gain the digest (expected TI-edit, named in the stage INTEGRITY line).
- `observed_tests.rs:51` `a_recorded_failure_is_bounded` still asserts 64 names and now also the digest of all.
- `apply_impact_tests.rs:41` asserts the digest is recorded.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib verify::impact_tests::` and `--lib apply_impact` pass with the frozen contract.

## Traps

- `base-provision` owns `base.rs`, where `stands()` builds an `ImpactFailure`. Pinned for both workers: `impl ImpactFailure { pub fn with_digest_of(mut self, full: &[String]) -> Self }` sets `selection_digest` from the full list; every construction site from a full list ends with `.with_digest_of(&tests)`. `base-provision` calls it in `stands`.
