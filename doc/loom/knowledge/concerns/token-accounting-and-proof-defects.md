---
sources:
- loom/src/verify/criteria/runner.rs
- loom/src/verify/criteria/result.rs
- loom/src/verify/criteria/cache.rs
- loom/src/commands/usage/transcript.rs
- loom/src/commands/usage/discovery.rs
- loom-hooks/poll-guard.sh
- loom-hooks/codex-forward-guard.sh
verified: 7d6a14caf1750cc1e516519e650e2ee68641e0a1
---
# Token Accounting and Proof Defects (open at 7d6a14ca)

> Cache false pass, usage undercount, poll-guard gap, hook tests polluting ledgers

## Criterion Cache False Pass (owner: measurement-and-cache)

`run_with_cache` (`loom/src/verify/criteria/runner.rs:159-208`) stores a pass on raw exit success before `check_extended_criterion` runs; `CriterionResult::cached` (`loom/src/verify/criteria/result.rs:50-61`) rebuilds empty stdout/stderr with exit 0. `stdout_not_contains`/empty-stderr criteria falsely pass on the second run and positive-output checks falsely fail. `compute_cache_key` (`cache.rs:204-236`) omits environment, confinement, timeout and the extended criterion; files over 8MiB key on size plus whole-second mtime. Reproduction: `doc/token-optimization-2026-09-12-data/cache_probe.rs` printed `first_passed=false second_passed=true second_cached=true`.

## Usage Stream Undercount (owner: measurement-and-cache)

`merge_request` (`loom/src/commands/usage/transcript.rs:216-225`) keeps the first nonzero usage of a streamed message; final usage is the correct vector (audit: 24.53% of output missed). `UsageArgs` (`commands/usage/mod.rs:32`) has `--since` only, and `is_recent` (`discovery.rs:271-277`) prefilters by mtime before event time.

## `loom subagents list` Polling Evades Poll-Guard (owner: job-lifecycle)

The repeat rule (`loom-hooks/poll-guard.sh:132-152`, warn at 3, deny at 5) only counts programs in its read-only allowlist, which lacks `loom`.

## Hook Tests Write Into the Live Stage Ledger (owner: job-lifecycle)

`codex-forward-guard-blocks-edit.sh`, `codex-forward-guard-quoting.sh`, and `codex-forward-guard-bash-companion-only.sh` inherit `LOOM_STAGE_ID`/`LOOM_SESSION_ID`/`LOOM_WORK_DIR` from the running session, and `loom-hooks/codex-forward-guard.sh` writes its ledger through them. Running the hook suite inside a live stage session (job-lifecycle, context-admission, and integration-verify each run `bash loom-hooks/tests/run-all.sh`) appends fake forward records to that session's own `.loom/work/subagents/<stage>/codex.jsonl`; observed: 11 fake records in one knowledge-bootstrap run, none from a real Codex job.
