# Cache correctness before reuse

Worker: `loom-codex-forwarder`, `--model gpt-5.6-sol --effort xhigh`. The Opus/high stage orchestrator verifies and commits. Do not run git, spawn workers, run builds/tests/linters/formatters, or alter acceptance to make a failure disappear. Return changed files, assumptions, and unresolved evidence. All new names below are proposed interfaces, not existing symbols.

## Exclusive write ownership

- Existing: `loom/src/verify/criteria/cache.rs`, `cache_ignore.rs`, `runner.rs`, `result.rs`, `confine.rs`, `executor.rs`, `config.rs`, and `mod.rs` in that same directory.
- Existing tests: `loom/src/verify/criteria/tests/cache_tests.rs`, `runner_tests.rs`, and `mod.rs` in that tests directory.
- New: `loom/src/verify/criteria/cache_contract.rs`, `cache_fingerprint.rs`, and `loom/src/verify/criteria/tests/cache_contract_tests.rs`.

Read `loom/src/process/environment.rs`, `loom/src/models/stage/types.rs` (`TruthCheck`, `AcceptanceCriterion`, `CommandConfinement`), the report's cache section, and `doc/token-optimization-2026-09-12-data/cache_probe.rs`. Do not change the model/schema definitions, process environment policy, signals, quota, or usage files. Preserve existing public executor entry points while refactoring their private preparation path. Split owned modules as listed to satisfy 400-line/50-line limits.

## Grounded failure and required result

`run_acceptance_with_config` calls `run_with_cache` before `check_extended_criterion`. The latter evaluates positive/negative stdout and empty stderr after raw exit success has already been cached. `CriterionResult::cached` fabricates empty output and exit 0. The diagnostic probe against `61847ac9` printed `first_passed=false second_passed=true second_cached=true` for `printf forbidden` with `stdout_not_contains: [forbidden]`.

`compute_cache_key` hashes command/directory/HEAD/status/changed content, excluding some external/ignored references. It does not include effective environment, confinement, timeout, or the extended criterion. Files over 8MiB use size and second-resolution mtime; the key is captured before the command, with no post-run stability comparison. `CriterionResult::cached` also charges original execution duration as current elapsed duration.

Fix these mechanisms. No broadened cache eligibility, reduced test coverage, bypass of an actual failure, or claim that arbitrary shell commands are hermetic is authorized.

## Chosen contract

1. Introduce private `CriterionContract` capturing the expanded `CommandSpec`, simple/extended kind, expected exit, stdout positive/negative requirements, stderr rule, timeout, and resolved confinement. Serialize deterministically and hash it. Include setup commands in the command contract exactly as the current runner expands them. Distinguish shell from program-plus-argv; do not use the human `Display` string as a lossless argv encoding.
2. Replace raw-exit pass records with versioned `CachedCriterionPass` records that certify the **fully evaluated** criterion. Record contract digest, input/context digest, original execution duration, actual exit, complete assertion verdict, bounded diagnostic tails, and recorded time. Existing unversioned records are misses, not migrated into trusted passes. Missing fields, mismatched digests, invalid verdicts, truncation, and malformed/torn records are misses. Never delete unrelated cache state.
3. Refactor the runner to perform lookup for the full contract, otherwise execute, evaluate every acceptance assertion, and store only a fully evaluated pass. A hit returns the previously certified verdict for that exact contract; it must not re-evaluate positive/negative requirements against empty strings or truncated tails. Expected nonzero exit is preserved when it is the specified passing condition. Changing a simple criterion to an extended one, changing any pattern, or changing timeout/confinement cannot reuse the old pass.
4. Construct the confined command once in the existing `confine.rs` path and derive its execution fingerprint from the same resolved environment and executable identity that will be used to spawn it. Preserve `apply_stage_environment`'s credential exclusions. No second independently sampled ambient environment may serve as the key. Persist only an aggregate digest, never raw environment values or credentials. `CommandConfinement::Inherit` is ineligible unless its complete effective input environment can be safely fingerprinted; choose bypass in this plan. Missing executable/toolchain identity or uncertain external/ignored inputs also bypass. Retain all existing refusal rules as a floor; do not add automatic eligibility for commands whose external inputs are unknown.
5. Replace the size/mtime approximation with streamed content hashing within a bounded work budget; when complete hashing is not possible, return ineligible rather than a partial key. Handle symlinks and unreadable/deleted paths conservatively. Capture fingerprints before and after execution; any changed relevant input prevents storing the pass. A command that changes its own inputs is still evaluated normally but cannot publish a reusable pre-run receipt. A cache lookup failure never prevents a real verification run.
6. Keep atomic pass writes and existing locking discipline. Bound reads before parsing; a corrupt record cannot become a pass merely because JSON deserializes. Cache records are optimization evidence, not a source of authority to skip an unknown check.
7. Report actual lookup/execution elapsed duration in current `CriterionResult.duration`; retain original execution cost in the private cached record and diagnostic metadata. Do not add a required public field solely for this change. Audit `AcceptanceResult::total_duration` callers and update tests documenting the old timing meaning. Preserve the `cached` marker.

## Discriminating regression matrix

Turn the saved diagnostic into a regression whose **second** execution must still fail. Add positive-output success, negative-output failure, nonempty-stderr failure, expected-nonzero success, altered-pattern same-command, simple-to-extended, changed timeout, changed confinement, changed allowlisted environment, and repeated identical successful-contract cases. A constant-output command in a confined disposable fixture proves the positive hit path; do not use an inherited external marker as evidence of hermetic caching.

For fingerprinting, cover same-size same-mtime different bytes (including a file above the old 8MiB boundary), changed file during execution, untracked/ignored/external inputs, source symlink change, unreadable file, executable change, missing digest capability, corrupt/legacy record, and interrupted write. Assertions must observe whether the command actually executed and whether the full criterion passed, not merely a `cached` flag. Each cache-hit test has a neighboring mutation test that must miss.

Use injected roots and existing temporary Git-repository fixtures; never use the operator's live state or credentials. Record why each unknown-input case bypasses. Distinguish a correctness-required miss from a token optimization; a former false pass is not an eligible performance baseline.

## Orchestrator proof

```sh
cargo test --manifest-path loom/Cargo.toml --lib verify::criteria
cargo build --manifest-path loom/Cargo.toml --all-targets
cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path loom/Cargo.toml --check
```

The full integration suite remains the integration-verify gate. The owning standard stage must show both the saved reproducer's old behavior and the new failing-verdict preservation, plus positive reuse on a fully bound contract. No provider calls are needed.
