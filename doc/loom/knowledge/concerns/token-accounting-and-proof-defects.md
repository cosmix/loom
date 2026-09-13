---
sources:
- loom/src/verify/criteria/cache_contract.rs
- loom/src/verify/criteria/tests/runner_tests.rs
- loom/src/commands/usage/transcript_merge.rs
- loom-hooks/poll-guard.sh
- loom/tests/integration/hooks_poll_guard_subagents.rs
- loom-hooks/tests/codex-forward-guard-blocks-edit.sh
- loom/src/git/runner.rs
- loom/src/process/mod.rs
- loom/src/process/environment.rs
- loom/src/quota/history_store.rs
- loom/src/commands/usage/sections/edits.rs
- loom/src/fs/safe_read.rs
- loom-hooks/codex-forward-guard.sh
- CLAUDE.md.template
verified: 499b09b6297aeee4896a66df3da86d00f652a618
---
# Token Accounting and Proof Defects (open at 7d6a14ca)

> Resolved plan defects and open token-accounting follow-ups

## Criterion Cache False Pass (owner: measurement-and-cache)

**Resolved by measurement-and-cache (2026-09-13).** Was: `run_with_cache` stored a pass on raw
exit success before the extended criterion ran, and a cached result was rebuilt with empty
stdout/stderr and exit 0, so `stdout_not_contains` and empty-stderr criteria passed falsely on the
second run; the key omitted environment, confinement, timeout and the extended criterion.

Now the cache stores only a certified, fully evaluated pass keyed by a complete
`CriterionContract` (`loom/src/verify/criteria/cache_contract.rs`: command, simple or extended
kind, expected exit, both output predicate lists, the empty-stderr flag, timeout, confinement and
the input fingerprint; `CACHE_RECORD_VERSION` 2, bounded 4 KiB diagnostic tails). Unknown
eligibility is a miss. Regression: `forbidden_output_failure_executes_and_fails_twice`
(`loom/src/verify/criteria/tests/runner_tests.rs:179`). The audit's standalone reproducer program
was never committed; that test is the reproduction.

## Usage Stream Undercount (owner: measurement-and-cache)

**Resolved by measurement-and-cache (2026-09-13).** Was: `merge_request` kept the first nonzero
usage of a streamed message (audit: 24.53% of output missed), `UsageArgs` had `--since` only, and
discovery prefiltered files by mtime before event time.

Now `merge_usage_observation` (`loom/src/commands/usage/transcript_merge.rs:10-31`) replaces the
whole usage vector with the latest observation by `(timestamp, line_ordinal)` and counts the
observations; `--until` gives an inclusive event-time upper bound, and the mtime prefilter is gone.
See [Token Accounting and Receipts](../architecture/token-accounting-and-receipts.md).

## `loom subagents list` Polling Evades Poll-Guard (owner: job-lifecycle)

**Resolved by job-lifecycle (2026-09-13).** Was: the repeat rule counted only programs in its
read-only allowlist, which lacked `loom`.

Now `loom-hooks/poll-guard.sh:105-110` counts `loom subagents list` (declared flags plus at most one
display pipe) toward the repeat rule. When the stage has a `forward-receipts.jsonl`, its guidance
names the exact wait (`loom subagents wait --receipt <id> --timeout 3600`, `poll-guard.sh:155-190`)
instead of more polling. Test: `loom/tests/integration/hooks_poll_guard_subagents.rs`.

## Hook Tests Write Into the Live Stage Ledger (owner: job-lifecycle)

**Resolved by job-lifecycle (2026-09-13).** Was: the `codex-forward-guard-*` hook tests
inherited `LOOM_STAGE_ID`/`LOOM_SESSION_ID`/`LOOM_WORK_DIR` from the running session, so every
hook-suite run inside a live stage appended fake forward records to that stage's own
`.loom/work/subagents/<stage>/codex.jsonl` (11 in one knowledge-bootstrap run).

Now each test unsets the `LOOM_*` identity on its own line 3
(`loom-hooks/tests/codex-forward-guard-blocks-edit.sh:3`), and
`codex-forward-guard-live-identity.sh` covers the live-identity path. IV's gate run left no
`codex.jsonl` for its stage. The unset has to live in the test script: `commit-filter.sh` blocks an
orchestrator's own Bash call that unsets `LOOM_MAIN_AGENT_PID`.

## Open Follow-Ups After the Token-Optimization Plan (2026-09-13)

Found during the plan's stages and integration-verify, verified against the tree at this page's
`verified` revision, and not fixed by the plan. Each names its owner.

- **Git runner does not isolate git's environment.** `run_git_program` (`loom/src/git/runner.rs:26`)
  sets only `LC_ALL`/`LANG`, so an inherited `GIT_DIR`, `GIT_WORK_TREE` or `GIT_INDEX_FILE`
  redirects every loom git call, knowledge evidence freshness included, to another repository.
  Owner: `git/runner.rs`; `env_remove` the `GIT_*` repository variables.
- **`process::run_bounded_output` bounds time, not bytes.** Its reader threads `read_to_end` with
  no cap (`loom/src/process/mod.rs:209`), so callers' output limits (for example
  `MAX_GIT_OUTPUT_BYTES` in `fs/knowledge/catalog/evidence.rs`) cap the accepted result, not peak
  memory. Owner: `process/mod.rs`; read through `take(limit + 1)` and error over the limit.
- **Two atomic-write implementations for quota state.** History rewrites reuse
  `quota/cache.rs`'s `atomic_write` instead of `fs::locking::atomic_write_locked`, and
  `ensure_history_dir` (`loom/src/quota/history_store.rs:55`) checks for symlinks before
  `locked_dir_update` takes the lock (TOCTOU; low, since the quota dir is 0700 and single-writer).
- **Legacy stage inference from a project slug.** `commands/usage/sections/edits.rs:26` infers a
  stage from `project_slug` via `stage_id()`, the pattern the provider ledger forbids for new
  attribution. `cache_ignore.rs::check_ignore` duplicates `process::run_bounded`'s spawn and wait
  handling except stdin.
- **No shared any-path bounded reader.** `usage --compare` keeps a local bounded reader because
  `fs/safe_read.rs::read_bounded` is root-relative and no-follow (see
  [Token Accounting and Receipts](../architecture/token-accounting-and-receipts.md)); a shared
  any-path variant in `fs/safe_read.rs` would retire the copy.
- **`#[serial]` env mutation races non-serial siblings.** `verify/criteria` runner tests mutate the
  process-wide `LANG` through `EnvGuard` (`loom/src/verify/criteria/tests/runner_tests.rs:320`);
  `apply_stage_environment` (`loom/src/process/environment.rs:80`) re-reads the process environment
  per run, so a concurrent non-serial test
  that spawns a confined command could fingerprint the mutated value. Not observed failing; pass the
  allowlisted env explicitly in those fixtures.
- **IV fence wording.** `CLAUDE.md.template`'s Rule 5 EXCEPTION still tells every IV review or
  verify subagent to run the full build, suite and linter, while the IV stable prefix assigns one
  canonical verifier (`orchestrator/signals/cache.rs:112-123`). The fence is byte-pinned by
  `tests_doctrine.rs`; a stage that owns the template must align both.
- **Deliberate fail-open choices.** `codex-forward-guard.sh`'s one-forward limit allows a call when
  the forwarder transcript is missing or unreadable (the harness may not have written it yet when
  PreToolUse fires); worker-bound brief records are never reset by compaction. Both stay until
  runtime evidence justifies failing closed.
- **Read-receipt runtime assumptions** (adjacent rows; transcript written before PostToolUse) are
  unmeasured; see the last section of the architecture page.
- **Hook size.** `loom-hooks/poll-guard.sh` grew past 400 lines. Hook scripts ship as
  `include_str!` constants (`loom/src/fs/permissions/constants.rs`) and are registered in
  `fs/permissions/hooks/config.rs`, so splitting a helper out needs ownership of both files, and
  `loom-hooks/tests/run-all.sh` lists every hook test explicitly.
- **Operator cleanup.** Knowledge-bootstrap left stray fixture state and 11 fake forward records in
  the live `.loom/work` (recorded in [Verification Harness](../mistakes/verification-harness.md));
  agents never edit `.loom/work` directly.
