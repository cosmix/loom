# Usage Evidence

>

## Completeness

Each provider report carries `completeness` (`commands/usage/completeness.rs::assess`): `measured`,
or `partial` with reason codes in a fixed order: `missing-roots`, `unreadable-files`,
`malformed-rows` (unparseable lines), `unreadable-session-records`, `missed-session-transcripts`
(a session record names an in-window transcript the report did not read), `unknown-usage-rows`,
`ambiguous-rows`, `codex-scope-unavailable`, `malformed-forward-receipts` (a damaged or truncated
receipt line, Codex only: it can hide a project thread), `forward-thread-missing` and
`forward-receipt-unthreaded`. The first four and the receipt reasons are the evidence-loss
reasons. `fallback_coverage_rows` is a coverage count and `missing_timestamp_rows` counts every row
type without a timestamp, so neither is a reason; a usage row without a timestamp is dropped
without the report reading partial
([concern](../concerns/token-accounting-and-proof-defects.md#delivery-proof-and-quota-continuity-defects)).

Session-record inspection (`missed_claude_transcripts`) canonicalizes the work dir once, because
from a stage worktree `.loom/work` is a symlink and the bounded record read opens its root without
following links
([mistake](../mistakes/completion-runner-blocks.md#a-symlinked-work-dir-made-every-session-record-read-as-unreadable)).
Per-record diagnostics print to stderr through `inline_safe`.

## Attribution Without a Hook Ledger

`loom usage` joins each transcript to the hook ledgers first; `commands/usage/parent_spawns.rs::ParentSpawns::fill_fallbacks` fills only a field the join left `None`, so a ledger value always wins.

- **Stage.** From the project slug of the transcript's directory: `discovery.rs::stage_from_project_slug` takes the remainder after `<repo-slug>--worktrees-` (the exact remainder when the repo slug is known, otherwise only a remainder with no second delimiter), and accepts it only when it is non-empty and uses `[A-Za-z0-9_-]`. `ParentSpawns::new` loads the recorded stage ids once (`discovery::recorded_stage_ids` through `read_stage_ids`, which strips the depth prefix: `01-core.md` is stage `core`); a slug remainder outside that set names no stage, so a session started in `.worktrees/foo/loom` is not stage `foo-loom`. With `--all` or no readable work dir the set is `None` and the slug parse stands unchecked.
- **Requested model of a subagent.** The sidecar's `model`, else the `model` of the parent transcript's spawn `tool_use` (`index_parent` streams the parent line by line, skipping lines over 8 MiB), else `default`. A subagent row therefore reports its requested model (`sonnet`, `opus`, `haiku`) and is never unknown when either source exists.
- **`--stage` filter.** `discovery.rs::filter_stages` matches `ends_with("-worktrees-<stage>")`, so `--stage b` also selects the slug of stage `a-b` ([code-quality-and-hook-debt](../concerns/code-quality-and-hook-debt.md)).
- **`loom usage --quota`** prints the `## Model Budget` section a stage signal would carry, or a no-data line ([signal-generation.md](signal-generation.md#model-budget-section-formatbudgetrs)); it needs no stage and reads no transcript.

## Codex Scope

The Codex store is shared by every project on the machine, so a Codex row counts for a project only
when a forward receipt of that project names its thread (`codex_scope.rs`, `CodexScope`); every
other row is foreign and dropped. Without readable receipts the scope is `Unavailable` and the
report reads `codex-scope-unavailable`; `--all` is unscoped. A receipt's `observed_at` is its
PostToolUse instant, an end time, so rows are windowed by their own timestamps and a thread is
checked for missing rows only when its receipt lies in `[since + MAX_FORWARD_SECS (600), until]`
(`forward-thread-missing`); `forward-receipt-unthreaded` counts receipts without a thread id
observed in `[since, until]`. `from_receipt_threads` builds a scope from receipt threads without a
`ForwardJoin`. Scope and session-record completeness read only the current `.loom/work`;
`loom init` archives and removes it (`commands/init/cleanup.rs::cleanup_work_directory`), so a window before the
last init reads 0 Codex tokens.

## Run Evidence

`loom usage --run-evidence <declaration.json> --project <dir> --since <rfc3339> --until <rfc3339>`
(`run_evidence.rs`, `run_evidence_stages.rs`) writes one run's `RunEvidence`, the input
`--compare` pairs. The declaration (at most 1 MiB) supplies the identities the tooling cannot
measure; token vectors, retries, fixes, the critical path and open review findings are measured.
The stage set is every stage that started inside the window; a stage straddling either bound
aborts the producer, and stages are sorted by id before that check. The producer refuses in-scope
usage from a provider the declaration does not assign, and an assigned provider reading `partial`
marks the run's provenance partial. Finding severity maps `critical`→critical, `major`→high,
`minor`→medium, anything else→low. Produced evidence carries `notification_latency_ms: null`. The
flag conflicts with `--compare`, `--all`, `--provider`, `--stage` and `--plan`, and requires
`--project`, `--since` and `--until`. The procedure that uses it:
[Live Paired Trials](#live-paired-trials).

## Live Paired Trials

For a change that needs live evidence (frozen replays of later plans, or a question the offline
comparison cannot settle). Live trials consume subscription quota, so each needs operator
scheduling and authority, and runs from an ordinary shell with no other Claude or Codex session
open: a stage session cannot run `loom run`, and its own consumption would count as concurrent use
(`comparison_quota.rs`, `unrelated_use`). No stage runs this procedure.

1. **One clone per run.** Each run gets a new clone at `<experiment-root>/<intervention>-<arm>-<run>`,
   outside this repository, so no two runs share a `~/.claude/projects` slug. Reset the clone's
   checked-out branch to the pinned revision and use it as the plan's base branch; no branch is
   created. Write the same `.loom/config.toml` into every clone of an experiment (`.loom/` is
   gitignored): no intervention dimension covers configuration, so its digest is part of the
   environment identity, and an intervention that is a configuration change ships as a revision
   that changes the default.
2. **Shared install, one arm at a time.** A clone isolates only `.loom/work` and
   `.loom/config.toml`. The `loom` binary, the hooks under `~/.claude/hooks/loom/`, the agents,
   the skills and `~/.claude/CLAUDE.md` are shared, so arms run one at a time, and before each arm
   `./dev-install.sh` installs that arm's revision from its clone. The declaration's
   `environment_id` is a digest of what must not vary between arms: host and OS, the `claude` and
   `codex` CLI versions, `~/.loom/config.toml`, the clones' shared `.loom/config.toml`, and
   `~/.claude/settings.json` unless `./dev-install.sh` rewrites it (then its digest is
   provenance). It never includes the installed binary's revision or the installed assets: those
   come from the arm's revision, the declared `implementation_revision`, and `--compare` requires
   `environment_id` equal (`incomparable-environment`), so an identity carrying them makes every
   implementation-change pair incomparable. Record the installed binary's revision and the
   installed assets' digests per run in `<run>/provenance.json` beside the run's `RunEvidence`, and
   check the binary's revision equals the declared `implementation_revision` before the run starts.
3. **Freeze the inputs.** Fix the workload fixtures, the source-input revision and digest, the
   acceptance contract, the required commands, the review dimensions, and the providers, models
   and effort. Name the one intervention; the manifest lists one changed dimension, because an
   empty `changed_dimensions` is invalid.
4. **Cover the outcomes.** Include successful work, expected failures and review-heavy work, with
   the same checks and independent review in both arms. A clone at a new path starts cold; a warm
   start is a second run in the same clone inside the first run's cache TTL, started only after
   the first run's evidence is produced and saved (step 7). Its window must not overlap the first
   run's: the producer takes a run's stages by window, so the first run's stage records in the
   same `.loom/work` stay out of the second run's evidence.
5. **Interleave and keep everything.** Interleave baseline and candidate order across
   repetitions, and keep every outcome, including failures, retries, fixes and incomplete runs.
   A missing measurement is unknown, never zero.
6. **Measure only canonical rows.** Only `measured-canonical` rows of `loom usage --provider all
--since <start> --until <end> --json` are measurements. `duplicate-exact`,
   `ambiguous-conflict`, `fallback-coverage`, `synthetic` and `unknown-usage` rows, and every
   absent dimension, go into the artifact as missing. A clone isolates Claude slugs only; Codex
   rollouts share `~/.codex/sessions`, so copy each run's rollouts out at the end of the run (they
   are not retained), and rely on forward-receipt scoping to keep another arm's threads out.
7. **Produce the evidence at once.** Run `loom usage --run-evidence <declaration.json> --project
<clone> --since <start> --until <end>` as soon as the run ends, before the next run starts in
   any clone, and keep the file: it is the run's evidence and is never produced again later,
   because review rounds and dispute rulings recorded afterwards change `open_findings`. The
   declaration names every provider the run used; the producer refuses a run with in-scope usage
   from an undeclared provider. The independent reviewer replaces the local finding ids
   (`<stage>:F-<round>-<k>`) with cross-arm ids and keeps the severity mapping (critical, major,
   minor and unspecified map to critical, high, medium and low). Assemble the pairs into an
   artifact that declares `"token_rule": "net-total"` and a `"latency_tolerance_ms"` (produced
   evidence carries `notification_latency_ms: null` and needs the tolerance to reach
   `supported-candidate`), fix the pair count and the stopping rule before the first pair, and run
   every retained artifact through `loom usage --compare <artifact> --json`. Any new defect,
   omitted requirement, weakened check or review, failed semantic completion, or latency
   regression beyond the tolerance rejects the candidate, whatever the average. A rejected
   candidate is reverted through the operator's ordinary deployment procedure, and its evidence
   is kept.
8. **Reading the verdict.** The net-total rule needs at least 3 pairs per artifact (fewer read
   `insufficient-pairs`) and judges the mean difference against 2 standard errors
   ([Offline Comparison](token-accounting-and-receipts.md#offline-comparison-loom-usage---compare)).
   A check that asks for a median computes it from `.pairs[].baseline_provider_tokens` and
   `.pairs[].candidate_provider_tokens` of the `--compare --json` report. Quantities
   `RunEvidence` does not carry are read from `loom usage --project <clone> --since <start>
--until <end> --json` over the same window: API calls from
   `.provider_ledger.providers[].coverage.measured_responses`, cache read and cache write from the
   provider token vectors, tokens before the first edit from `.lifecycle.orientation`. A window
   opened before the clone's last `loom init` cannot be measured: Codex scope and session-record
   completeness read only the current `.loom/work`.

## Token Usage Sources

- **Claude transcript lines.** Assistant lines carry `message.id`, `message.model`, `message.usage` (`input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`) and an RFC 3339 timestamp. One API response spans several lines sharing `message.id`. Subagent transcripts are `<dir>/<session uuid>/subagents/agent-<agentId>.jsonl` with `isSidechain` true and `agentId`.
- **Claude effort.** Claude Code 2.1.300 assistant lines also carry top-level `effort` and `perTurnEffort` (equal on 2342 of 2344 sampled lines; one line carried only `perTurnEffort`). A subagent's lines carry its own effort, which can differ from its parent's. The sidecar `agent-<id>.meta.json` names the model as an alias (`haiku`) and has no effort key. Loom's Session record stores neither model nor effort (`models/session/types.rs`), so the transcript is the only per-agent source.
- **Uncached input is tiny by design.** `input_tokens` counts only what follows the last cache breakpoint: about 2 per API call in Claude Code sessions (three transcripts measured), so the web mix's "uncached input" row stays near twice the call rate while fresh tokens are mostly cache writes and output.
- **Codex rollouts.** `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<thread_id>.jsonl`, written line by line. `event_msg` `token_count` carries cumulative `total_token_usage` and per-call `last_token_usage`. `total_tokens = input_tokens + output_tokens`, so `cached_input_tokens` and `cache_write_input_tokens` are parts of `input_tokens`. `info` null is a rate-limit ping. `turn_context.payload.model` names the model and `turn_context.payload.effort` the reasoning effort. The prolite plan reports only a weekly window (`primary.window_minutes` 10080, `secondary` null).
- **Forward receipts.** `.loom/work/subagents/<stage>/forward-receipts.jsonl` (`loom/src/models/forward_receipt.rs`) record `codex_thread_id`; loom never passes `--ephemeral` to codex. PostToolUse writes a receipt only after the forwarder's foreground Bash call returns, so a running Codex job has no receipt (and no `codex_thread_id`) yet. Companion job records under `~/.codex/plugin-data/state/*/jobs/*.json` carry `threadId` while running.

## Status Pipeline and Web Contract

- The daemon status broadcaster re-reads files every second and never reads in-memory orchestrator state. A new status field needs a producer that persists a file the collector reads (the quota cache pattern).
- `loom/src/commands/status/web/model_tests.rs` `fixture_matches_serde_output` pins exact equality between the Rust serializer and `web/src/api/fixtures/snapshot.json`. `stageSummarySchema` and `snapshotSchema` are `.strict()`; `statusDataSchema` is not.
- `web/dist` is committed and embedded by `loom/build/assets.rs`. Cargo never runs bun; vite build output is deterministic.

## Quota History

- The quota poller samples every 180 s, so quota slopes need a window of about 30 minutes.
- `.loom/work/quota/history/<provider>.jsonl` flips the five-hour `resets_at` between two values one second apart on successive polls. `loom/src/quota/history.rs` `continuity_from_windows` marks a `resets_at` difference of more than `quota::RESET_TOLERANCE_SECS` (300) in either window as `Reset`, and the history store skips a row identical to the last one. The tracker (`tokens/quota_points.rs`) ignores `HistoryContinuity` and segments on `resets_at` with the same constant, because the continuity mark also flags seven-day window changes.

## Maintainability Baseline

`loom/maintainability-baseline.txt` records exact sizes for existing violations. `tests/maintainability/baseline.rs` rejects growth, requires a reduced entry when an existing violation shrinks, and requires removal once it is under the limit. Version 2 plans may also declare this file as a `ratchet_files` entry, so follow the applicable plan integrity rules when updating it.

## Data Flow

The daemon owns a `tokens-tracker` thread (`spawn_token_tracker`, `loom/src/tokens/tracker.rs::spawn_token_tracker`, started with the quota poller in `daemon/server/lifecycle.rs` and joined at shutdown). Every `TRACKER_INTERVAL_SECS` (5) it reads the new bytes of every transcript that belongs to a stage and publishes `.loom/work/tokens/flow.json` (a `TokenFlow`, `loom/src/tokens/model.rs`) and `checkpoint.json`, each only when a pass memo (`pass_memo.rs`, below) shows the file does not already hold it. The status collector reads `flow.json` into `StatusData.tokens`; the 1 s broadcast carries it to the TUI and the web dashboard. The tracker never runs inside the broadcaster, which only reads files (the quota-cache pattern). Source line formats and the Codex receipt lag are under Token Usage Sources above.

## Units

- **Bucket** `BUCKET_SECS` = 30; index = `floor(unix_secs / 30)`. Bucketing uses the timestamp on the line, never the read time, so a backlog read in one pass lands in its own past buckets; a line older than the series window adds to totals only.
- **Fresh tokens** = `input + cache_write + output` (`TokenUsage::fresh`). Every rate, series and headline is fresh unless labelled otherwise; cache reads appear only in the mix and the cache-hit percentage `cache_read / (cache_read + input + cache_write)` (undefined, shown `—`, at 0).
- **Rate** (`flow_build.rs` `rate`) = the sum of the last `RATE_BUCKETS` (10) buckets times 60 divided by their span in seconds, integer division per field. The span runs from the start of the oldest bucket to `now`, 271 to 300 s, so a rate does not drop when a new bucket starts. **Series** = fresh per bucket, oldest first: 120 values for a stage or agent (`STAGE_SERIES_LEN`), 60 for the plan (`PLAN_SERIES_LEN`, the per-bucket sum of each stage's last 60). A stage's **`hit_series`** is the cache-hit percent per bucket (`Option<u8>`, `None` for a bucket without input-side tokens), aligned with `series`.
- **`models`** (`TokenFlow.models`, `Vec<ModelUsage>`) holds plan-wide totals per (model, effort), largest fresh total first, at most `MAX_MODEL_ROWS` (24); past that the tail merges into one `other` entry placed last, so the entries sum to the plan total. Each agent books usage under its current (model, effort) in `by_model`, at most `MAX_MODEL_KEYS` (8) keys, overflow to `(other, None)`; a model that sanitizes to empty keys under `unknown`, an effort that does goes absent.
- **Active** agent: any of its last `ACTIVE_BUCKETS` (4) buckets is non-zero. Every UI reads `AgentFlow.active` verbatim; none derives it from `series`.
- Agent id: `main-` or `codex-` plus the first 8 characters of the session uuid or thread id; a subagent's id is its key. Agents of one stage that share a short form all get their full key (`flow_build.rs` `ordered`), so a UI never assumes 13-character ids.

## Tracker Pass

`scan_pass` (`tracker.rs::scan_pass`) runs the pass of `scan_until` and `run_pass`: `discover` (`discover.rs`: stage session `transcript_path`s and their `<stem>/subagents/` files, then Codex rollouts named by legacy receipts or validated authorization-to-companion-job correlation; historical sessions without Claude transcripts also supply stage identities), reads each source from its checkpointed offset (`tail.rs`), accumulates (`accumulate.rs`, `claude_line.rs`, `codex_line.rs`), prunes, builds the flow (`flow_build.rs`), takes quota time left (`quota_left.rs`, `quota_points.rs`), then saves the checkpoint and publishes `flow.json` through the memo.

- **Pass memo.** `PassMemo` (`pass_memo.rs`) lives in the tracker thread's `TrackerLocal` and is kept per work root, compared as the path was given: another spelling of the same directory only costs a fresh memo. It reuses nothing unless the file it stands for still carries the `FileStamp` (inode, length, mtime) the memo took, and a failed or over-cap write forgets what it would have replaced. It holds the checkpoint the last pass ended with (up to 8 MiB of JSON, held between passes), so an idle pass reads no `checkpoint.json`; `save_checkpoint` skips the write when the compact JSON hashes to that of the last save; `publish_flow` skips the write when the flow's JSON, with `observed_at` zeroed, hashes the same, the file is as the last write left it, and the last write is under `FLOW_HEARTBEAT_SECS` (60) old; `quota_left` is reused for `QUOTA_LEFT_REFRESH_SECS` (30) while the four quota files' stamps hold. The hashes are keyed per process (`HASH_KEYS`, a `RandomState`) because the hashed JSON derives from transcript text.
- **Claude dedupe.** One API response spans several lines sharing `message.id`; the per-file table keeps the latest whole observation by timestamp, using shared `provider_data::supersedes`. Equal timestamps retain the first observation. A replacement retracts the previous usage from totals, its time bucket and model/effort attribution before adding the new vector. Older observations are ignored; the bounded table keeps the newest 4096 ids per file.
- **Codex.** Deltas between successive cumulative `total_token_usage`; a decrease makes the new total the delta. `turn_context` sets the current model and effort. A thread whose rollout is not found is cached as a miss and retried at most every `MISS_RETRY_SECS` (600), except in the final pass (`tracker.rs::final_scan`, `LastLine::Count`), which drops cached misses first so a rollout that appeared late is counted. A native Codex worker on macOS writes its rollouts in its private root, so they reach the host sessions tree (and live totals) only when the released boundary's rollouts are moved ([macOS Boundary](codex-concurrency.md#macos-boundary-seatbelt)); the final scan runs after that.
- **Final scan and plan totals.** The daemon runs `final_scan` once every stage is done (`LastLine::Count`: an unterminated last transcript line counts only when it parses as a whole JSON value, and the offset advances past it). Before any checkpoint write it registers per-agent totals in a process-global map keyed by the exact work-root path (`tokens/plan_totals.rs::register`); `plan_tokens` prefers that map to the on-disk checkpoint, and `plan_stats` reads per-stage and per-(model, effort, lane) totals from it ([plan stats](plan-lifecycle-and-fields.md)). Lanes are `CLAUDE_LANE` and `CODEX_LANE`.
- **Model and effort.** A Claude line sets the agent's current model (the `<synthetic>` sentinel of an API-error row is ignored) and effort (`perTurnEffort`, else `effort`); usage books under the pair current when the line is read, in `by_model` (`add_model_usage`). `visible` (`accumulate.rs`) keeps a value only when `inline_safe` leaves text, so a blank one never becomes a label or a key.
- **Reading.** Whole lines only (the offset stays at the start of an unfinished line); a line over `MAX_LINE_BYTES` (32 MiB) is skipped without buffering and counted unreadable; files open `O_NOFOLLOW | O_NONBLOCK` and `stat` uses `symlink_metadata`; the shutdown flag is checked between lines. A file that shrinks or changes inode resets only that file's state; a file no longer discovered keeps its agent's totals.
- **Unreadable lines.** Invalid JSON that contains `"usage"` (Claude) or `"token_count"` (Codex), or a usage object lacking numeric `input_tokens`/`output_tokens`, counts in `unreadable_lines`. An absent or `null` cache counter is 0 in both parsers (`optional_count`, `model.rs`); a present non-numeric one is unreadable.
- **Checkpoint.** `CHECKPOINT_VERSION` is 4 (`tokens/checkpoint.rs`; whole message observations support replacement and retraction). `try_load` reads the version before the full parse, so an older file is refused as "checkpoint is version N", not blamed on a missing field, and every transcript is read from the start. `Checkpoint::save` refuses to write over `MAX_CHECKPOINT_BYTES` (8 MiB, what `load` reads) and returns `Saved::OverCap`; `save_shedding` then clears the message-id tables of agents not in `advanced` (their file did not change this pass) and measures again, and still over means nothing is written and the pass warning says so. `load_reporting` names why it fell back to an empty checkpoint (missing is silent). A checkpoint write failure still publishes `flow.json`, then fails the pass.
- **Flow file.** Compact JSON, at most `MAX_FLOW_BYTES` (512 KiB, a quarter of the daemon's 2 MiB response frame). Over it, the tracker empties the `series` of inactive agents (totals stay); still over, the write is skipped and logged once, the file ages out and the collector publishes no `tokens`. An inactive agent's series can be empty or short, so UIs left-pad.
- **Shutdown and errors.** `scan_pass` has a pinned signature without a stop flag, so a thread-local (`TrackerLocal`, `tracker.rs::TrackerLocal`) carries the flag and the memo in and the pass warning out; `guarded_pass` runs the pass under `catch_unwind` and `log_change` logs a given warning once. `ensure_tokens_dir` refuses a symlinked `tokens` directory.

## Collector Boundary

`status/data/collector.rs` keeps the flow only when `now - observed_at` is within `0..=FLOW_STALE_SECS` (600; a future-dated flow is never fresh) and runs `sanitize_token_flow` (`status/data/sanitize_token_flow.rs`): strings go through `inline_safe`, every number is clamped to 2^53 - 1 (the web schema rejects a larger integer and then drops the whole frame), series keep only their newest 120 or 60 values (a stage's `hit_series` also clamps each value to 100), `models` keeps at most `MAX_MODEL_ROWS`, and an empty effort becomes absent. `bucket_secs` is clamped but not required positive, so the web rate helper guards a zero itself. `read_flow` opens through `crate::fs::safe_read::read_bounded`, so a FIFO or symlink never blocks the 1 s broadcaster. Callers name `read_flow` and `spawn_token_tracker` through their defining modules, because the source graph adds no call edge through the `crate::tokens::` re-export and `reachable` checks walk those edges.

## Quota Time Left

`quota_left::estimate(points, resets_at, now)` shows a duration, never a clock time ("~2h07m left", "lasts to reset", or nothing), and only for a provider that reports a five-hour window (Codex normally shows nothing).

1. `quota_points.rs` builds the points from `quota::read_history(work_root, provider, now - 1800, ...)`. It ignores `HistoryContinuity` and segments on the five-hour window alone: a point starts a new period when its `resets_at` differs from the previous point's by more than 300 s or its `used_percent` is lower (only the percent rule when either `resets_at` is null). The history flips `resets_at` by 1 s between polls, which `continuity_from_windows` marks `Reset`, so trusting it leaves one point.
2. The history store skips a row identical to the last, so the current snapshot is appended as the newest point when it was observed after the newest row.
3. Fewer than 2 points or a span under 360 s gives nothing; slope = `(last_used - first_used) / ((last_t - first_t) / 60.0)` in that f64 order; slope 0 or less gives `lasts_to_reset`; else `minutes_left = max(0, (100 - last_used) / slope - (now - last_t) / 60)` rounded down, and `lasts_to_reset` is set when `now + minutes_left * 60 >= resets_at`.

The 30-minute window exists because the poller samples every 180 s.

## Surfaces

- **Web.** `status.tokens` is optional in the strict schema, and a frame whose only defects sit under `tokens` drops that block alone (`web/src/api/schema.ts::statusDataSchema`). `TokenLine` (`header-lines.tsx`) is the header TOKENS rail: two cells in one row (plan total and cache hit of the plan total, the only cache-hit figure on the overview) and no sparkline. `TokenFlowPanel` (`token-flow-panel.tsx`) is the Token flow panel on `/` and `/ledger`: the fresh/min headline, per-stage fresh bands and a mix row whose "uncached input" entry carries a hint (a `title`, so absent on touch), with no cache hit. Under the bands `TokenModels` (`token-models.tsx`) draws the **By model** block: a stacked time-series bar chart, then from `flow.models` a pie of fresh tokens per model (efforts summed), a legend with whole-percent shares (`wholePercents`, largest remainder, summing to 100) and a rate from `modelRates`, and a totals table per model and effort. The bar chart (`model-history-chart.tsx`, `lib/model-history.ts` `modelHistoryBars`) has sixty bars, one per 30 s bucket ending at the bucket of `observed_at`; each bar is the fresh rate per minute over the ten buckets ending at it (`RATE_BUCKETS`, the backend's five-minute window), from every agent's `series` summed under its current model, so an agent that switched models books its whole series under the newest one. The newest window ends at `observed_at` with the backend `rate` span (271 to 300 s), so the newest bar stacks to the headline and the legend rates ([mistake](../mistakes/computed-values-and-hidden-couplings.md#web-by-model-bars-drew-single-bucket-rates-under-a-five-minute-headline)). The y-axis ceiling is the window's peak rounded up to 1, 2 or 5 times a power of ten, recomputed every frame. The bar chart, pie and legend share `modelColors`, which calls `stickyModelColors` (`web/src/lib/model-color.ts`). A model's preferred color is a hash of its whole name, version included, giving an OKLCH hue and one of three lightness steps at fixed chroma. A new model (new models in name order) keeps that color when its OKLab distance to every color already given out is at least `TARGET_GAP`; otherwise it takes the grid candidate (5-degree hue steps by three lightness steps) with the largest capped gap, ties going to the candidate nearest the preferred color, so a full palette degrades to farthest-point placement. Assigned swatches are remembered per page and in localStorage (`loom.model-colors.v2`), so a model never changes color when another appears. Colors are independent of token-share order. Models absent from `modelColors` (folded into `other` by the backend) stack last in `--tone-dimmed`. A 1.5-unit gap separates stacked segments, and gaps separate pie slices. Efforts remain summed into one slice per model. Bands use `--series-1..6` (`index.css`), never a state tone; `stageColors` gives each stage the slot of its place among all plan stage ids in id order, so a stage keeps its colour while others start or finish ([mistake](../mistakes/web-dashboard-server.md#stage-bands-wore-state-tones-by-rank)). The ledger rows carry a TOKENS cell, graph cards a `NodeTokens` trace (`graph/node-tokens.tsx`), and the stage dialog a Tokens section (`stage-tokens.tsx`, mounted in `stage-sections.tsx`) whose four chips (fresh rate, fresh so far, cache hit per 30 s, agents live) each carry a 30-minute trend (`TREND_POINTS` 60: `rollingRate`, `cumulativeSeries`, `carryForward` over `hit_series`, `liveCounts`). `QuotaMeters` takes `quotaLeft` for the label beside the Claude 5h meter. Helpers are in `web/src/lib/tokens.ts` (`fresh`, `lastValues`, `maxOf`, `compactTokens`, `quotaLeftText`, `modelGroups`, `modelShares`, `wholePercents`, `rollingRate`).
- **TUI.** Header token summary (`header.rs`, the first thing to give way), TOKENS column, time-left suffix in the quota line (`quota.rs`, FULL and MEDIUM tiers only) and the `t` pane (`tokens_pane.rs`): live agent rows, a blank line, then the by-model block (`tokens_models.rs` `model_lines`: a title, one row per model with a share bar, one dim row per effort, cut at `MODEL_BLOCK_MAX_ROWS` = 10); layout in [status-data-model.md](status-data-model.md#what-the-live-tui-renders-today). Pane agent states are live or idle; the title row notes "no active agent" or "no token data yet". Model shares round half up independently in the TUI, while the web legend uses largest remainder, so the same flow can print different percentages.
- **Back to details.** The terminal and history views of the stage dialog carry a `BackButton` (`terminal/terminal-view.tsx`) that returns to the stage details; see [web-terminal.md](web-terminal.md#keystroke-wire-contract-and-the-esc-rule).
- An executing stage shows its rate; every other stage with a flow shows its total dimmed in both UIs. A stage absent from `stages` shows `—` or blank, never 0.
- **Lower bound.** Numbers come from local transcripts: compaction and auxiliary calls may be missing, and sessions under another `CLAUDE_CONFIG_DIR` or `CODEX_HOME` may be invisible. Known gaps: [concerns/token-accounting-and-proof-defects.md](../concerns/token-accounting-and-proof-defects.md#token-flow-gaps).

## Planner Token Pass

The plan chat's background list shows each background agent's model, fresh total and rate from the same reader, flow builder and checkpoint the daemon uses for stages (`loom/src/tokens/planner.rs`). `scan_pass`'s helpers take their sources and checkpoint directory as arguments; `run_pass` keeps the daemon behaviour (`discover(work_root)`, `flow.json`, `quota_left`).

- **Entry.** `planner_pass(tokens_dir, groups, now) -> Result<GroupFlows>` (`GroupFlows` = `Vec<(group id, Vec<AgentFlow>)>`) reads exactly the files the `PlannerGroup`s name, keeps offsets in `<tokens_dir>/checkpoint.json`, writes no `flow.json` and computes no quota. A group the caller stops passing is dropped from the checkpoint, so a finished chat session does not stay in it; the `TRACKER` pass memo is keyed by the tokens directory. It fails only as `run_pass` does, when the checkpoint cannot be written.
- **Chat-session group.** `PlannerGroup { id, main, subagents: Vec<(agent id, transcript path, agent type)> }` is built from `Bridge::chat_session_id()` and `Bridge::transcripts(id)`; only the chat session gets one, because only its table is published. `agent_type` comes from each subagent's `agent-<id>.meta.json` `agentType`, read size-bounded (`plan_server/chat/background/groups.rs`, one bounded meta read per subagent per pass); teammates (`taskKind: in_process_teammate`) are skipped; there are no Codex sources. A subagent's `AgentFlow.id` is its full agent id (`short_id` shortens only main and Codex ids), which is the join key to the bridge table; the main agent's row has no table entry and is dropped.
- **Thread and checkpoint.** `spawn_planner_tokens(tokens_dir, groups, sink, stop)` runs a named `planner-tokens` thread every `TRACKER_INTERVAL_SECS` (5), started by `plan_server/chat/background/mod.rs` through `ctx.own_worker` so shutdown sets the stop flag and joins it. The checkpoint directory is `<RuntimePaths.dir>/tokens/` under the standard `checkpoint.json`, keyed by the plan chat session id in its `stages` map, with the same 8 MiB cap. A pass that read a source from offset 0 cannot double-count, because message-id dedup makes a re-read idempotent: offsets are a cost property, and no test fails when they are ignored (only losing both the offsets and the dedup table turns the contract red).
- **Wire unchanged.** `StageFlow`, `stage_id`, `TokenFlow`, `flow.json` and the checkpoint format keep their names, so the dashboard schema and fixtures needed no change; the private `Source.stage_id` is the group id inside `tokens/`. The planner reads the rows through `PlanMessage::Background`, not the dashboard wire ([the table and the join](planning-session-bridge.md#transport)).

## Sources

- **Claude**: `GET https://api.anthropic.com/api/oauth/usage` with `Authorization: Bearer <token>` and `anthropic-beta: oauth-2025-04-20` (`loom/src/quota/claude.rs::fetch` `fetch`). `parse_response` (`quota/claude.rs::parse_response`) accepts two shapes: `five_hour`/`seven_day` buckets with `utilization` 0-100 and an RFC 3339 `resets_at`, or a `limits` array whose `session`/`weekly_all` kinds map to the same two windows (a `percent: 0` limit with null `resets_at` is a placeholder and is ignored; buckets win when both shapes are present). A 429 becomes the typed `RateLimited { retry_after_secs }` error.
- **Codex**: one bounded `codex app-server` stdio exchange per poll (`loom/src/quota/codex.rs::poll_once`): `initialize` (id 0), the `initialized` notification, then `account/rateLimits/read` (id 1). `rateLimits.primary`/`secondary` are classified by `windowDurationMins` (300 = five-hour, 10080 = seven-day); `planType` is kept as `plan`. A provider may report only the weekly window; absent windows remain unknown rather than being inferred as zero usage.
- **Token lookup**: `credentials::access_token(home)` (`loom/src/quota/credentials.rs::access_token`) reads `.claudeAiOauth.accessToken` from the macOS keychain item `Claude Code-credentials` (bounded by `run_bounded`) or from `~/.claude/.credentials.json`; no login means no claude file, silently. The codex binary comes from `codex::find_codex_path()`; absent means no codex file.

Claude plan metadata comes from subscriptionType in the same OAuth credential source as the access token (credentials::account), and is attached to successful shared quota polls. The web tooltip explains when the provider does not report a window.

## Cadence and backoff

`quota::poller::spawn_quota_poller` (`loom/src/quota/poller.rs::spawn_quota_poller`, wrapped by `daemon/server/broadcast.rs::spawn_quota_poller` and joined in `lifecycle.rs` next to the status broadcaster) runs one thread for both providers. Each polls immediately at daemon start, then every 180 s (`POLL_INTERVAL`); a failure doubles the interval up to 15 min (`MAX_BACKOFF`); a Claude 429 waits `Retry-After` with a 300 s floor. Failures log once per distinct message and once on recovery, through the daemon's stderr log. The loop sleeps in 250 ms slices and checks `shutdown_flag` between polls; a Claude request in flight can hold the thread up to the 10 s HTTP timeout, a Codex exchange checks cancellation on bounded waits and retires its owned process group before a deadline-bounded reader completion wait. The daemon also bounds its shutdown join. `HOST_ENV_ALLOWLIST` (`daemon/server/environment.rs::HOST_ENV_ALLOWLIST`) forwards `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY` so the poll honours a proxy from the scrubbed daemon environment.

Claude is polled once per account across every daemon on the machine (`loom/src/quota/shared.rs`). The usage endpoint rate-limits per token, and every daemon (one per project), every Claude Code session, and statusline tools such as ccstatusline share that token, so 429s are routine. `poll_shared` (`shared.rs::poll_shared`) takes an exclusive flock on `~/.loom/quota/claude-<key>.lock` (`<key>` = first 16 hex of SHA-256 of the token, so separate accounts get separate files and the token is never written), reads `claude-<key>.json` (`AccountState`: last good `quota`, `next_poll_at`, `interval_secs`, `consecutive_rate_limits`, `last_error`), fetches only when `next_poll_at` has passed, and otherwise adopts the state another daemon left. `poll_claude` (`poller.rs::poll_claude`) then projects that state into the project cache (`project_account`, `poller.rs::project_account`) and wakes on the account's `next_poll_at`. `display_error` (`shared.rs::display_error`) hides a 429 streak until `RATE_LIMIT_CONFIRMATIONS` (3) in a row; until then the cache keeps the last good reading with `error: None`. Other errors show at once. Stale JSON data for other keys is pruned after 24 hours; lock files persist to preserve their synchronization identity for holders and waiters. Codex polls per daemon.

## Cache files

`.loom/work/quota/claude.json` and `codex.json` (`loom/src/quota/cache.rs`), one pretty-printed `ProviderQuota` each: `observed_at` (epoch seconds of the last success), `windows` (five-hour first), `plan`, `error`. Written temp-file-then-rename with fsync into a 0700 directory; the writer refuses a symlink at the target and unlinks then `create_new`s the `.tmp` sibling so a planted link is never followed. Readers cap at 64 KiB, treat oversize or malformed content as absent, never delete, and re-run the hygiene (`clamp_percent`, one window per kind, `inline_safe` on `plan`/`error`). A failed poll rewrites the existing file with `error` set and the previous `observed_at`/`windows`/`plan` kept (for Claude, an unconfirmed 429 leaves `error` unset; see Cadence and backoff); with no existing file it writes nothing, so a provider that never succeeded has no row at all. The collector, the web file-poll fallback, and tests read only these work-dir files; the daemon's Claude poller alone also reads and writes the account-shared state in `~/.loom/quota/`.

## Rules that must not regress

- The OAuth token exists only inside `credentials::access_token` and the single request in `claude::fetch`; it is never logged, cached, put in an error string, or sent over the daemon socket. HTTP failures carry the status code only.
- Every string from a network body or the codex subprocess (`plan`, `error`, JSON-RPC `message`) goes through `context::untrusted::inline_safe` before storage and again on read.
- Percentages are clamped 0..=100 and non-finite values dropped; unparsable reset times become `None` instead of failing the window; epoch values above 100 000 000 000 are treated as milliseconds (`normalize_epoch`).
- Codex exchange uses `CODEX_DEADLINE` (15 s). `quota/codex.rs::teardown` retires the shared `OwnedProcess` even when the direct child has exited, then waits for reader completion only until the retirement deadline. Dropping the message receiver releases a blocked sender; process-group retirement closes descendant-held pipes.

## Renderers

- **TUI** (`loom status --live`): `ledger/quota.rs::quota_layout` `quota_line` draws one line above the legend line when either provider has data. `layout.rs` gives the footer 2 rows through `footer_height(has_quota)` and drops back to 1 as a fourth degrade step when the table would otherwise fall under 6 rows. `quota_layout(width)` picks the tier: 8-segment bars plus countdown from 120 columns, 6 segments from 90 (`MEDIUM_WIDTH`), percent only below; `quota_line` then falls back to plainer tiers before ever truncating a meter. Stale readings (`age_secs >= STALE_AFTER_SECS`, 600 s) and `error` append dimmed suffixes. Both window slots are always drawn, a missing one as a dimmed `—`. The same change made the `↑↓ scroll` hint conditional on `LedgerView.scrollable` (the table overflows its viewport).
- **Web** (`loom status --web`): `web/src/components/quota-meters.tsx` inside the sticky footer of `web/src/routes/shell.tsx`, fed by `web/src/lib/quota.ts` (mirrors `model.rs`: `quotaHealth`, `formatReset`, `resetText`, `ageText`, `providerRows`). Each window is a notched gauge over a time-to-reset track, `role="img"` with an aria-label and a tooltip carrying the exact reset time and observed age; stale dims the group, error appends the text in the warning tone. The available container width controls layout in quota-meters.css: reset and time-left details hide at 1100px, provider groups share aligned label/5h/7d columns at 680px, and labels sit above each meter pair at 400px. On small viewports the version occupies a separate footer row. The help illustration renders the same production QuotaMeters.

## History consumers

`.loom/work/quota/history/<provider>.jsonl` (`loom/src/quota/history.rs`, `history_store.rs`) is read by the token tracker's time-left estimate (`loom/src/tokens/quota_points.rs`) through `quota::read_history` over a 30-minute window; the snapshot cache above supplies the newest point when the store skipped an identical row. The five-hour `resets_at` flips between two values one second apart on successive polls. `quota::RESET_TOLERANCE_SECS` (300, `quota/mod.rs::RESET_TOLERANCE_SECS`) absorbs that jitter in three places: `continuity_from_windows` marks `Reset` only when `resets_at` differs by more than the tolerance, the token tracker's own segmentation (`tokens/quota_points.rs`, which ignores `HistoryContinuity`, since it flags seven-day window changes as resets) splits on the same constant plus the `used_percent` drop rule (details: [Quota Time Left](#quota-time-left)), and the offline comparison's quota checks (`commands/usage/comparison_quota.rs`) treat `resets_at` values within it as one reset identity. The tracker thread and the 1 s collector both read quota snapshots through `quota::cache::read_provider`. Both snapshot and history readers use `fs::safe_read::read_to_string_bounded` (`loom/src/quota/cache.rs::read_provider`, `loom/src/quota/history_store.rs::read_history_body`), rejecting FIFO, symlink, and other nonregular files instead of opening them with an unbounded blocking read.

## Model Budget consumers

The snapshot cache also feeds routing text, not only meters: `orchestrator/signals/format/budget.rs::current_model_budget` reads `quota::read_snapshot`, drops a provider whose reading is older than `STALE_AFTER_SECS` (600 s), carries an `error` or has no windows, and renders the `## Model Budget` signal section that `loom usage --quota` also prints. It reads the snapshot only, never the history store. Pressure rules and routing text: [signal-generation.md](signal-generation.md#model-budget-section-formatbudgetrs).
