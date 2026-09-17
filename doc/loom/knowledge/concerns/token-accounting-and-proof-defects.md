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
# Token Accounting Follow-Ups

> Open follow-ups from the 2026-09-13 token-optimization plan

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
