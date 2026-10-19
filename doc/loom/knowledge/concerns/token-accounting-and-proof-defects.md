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

> Token follow-ups

## Open Follow-Ups After the Token-Optimization Plan (2026-09-13)

Found during the plan's stages and integration-verify, verified against the tree at this page's
`verified` revision, and not fixed by the plan. Each names its owner.

- **Git runner does not isolate git's environment.** `git_command` (`loom/src/git/runner.rs`)
  sets `LC_ALL`/`LANG` and the replace-ref and graft shutoff but removes no `GIT_*` repository variable, so an inherited `GIT_DIR`, `GIT_WORK_TREE` or `GIT_INDEX_FILE`
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
- **IV fence wording.** the EXCEPTION line of `loom-hooks/_subagent-preamble.txt` (`:16`) still tells every IV review or
  verify subagent to run the full build, suite and linter, while the IV stable prefix assigns one
  canonical verifier (`orchestrator/signals/cache.rs:112-123`). The fence is byte-pinned by
  `tests_doctrine.rs`; a stage that owns the template must align both.
- **Deliberate fail-open choices.** `codex-forward-guard.sh`'s one-forward limit allows a call when
  the forwarder transcript is missing or unreadable (the harness may not have written it yet when
  PreToolUse fires); worker-bound brief records are never reset by compaction. Both stay until
  runtime evidence justifies failing closed.
- **Read receipts do not form** (missing `is_error` on successful results); the adjacency and
  PostToolUse-ordering assumptions stay unmeasured until they do. See "Delivery-Proof and
  Quota-Continuity Defects" below and the last section of the architecture page.
- **Operator cleanup.** Knowledge-bootstrap left stray fixture state and 11 fake forward records in
  the live `.loom/work` (recorded in [Verification Harness](../mistakes/verification-harness.md));
  agents never edit `.loom/work` directly.

## Open Follow-Ups From the Efficiency-and-Acceptance Plan (2026-09-19)

- **Knowledge check baseline.** After `./dev-install.sh`, run `loom knowledge check --strict`. It exits 0 on this tree,
  so no baseline file is needed; if a newer binary reports issues, record them with
  `loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt`, commit the file and gate with
  `--baseline` (see [knowledge-cli-gaps](knowledge-cli-gaps.md)).
- **Confirm the retrieval precision floor.** Run `loom knowledge eval`: `precision_floor` is 0.40 against a measured
  precision@5 of 0.45 on 2026-09-19, judged over the hook-delivered pack for prompt cases
  ([context-retrieval](../architecture/context-retrieval.md)).
- **Read the `peaks` section a week later.** `loom usage --since 7d` now prints peak resident context by scope
  (`commands/usage/sections/peaks.rs`): main and subagent p50, p90 and max, the share of transcripts above 250k and
  400k tokens, and for subagents the share whose peak never exceeded twice their first request (boot-dominated). The
  plan set no orchestrator context budget and no separate worker-brief budget key on purpose ("precision first,
  measure after"); this section is the measurement that decides whether either is needed.
- **No bounded build/test output command was built.** The `post-tool-use.sh` large-output notice covers the behaviour
  generally, and the by-command breakdown needed to design a dedicated command does not exist. Measure before adding.
- **24 oversized skills keep their bodies.** They cost tokens only when loaded and 77% were never loaded in the audit
  window; only their triggers and descriptions were in scope. Revisit if `loom usage` shows load cost.
- **Declared skills resolve against the home catalog only.** `check_declared_skills` reads `~/.claude/skills` plus the
  catalog, so a project-local skills directory is not consulted for a stage's `skills:` list
  ([plan-lifecycle-and-fields](../architecture/plan-lifecycle-and-fields.md)).

## Token Flow Gaps

Open items from PLAN-token-flow; the design is in [token-flow-sources](../architecture/token-flow-sources.md).

### Measurement Limits

- **Codex jobs require retained correlation evidence.** `tokens/discover.rs` combines legacy receipt threads with `codex_lifecycle/token_threads.rs` validated authorization-to-companion-job discovery. It includes running and completed jobs and historical stage sessions without Claude transcripts, and deduplicates threads before rollout lookup. If the companion job record is removed and no legacy receipt names its thread, an undiscovered rollout remains absent. Arbitrary lifecycle journal thread strings are not trusted.

- **Numbers are a lower bound.** The tracker reads local transcripts only. Compaction and auxiliary API calls may be missing from them, and a session whose transcript is not found keeps its stage out of `stages` (the UIs show `—`).
- **Other config homes are invisible.** Sessions under another `CLAUDE_CONFIG_DIR` or `CODEX_HOME` than the daemon's are not discovered: the daemon's environment allowlist (`daemon/server/environment.rs`) forwards `HOME` but not those two.
- **Codex fork rollouts would double count.** A forked rollout replays its parent's history and the tracker has no fork handling, so a forked thread counts the replayed turns again.
- **Running Codex jobs are discovered before forward receipts.** Validated companion job records provide `threadId` while the job runs. The legacy PostToolUse receipt still requires a forwarding result (`commands/hook/forward_receipt.rs:66-69`), but receipt lag no longer blocks an authorized job with a retained companion record.
- **A thread in two stages' receipts counts once.** `discover.rs` dedupes by path under the alphabetically first stage; if the other stage already accumulated it, its state is kept and the thread double counts across stages. Needs a thread forwarded from two stages.
- **Flat usage can lose the time-left label.** With no history row inside the 30-minute window only the snapshot point remains, and fewer than 2 points gives no label although a slope-0 "lasts to reset" would be the better answer (`quota_points.rs`).

### Silent Degradation

- **Receipt discovery failures remain silent.** Legacy receipt load errors skip receipt-only threads without a warning. Authorized companion-job discovery can still supply the same thread; previously accumulated state remains in the checkpoint.
- **A checkpoint over 8 MiB means a full rescan every pass.** Each Claude transcript keeps up to 4096 message ids (about 55 bytes each, around 225 KB per long transcript), so about 35 long main transcripts in one checkpoint pass `MAX_CHECKPOINT_BYTES`. The tracker sheds the id tables of idle agents and warns once; active agents' tables are never shed.
- **A deterministic panic prints every pass.** The `catch_unwind` arm in `guarded_pass` has no test, and the default panic hook prints a line per pass, defeating `log_change`'s dedupe.

### Hardening Backlog

- **Quota cache/history file reads refuse nonregular files.** `quota/cache.rs:45-60` and `quota/history_store.rs:74-88` use `fs::safe_read::read_to_string_bounded`; a FIFO at either path is rejected, so the readers do not retain the blocking `File::open` gap.
- **Writes by path after the symlink check.** `checkpoint.save` and `write_flow` write by path seconds after `ensure_tokens_dir` checked the directory (a first pass can take seconds); recheck before each write or write through a directory fd. Exploiting it needs write access to `.loom/work`, which sandboxed sessions lack.
- **Unbounded fan-out and id length.** `discover.rs` does not cap subagent files per directory or agents per stage, and `MessageTable` (`accumulate.rs`) caps the number of ids (4096) but not their length.
- **`by_model` is capped at ingest, not when a checkpoint loads.** `add_model_usage` holds an agent to `MAX_MODEL_KEYS` keys plus `other`, but a hand-planted 8 MiB checkpoint can carry tens of thousands of entries per agent, and `add_model_usage` finds its key by a linear scan on every ingested line. `model_totals` aggregates in a `BTreeMap`, so only that scan and the flow size grow. Truncate `by_model` on load. It needs write access to the 0700 tokens directory.
- **A planted `flow.json` can still reach the UIs oddly.** `sanitize_token_flow` truncates `models` at `MAX_MODEL_ROWS` instead of folding the rest into `other`, and turning a blank effort into `None` can leave two `(model, None)` entries, which collide as React keys in the by-model table (a warning). The tracker's own flow has neither.
- **The pass memo takes its directory from callers.** `take_checkpoint`, `publish_flow` and `quota_left` take their root from the caller rather than `self.work_root`; it is correct because `run_pass` passes one root, and reading the field would make cross-root use impossible by construction.

### Structure

- **Shared discovery is below the commands layer.** The claim that `tokens` imports discovery from `commands::usage::codex_discovery` is incorrect: `tokens/discover.rs:9` imports `codex_root`, `collect_jsonl` and `CodexDiscovery` from `provider_data::codex_discovery`. `rollout_files` still drops unreadable-directory diagnostics.
- **Small couplings.** `checkpoint.rs` imports `CHECKPOINT_FILE` from `tracker.rs` while `tracker.rs` imports `Checkpoint`; the bucket index `div_euclid(BUCKET_SECS)` is inlined in `accumulate.rs`, `flow_build.rs` and `tracker.rs`; `tracker.rs` carries a `thread_local` `TrackerLocal` only because `scan_pass`'s signature is pinned (`guarded_pass` and `scan_until` share a module and could pass the flag directly); `CodexLine::SessionMeta` is parsed and never consumed, `tokens/accumulate_tests.rs` sits at 399 of the 400-line limit (the next test there needs a sibling test file), and `claude_line`, `codex_line` and `TRACKER_INTERVAL_SECS` are `pub` with no outside user, which hides dead code from rustc.
- **Layering of TUI helpers.** `compact_tokens`, `spark` and `quota_left_text` are shared by `header.rs`, `cells.rs` and `quota.rs` but live in `tokens_pane.rs`. The `watch_lock_identity` doc comment in `daemon/server/lifecycle.rs` names only `tick::record` and the quota poller as path-writing threads, not the tracker.

### UI Polish

- `compact_tokens` returns 6 or more cells at 10M and above (`10.00M`), past the 5-cell rate slot (`{:>5}`) in `cells.rs`; only a stage rate of 10M fresh per minute reaches it.
- Below 74 columns the pane title row's "no active agent" note is cut; put the note before the legend text or drop the legend text when a note is present.
- On phones the ledger's mobile card layout always shows `tokens —` because the cell is never `:empty`.
- The 20-point 56x12 sparkline config is duplicated between `ledger-row.tsx` and the node tray; sparklines share `lastValues` padding.
- `token-flow-panel.tsx` `StageChart` is 94 lines (the 50-line guideline is not enforced for TSX) and zero-window stages draw zero-area stroked polygons; the six-colour palette repeats after six stages.- **Legend rate.** `modelRates` keys each agent's 5-minute rate by its last model while shares key by per-line model, so an agent that switched models attributes its whole rate to the new one and `other` always reads 0/m.
- **Chip trends.** The "fresh so far" trend on a 0-based sparkline is nearly flat at the top for a stage older than an hour (subtract the window minimum), and the leading null buckets of the cache-hit trend carry forward as 0%, so a young stage appears to climb from 0.
- **TUI by-model block.** `model_lines` cuts at `MODEL_BLOCK_MAX_ROWS` (10) with no "more" marker: past four models with efforts the smallest vanish silently and the visible shares stop adding to 100%, and `name_cells` is measured over groups the cut may hide. `render` (`layout.rs:62`) also rebuilds the pane and re-runs `budget` for the live-only fallback when the pane has no by-model block, where the second build is identical.
- **Hint and types.** The uncached-input hint is a `title` attribute, unavailable on touch and keyboard (an accessible disclosure would replace it). `Pie` types its prop as `ReturnType<typeof modelShares>` although `ModelShare` is exported.

### Test Weaknesses

- The frozen contract `rate-is-five-minute-average` stays green under an 11-14 bucket rate window (the outside line sits at `now - 420 s`); `flow_build.rs` uses `RATE_BUCKETS` = 10 correctly. A contract revision should place the outside line in bucket 11 (a dispute, not an edit).
- The contract `tracker-memo-keeps-work-roots-apart` stays green with the memo's `is_for` guard removed: its two fixtures differ in transcript paths and flow hash, so cross-root reuse is unobservable (`tracker.rs:106`). The implementation is right; the contract is weak.
- The contract `stage-chips-carry-trends` stays green when a chip draws an empty series, because an empty `Sparkline` still renders `role="img"`.
- No test clicks Back to details in control mode (`web/src/components/stage-modal.test.tsx`).
- `quota/codex_tests.rs`: when `poll_once` fails early the pid-file watcher keeps polling for up to `REPLY_DEADLINE` (60 s) before the scope joins and the `expect` panics; a shared done flag would end the wait. The detached `std::thread::spawn` in `a_shutdown_flag_set_mid_exchange_returns_quickly` predates the scoped helpers.

### Proposals

- **Dist freshness check.** A `loom plan verify` or gate check that rebuilds `web/dist` and compares it would end the recurrence in [Token-flow delivery](../mistakes/verification-v2-delivery.md) and the legend-text mirror in [verification-v2-delivery](../mistakes/verification-v2-delivery.md).
- **Commit-output filter.** A PreToolUse hook that refuses `loom stage commit` followed by a pipe or redirect would end the lost commit ticket, which stages of two plans hit (token-core and tui-tokens, then web-insights of the live-token-insights plan) after [Memory Relay Drain Gap](../mistakes/memory-relay-drain-gap.md) was written.
- **Wiring detector and `#[path]` test modules** (first recorded in [Test Modules Wired by `#[path]` Read as Unwired](../mistakes/verification-v2-delivery.md), now failing a stage's finish step). `verify/wiring_detection.rs` `detect_unwired_files` matches only `mod <stem>` or `use .*<stem>`, and its `is_test_file` misses file names ending in _tests or starting with tests_, so a test file wired by a `#[path]` attribute on `mod tests;` is flagged and the stage's finish step fails; the error is lost when impact-test notes overflow the tool output.
- **Integrity events.** `loom stage review integrity` reported no events after a commit although width-test assertion numbers changed in several `tui/ledger` test files; the gap is in the detector's reading of a numeric edit.

## Token Model Blank-Row Investigation

`web/src/components/token-models.tsx:114-123` groups rows by model and uses a row-spanning model cell rendered only on the first effort row. Subsequent effort rows look blank in the Model column, but retain the same model. The xhigh row below claude-opus-5-5 is an effort continuation, not a missing model; repeating the label or strengthening group styling would remove the ambiguity.

## Stage Token Metric Layout

Stage-modal token metrics align values left and labels right, with a full-width sparkline below. Sparkline stretching is opt-in so other consumers retain their fixed sizing. Narrow metric containers allow labels to wrap while remaining right-aligned.

## Codex Token Discovery Repair

Token discovery combines legacy forward receipts with validated authorization-to-companion-job correlation in codex_lifecycle/token_threads.rs. Running and completed jobs contribute to agent, stage, model and plan totals even without a Claude transcript. Duplicate thread sources count once. Regression coverage verifies a 98,821 fresh-token Terra fixture and rejects unrelated jobs, wrong state roots and ambiguous authorizations. Companion records must remain available unless a legacy receipt names the thread.

## Model Legend Layout

The by-model legend aligns swatches, model names, shares and rates in shared grid columns. Shares and rates align right. Below a container width of 26rem, the donut stacks above the full-width legend; wider containers display them side by side. Palette and token calculations are unchanged.

## Delivery-Proof and Quota-Continuity Defects

Verified against the tree and this machine's transcripts; each blocks a token-saving claim that
depends on it. The remediation sequence is `doc/plans/optim-token-metaplan.md`.

- **Read receipts cannot form.** `result_body` (`loom/src/context/read_receipts/transcript.rs:142`)
  returns `None` when a tool_result has no `is_error` field, and successful `Read` results have
  none. Independently, Claude Code writes a call's tool_result row only after its PostToolUse hooks
  finish (828 of 831 sampled hook rows precede their result row), so completion at
  `loom-hooks/post-tool-use.sh:257` never sees the result. Fixtures mask both by writing the pair
  before calling complete. Identical same-agent rereads are about 0.4% of Read characters.
- **Receipt intents are stranded.** The Read PreToolUse path prepares intents in every session,
  and `post-tool-use.sh` exits before `read-receipt --complete` when the stage environment is
  absent; stage sessions strand them too (18 in one session).
- **Forward receipts never form.** Two causes, each enough alone. `loom hook forward-receipt`
  reads the transcript at PostToolUse, before the call's result row exists, and skips calls with no
  result (`loom/src/commands/hook/forward_receipt.rs`, `process`). Its argv parser
  `exact_forward_argv` (`loom/src/models/forward_receipt/transcript.rs`) accepts only the 8-word
  wrapper argv, while recorded forwards carry `--unit-id` (10 words) and `codex-forward-guard.sh`
  (`emit_authorized_input`) rewrites the executed command to 12 words with `--invocation-id`. No
  `forward-receipts.jsonl` exists, and Codex rows have no thread linkage.
- **Worker-brief records never bind.** 201 `worker-brief/*.pending` files and 0 bound `*.json`
  records exist across this machine's projects; the record loader reads only `*.json`
  (`loom/src/context/delivery.rs:273-277`). This repository's own stages issued no brief at all
  (0 of 44 subagents in two plans carry the marker); the cause is not established.
- **Compaction keeps spawn-brief credit.** `loom hook pre-compact` rotates the Read epoch and
  deletes only the prompt recipient's delivery record (`loom/src/commands/hook/pre_compact.rs:61-96`).
  `already_delivered` (`loom/src/commands/hook/user_prompt.rs:239-248`) still passes the
  `LOOM_SESSION_ID` spawn recipient, and `delivered_to_session` (`loom/src/context/delivery/session.rs:116-123`)
  credits its records while the context epoch is unchanged. The case has not occurred: 0 of 282
  stage transcripts on this machine compacted, and a compacted stage session re-reads its signal
  (`loom-hooks/session-start.sh:147-155`).
- **PreCompact is not registered globally.** `loom/src/fs/permissions/hooks/config.rs` `build()`
  registers no PreCompact hook; only stage sessions get one, so ordinary sessions, which do compact
  (11 transcripts on this machine), keep their prompt delivery record after compaction.
- **Quota continuity reads jitter as a reset.** `continuity_from_windows`
  (`loom/src/quota/history.rs:211-212`) marks any `resets_at` change as a reset, and
  `reset_incomparable` (`loom/src/commands/usage/comparison_quota.rs:121-128`) compares `resets_at`
  exactly across arms. The Claude five-hour and seven-day `resets_at` values alternate by one
  second between polls. Quota is also reported in whole percents, so a subscription verdict cannot
  resolve a plausible saving even once the jitter is tolerated.
