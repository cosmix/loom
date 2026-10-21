# Status Data Model

> Source of each status field

## Sources of Truth

Static and compact `loom status` read `StatusData`, built by `collect_status_data` (`loom/src/commands/status/data/collector.rs`). `loom status --live` does not poll it: the TUI subscribes to the daemon's push channel over the Unix socket (`SubscribeStatus`, `loom/src/commands/status/ui/tui/daemon_client.rs:15-71`; daemon side registers the subscriber in `status_subscribers`, `loom/src/daemon/server/core.rs:62`). The pushed value is a `StatusData` built by the same collector, so static, compact and live all read one model; only the transport differs (file poll vs socket push). The TUI event loop separately polls terminal input every 100ms (`POLL_TIMEOUT`, `loom/src/commands/status/ui/tui/app.rs:38`) and re-reads scheduler alerts from files at most once a second (`ALERT_REFRESH_INTERVAL`, `app.rs:56`; `refresh_alerts`, `app.rs:184-195`); the same throttle also refreshes `TuiApp::tick_age_secs` from `orchestrator::tick::read`.

Stage files `.loom/work/stages/<id>.md` and session files `.loom/work/sessions/<id>.md` are the `Stage`/`Session` structs serialized directly as frontmatter — there is no reduced on-disk schema for status (`list_all_stages` in `loom/src/verify/transitions/persistence.rs` parses each stage file with `parse_stage_from_markdown`; `load_session_from_file` in `collector.rs` parses each session file with `parse_from_markdown`).

## CLI Flags and Dispatch

Exactly three bool flags exist; there is no `--json`, `--watch`, or refresh-interval flag.

| Flag | Short | Type |
| --- | --- | --- |
| `--live` | `-l` | bool |
| `--compact` | `-c` | bool |
| `--verbose` | `-v` | bool |

Defined in `loom/src/cli/types.rs:81-93`; dispatched in `loom/src/commands/status.rs:148-188`.

## StageStatus — 13 Variants

`StageStatus` has 13 variants (`loom/src/models/stage/types.rs:853-917`). Icon, label, color, and bold come from `types.rs:986-1096`; the progress bucket from `types.rs:1108-1120`.

| Variant | Icon | Label | Color | Bold | Bucket | Needs a human |
| --- | --- | --- | --- | --- | --- | --- |
| Completed | `✓` U+2713 | Completed | Green | yes | Completed | no |
| Executing | `●` U+25CF | Executing | Blue | yes | Executing | no |
| Queued | `▶` U+25B6 | Queued | Cyan | yes | Pending | no |
| WaitingForDeps | `○` U+25CB | Waiting | Gray | no | Pending | no |
| WaitingForInput | `?` | Input | Magenta | yes | Executing | yes |
| Blocked | `✗` U+2717 | Blocked | Red | yes | Blocked | yes |
| NeedsHandoff | `⟳` U+27F3 | Handoff | Yellow | yes | Executing | no |
| Skipped | `⊘` U+2298 | Skipped | DarkGray | no | Completed (excluded from the completed count by `calculate_progress` in `collector.rs`) | no |
| MergeConflict | `⚡` U+26A1 | Conflict | Yellow | yes | Blocked | yes |
| CompletedWithFailures | `⚠` U+26A0 | Failed | Red | yes | Blocked | yes, but auto-retried up to `max_retries` |
| MergeBlocked | `⊗` U+2297 | MergeBlk | Red | yes | Blocked | yes |
| NeedsHumanReview | `⏸` U+23F8 | Review | Magenta | no | Blocked | yes (`review_reason` set) |
| NeedsAdjudication | `⚖` U+2696 | Adjudicate | Yellow | yes | Blocked | yes |

Meanings worth remembering: `WaitingForDeps` means dependencies are not yet both completed and merged; `Queued` means ready to spawn; `NeedsHandoff` means the context ceiling was hit and a fresh session resumes the stage; `Skipped` is terminal and does not satisfy dependents; `CompletedWithFailures` and `MergeBlocked` are retryable, not terminal. `ProgressSummary` counts (`data/mod.rs`) are computed only through `StageStatus::bucket()` in `calculate_progress` (`collector.rs`), so a caller that wants raw variant counts must not reuse `ProgressSummary`.

Related enums on `Stage`: `StageType` (`types.rs:14-29`) is Standard, Knowledge, IntegrationVerify, or KnowledgeDistill, with `default_model()` at `types.rs:46-60`. `implementers` (`types.rs:139-239`) is an ordered list of `Implementer::Claude` / `Implementer::Codex` lanes. `ExecutionMode` (`types.rs:75-81`) is Single or Team.

## Stage Fields Worth a Dashboard

`Stage` (`loom/src/models/stage/types.rs:634-831`) carries far more than status. Fields grouped by purpose:

- **Identity/graph**: id, name, description, status, dependencies, stage_type, worktree, session, held.
- **Timing**: created_at/updated_at/completed_at, started_at (first transition to Executing, `types.rs:658-661`), duration_secs (final elapsed time), execution_secs (accumulated active time, excludes backoff, `types.rs:666-670`), attempt_started_at (`types.rs:671-675`).
- **Retry/failure**: retry_count (`types.rs:685`), max_retries (`types.rs:688`, `None` means the global default of 3), last_failure_at (`types.rs:690`), failure_info: `Option<FailureInfo>` with failure_type, detected_at, evidence (`loom/src/models/failure.rs:54-63`).
- **Merge**: base_branch, base_merged_from, completed_commit, cleanup_warning (`types.rs:712-718`), merged, merge_assumed, merge_conflict, verification_status.
- **Context**: context_ceiling_tokens.
- **Adjudication/verification**: fix_attempts, max_fix_attempts, dispute_count (capped at 3, `methods.rs:11`), evidence_rounds, amendments_applied, stall_recoveries (automatic stall recoveries spent; reset by `human-review --approve`, `stage reset` and `stage retry`), review_reason.
- **Execution policy**: model (override), reasoning_effort, implementers, subagent_timeout_secs, files, auto_merge, outputs.

Helper accessors: `effective_model()` (`methods.rs:131-135`), `effective_subagent_timeout_secs()` (`methods.rs:153-156`), `get_effective_max_fix_attempts()` (`methods.rs:476-483`).

There are five distinct counters on `Stage` — retry_count, fix_attempts, dispute_count, evidence_rounds, stall_recoveries — that track different loops. Name which one is meant; they are easy to conflate.

## Session Fields

`Session` (`loom/src/models/session/types.rs:98-139`): id, stage_id, worktree_path, pid, status, context_tokens, transcript_path, created_at, last_active, session_type, merge_source_branch/merge_target_branch, tracking_key (`types.rs:128-133`), backend.

`SessionStatus` (`types.rs:38-49`): Spawning, Running, Paused, Completed, Crashed, ContextExhausted.

`SessionType` (`types.rs:7-28`): Stage, Merge, BaseConflict, Knowledge, Adjudication, Contract (the contract-writer phase's session kind, tracking key `loom-contract-{stage_id}`).

`context_tokens: u32` is the resident token count as of the last heartbeat that carried a reading (`types.rs:104-108`); a heartbeat tick with no measurement leaves the previous value in place.

`backend: SessionBackendKind` (`types.rs:68-76,138`) is Native or Tmux.

Two things that do NOT exist on `Session`: no `model` field (model is stage-level, via `Stage.model`/`effective_model()`), and nothing that records which `Implementer` lane the session runs (that lives on `Stage.implementers`).

## Heartbeat and Activity

`Heartbeat` (`loom/src/orchestrator/monitor/heartbeat.rs`): stage_id, session_id, timestamp, progress_at and activity_kind (`ActivityKind::Progress` or `Observation`, both optional), context_tokens: `Option<u32>` (`None` means not measured this tick, so the previous value is kept), transcript_path, last_tool, activity, subagent (written by a subagent's tool call rather than the stage's main agent). `Heartbeat::effective_progress_at()` is `progress_at` when present and not later than `timestamp`, else `timestamp`.

The file `.loom/work/heartbeat/<stage_id>.json` is written by the PostToolUse hook (`loom_post_tool_write_heartbeat` in `loom-hooks/_post-tool-heartbeat.sh`), not by Rust code; an adjudication session writes `<stage_id>.adjudication.json` instead. The Rust side reads and manages the files in `loom/src/orchestrator/monitor/heartbeat_store.rs` (`read_heartbeat`, `heartbeat_path`, `judge_heartbeat_path`, `remove_heartbeat`); its `write_heartbeat` is called only from tests. `loom status` reads the stage file through `read_heartbeat_for_stage` in `loom/src/commands/status/data/heartbeat_facts.rs`, which validates the stage id before joining it into the path.

Staleness is seconds since `effective_progress_at()`, so observation-only polling does not refresh it (`heartbeat_facts` in `heartbeat_facts.rs`, using `age_secs` from `orchestrator/monitor/progress`). The heartbeat only moves when a tool call returns, so silence alone cannot tell a hung session from one inside a long tool call or a long generation.

`ActivityStatus` (`loom/src/commands/status/data/mod.rs`): Idle, Working, Busy, Quiet, Stale, Error (session crashed), Orphaned (stage Executing but no session record). Computed by `determine_activity_status` in `heartbeat_facts.rs`. A finished stage (Completed/Skipped) and a session that ended without crashing both report `Idle`, regardless of heartbeat age. For a running session, with `stale_after` = the stage's `effective_subagent_timeout_secs()` (a 0 budget falls back to `STALENESS_THRESHOLD_SECS`, 300, `models/constants.rs`):

- silence ≤ `min(300, stale_after)`: `Working`, and no file is read;
- past that, an open `tool_use` with no `tool_result` in the main transcript (`Heartbeat.transcript_path`) or a subagent transcript (`<session-uuid>/subagents/agent-*.jsonl`, skipped when last written before the last progress): `Busy`, with `StageSummary.last_tool` replaced by the open tool's name and rendered `running <tool> <dur>`;
- else silence > `stale_after`: `Stale`, the same budget the orchestrator's hung detection uses;
- else `Quiet`, rendered neutral (`quiet <dur>`), no attention entry.

The probe is `commands/status/data/busy.rs`; it reuses `commands/subagents::open_tool_call` (a 512 KiB tail read in `classify/entry.rs`, so a final `tool_use` larger than that reads as closed). `transcript_path` is session-written, so it must be absolute `.jsonl` with no `..`, under the canonical Claude projects root, and is opened through `open_regular_no_follow`; any failure means nothing is open. A main agent blocked on a foreground Agent call has an open `tool_use`, so it reads `Busy` even when that subagent is hung; the monitor's hung detection does not consult transcripts and still acts on silence.

PID liveness comes from `crate::process::is_process_alive`, called in `session_facts` (`loom/src/commands/status/data/collector.rs`) to fill `session_alive`, recomputed on every collection and never persisted.

A context reading is shown only when `context_tokens > 0` and the session is not terminal: `reported_reading()` in `collector.rs`. The ceiling shown alongside it comes from `resolve_context_ceiling_tokens` (`loom/src/fs/work_dir/config_sections.rs`), resolution order: stage override, then workspace `[context]` config, then `~/.loom/config.toml`, then `DEFAULT_CONTEXT_CEILING_TOKENS`. A workspace `[context]` section that cannot be read still falls through to the user config tier.

## Stall Parking and the Desktop Notifier

A stage session that stops answering ends in `NeedsHumanReview`, not in a status of its own:
recovery gives up after `MAX_STALL_RECOVERIES` (2) automatic re-queues, and a session that never
worked (no tool call, no context tokens) is parked at its first report with a login remedy when
`claude auth status` says `NotLoggedIn`. The detection and action rules are in
[Soft Signals](signal-generation.md#soft-signals). The park sets `review_reason` (what `loom status`,
the TUI and the web view's `review_notes` show) and the short status reason `"stall park"`, so every
view reaches it through the NEEDS REVIEW attention entry below. There is no STALLED marker and no
`Stage` field for one.

`orchestrator/notify.rs` sends the desktop notification for a needs-human-review transition
(`notify_needs_human_review`, called from `announce_needs_human_review`). `send_desktop_notification`
is best-effort and never blocks the tick: the notifier (`notify-send` on Linux with the body's
`& < >` escaped, `osascript` on macOS) runs bounded (2 s and 3 s) on its own `loom-notify` thread,
a failure is logged and dropped, and a `cfg(test)` build sends nothing, so test runs never pop
notifications. The body carries only the reason's `review_headline` cut to 200 characters; the full
reason, pane tail included, stays on the stage record.

## Payload Shapes

**`StageSummary`** (`commands/status/data/mod.rs`, built by `build_stage_summary` in `collector.rs`; static/compact/live view — all three read the same struct): id, name, description, summary, status, stage_type, dependencies, context_tokens, elapsed_secs (started_at to completed_at, or to now while open; None until started), execution_secs, base_branch, base_merged_from, failure_info, activity_status, last_tool, last_activity, staleness_secs, context_ceiling_tokens, review_reason, review_notes (a copy of `review_reason`), merged, merge_assumed, cleanup_warning, held, retry_count, max_retries, pid, session_alive, model, session_type, incoherence, execution_models, dispute_count, judge_heartbeat_secs, session_backend, outgoing_session_exit_reason, completion_blocker, merge_resolver_session, merge_resolver_attempts, close_reason, merge_block (the block sentence while the stage is `MergeBlocked`) and stash_warning (every existing unrestored backup ref, comma-separated). `commands/status/data/merge_note.rs` fills the last two: `stash_warning` costs one `git rev-parse` per listed backup ref and only reads refs under `refs/loom/autostash/`. The web mirrors them in `schema.ts`, `attention-panel.tsx` and `format.ts`; the TUI renders them in `cells.rs` and `panels.rs`.

`execution_models: Vec<String>` is distinct execution-model display names observed for the stage's subagents, first-seen order (spawn ledger then codex ledger), empty until a subagent spawns — see "Execution-Model Ledgers" below. `dispute_count` and `judge_heartbeat_secs` surface adjudication state. `session_backend: Option<SessionBackendKind>` (Native/Tmux) is populated by `build_stage_summary`; the only reader is the web stage page (`web/src/routes/stage.tsx`), and no Rust renderer reads it — see concerns.md.

**`StatusData`** (`commands/status/data/mod.rs`): stages, merge (`MergeSummary`: merged, pending, conflicts), progress (`ProgressSummary`: total, completed, executing, pending, blocked), plan_name (the first H1 heading of the plan file with a leading `PLAN:` stripped, from `load_plan_name` in `collector.rs` via `plan::parser::extract_plan_name`; `None` when the config, plan file or heading is missing), and quota (`QuotaSnapshot` in `loom/src/quota/model.rs`: `claude` and `codex`, each `Option<ProviderQuota>` carrying `observed_at`, up to two `windows` (five-hour first; `used_percent` 0..=100; `resets_at` epoch seconds or `None`), `plan`, `error`). The collector fills it from the on-disk cache through `quota::read_snapshot(work_dir.root())` in `collect_status_data` (`collector.rs`), never by polling. The field is `#[serde(default)]` so an older daemon's frame still parses, but it is always serialized, so the web schema (`quotaSnapshotSchema` in `web/src/api/schema.ts`) requires it. Who writes the cache: [quota-poller.md](quota-poller.md). `tokens` (`Option<TokenFlow>`, `loom/src/tokens/model.rs`) is the live token flow: the collector reads `.loom/work/tokens/flow.json` through `tokens::model::read_flow`, keeps it only while `observed_at` is at most `FLOW_STALE_SECS` (600) old and not in the future, and runs `sanitize_token_flow` (`commands/status/data/sanitize_token_flow.rs`) on it; with no fresh flow the key is absent (`skip_serializing_if`), and the web schema marks it optional. Who writes the file: [token-flow-sources.md](token-flow-sources.md).

**Live-mode daemon push**: `Response::StatusUpdate` (`loom/src/daemon/protocol.rs:257-259`) carries one field, `data: Box<StatusData>` — boxed because the quota field pushed the variant past clippy's large-enum threshold; serde serializes the box transparently. It is the identical model the static and compact views build. `collect_status` (`loom/src/daemon/server/status.rs`) calls the same collector, so every `StageSummary` field reaches the live TUI; there is no narrower live payload and no per-bucket split.

`Response::OrchestrationComplete` carries a completion summary (`protocol.rs:36-48`; per-stage completion info, `protocol.rs:10-30`). `DaemonConfig` (`protocol.rs:60-69`: manual_mode, max_parallel, watch_mode, auto_merge) exists on the wire but nothing under `commands/status` reads it.

## Daemon Liveness and Loop Health

`DaemonStatus` (`loom/src/daemon/server/core.rs:16-36`): NotRunning, Running, ProcessOnly (process alive, socket unreachable), Unreachable (a flock proves a daemon owns the work dir, but the current sandbox denies the connect attempt — rendered as healthy).

`daemon_status_line` (`status.rs:102-145`) maps that status plus loop-stall detection to a marker, message, and hint.

Loop liveness is tracked by a single-slot tick file, `.loom/work/orchestrator.tick` (`loom/src/orchestrator/tick.rs`), stamped per phase (`tick.rs:38-47`) and overwritten each tick. It is considered stalled at 60 seconds or more (`STALL_THRESHOLD_SECS`, `tick.rs:33`), and surfaced through `scheduling_report::alerts` (`status.rs:228-235`). The orchestrator's poll interval is 5 seconds (`loom/src/orchestrator/core/orchestrator.rs:64`).

## Merge State

`MergeState` (`loom/src/git/merge/status.rs:14-26`): Merged, Pending, Conflict, BranchMissing, Unknown.

`check_merge_state` (`status.rs:62-110`) checks git ancestry first (`completed_commit` against the merge point), falling back to `Stage.merge_conflict` only when ancestry cannot be determined.

`MergeStatusReport` (`status.rs:113-123`): merged, pending, conflicts, warnings. `commands/status/merge_status.rs` only re-exports this.

## Retry and Backoff

`loom/src/orchestrator/retry.rs` holds pure logic with no persisted state: `should_auto_retry` (`retry.rs:12`), `calculate_backoff` (exponential, base times 2 to the power of attempt minus one, capped, `retry.rs:35-47`), `is_backoff_elapsed` (`retry.rs:55`), `classify_failure` (`retry.rs:100`).

There is no persisted next-retry timestamp on disk; the backoff window is recomputed each time from `last_failure_at` plus the calculated backoff.

## Attention Rendering (Static View)

Despite the heading (kept because `loom knowledge` cannot rename a section — see concerns.md), this section is shared by every view. `attention_entries(stages: &[StageSummary]) -> Vec<AttentionEntry>` (`commands/status/render/attention_model.rs:120`) is the single place a stage is judged to need a human. `render/attention.rs` prints it for the static blocks, `TuiApp::render` feeds it to the ledger's attention panel (`commands/status/ui/tui/ledger/panels.rs`), and `collect_snapshot` puts it on the web wire as `WebAttention` (`commands/status/web/model.rs`). No view can diverge on which stages get flagged or what it says.

Each entry carries `command: Option<String>` (a shell command the operator should run), `command_when` (the precondition printed before it, "Once <when>:"), `alternatives: Vec<Alternative { when, command }>` (printed "If <when>: <command>"), `acceptance` (a Blocked stage's criteria, which static `loom status` prints with the 0-based index `loom stage amend --index` takes), `note: Option<String>` (prose, never shown as a copyable command) and `automatic: bool` (loom is handling the state itself). `status_guidance` maps each status to a label plus guidance:

| Label | Guidance |
| --- | --- |
| MERGE CONFLICT, MERGE ERROR | automatic while the live merge resolver is working or one is awaited, with attempts used out of `MAX_MERGE_RESOLVER_ATTEMPTS` and where to watch it (`merge_guidance`); manual once the resolver is `Stale`. A `MergeBlocked` stage with a `merge.block` carries the block sentence as `failure_info` evidence and is retried, not given a resolver |
| STASH NOT RESTORED | takes precedence over every other label; command `git stash list`; dismissed by deleting the backup ref (the entry reads `stash_warning`) |
| MERGE BLOCKED | automatic; a typed `merge.block`, and loom retries the merge. An untyped `MergeBlocked` keeps MERGE ERROR with resolver guidance. `loom stage merge` prints the typed-block next step |
| BLOCKED | `render/blocked_guidance.rs`. Automatic auto-retry note when `should_auto_retry` holds (crash or timeout under the limit). A deliberate block (a reason, no `failure_info`): note "blocked by the stage agent: <reason>" (or "by the operator (rejected in human review)" when `review_reason` equals it) (the CLI and TUI append the saved terminal history path through `AttentionEntry::text_note()`; the web gets `terminal_history: bool` and links `/terminal/<id>`); command `loom stage retry <id> --context "<what you did and what it showed>"` once the reason's ask is done (`--force` at the limit); alternatives amend delete, amend replace and skip (a review rejection offers only skip). When `StageSummary.block_detail.failing_criteria` names failing criteria, each gets filled `--index <i>` amend commands; otherwise the `<n>` text stays. A setup failure (InfrastructureError, SandboxSetupFailure, StartupRefusal) gets retry after "you have fixed the cause named in the evidence" |
| ACCEPTANCE FAILED | `loom stage retry <id>`, plus `--force` and a limit note once `retry_count >= max_retries` (`retry_guidance`) |
| NEEDS REVIEW | no command; `has_human_review_choices` makes every view print the three full `human_review_choices` commands (:102) |
| NEEDS INPUT | note: answer the question in the stage's terminal (`loom stage resume` is hook-internal and answers nothing) |
| ADJUDICATING | automatic note |
| CLEANUP FAILED | `loom worktree remove <id>` |
| COMPLETION PENDING / BLOCKED, WRITER UNCONFIRMED | note is the blocker's `next_action`; automatic while the stage is Executing; review choices once it is parked in NeedsHumanReview (:146) |

The merge facts come from `merge_resolver_facts` in `commands/status/data/resolver.rs`. It is read-only: `live_merge_resolver` scans the loaded sessions, picks the newest live `Merge` session of the stage by `created_at` (the one `loom attach` and the web terminal attach to), and never calls `find_live_merge_session_for_stage`, which deletes stale signal files. They travel as optional `StageSummary` keys because daemon frames are deserialized before attention is built (mistakes/computed-values-and-hidden-couplings.md), and the browser snapshot carries them (`schema.ts` declares them). `stage.session` never names a resolver: it keeps the ended worker. `speaking_for` (`resolver.rs`) therefore substitutes the live resolver while the stage is `MergeConflict` or `MergeBlocked`, so `pid`, `session_alive`, `session_backend` and `session_type` describe the resolver and the web terminal gate opens on it. A resolver's activity is judged by its own heartbeat, or by its `created_at` before its first tool call (`heartbeat_facts.rs`). The merge note (`merge_guidance`, `attention_model.rs`) says the resolver is working; once it is `Stale` (silent past its stage's budget with no tool call open; `Busy` and `Quiet` stay automatic) it becomes a manual entry saying the resolver may be inside one long command or may have stopped, names where to look (`loom attach <id>` or the web terminal on tmux, the `loom-merge-<id>` window on native), and only then `loom sessions kill`. Loom has no end-of-turn signal for a resolver, so this staleness is the only idle detection. The TUI header and the web header count only non-automatic entries, and the web groups them under "needs attention" and "handled by loom". The TUI panel height comes from `attention_line_count` (`ledger/panels.rs`), so a review entry's three commands are never clipped.

Evidence lines are capped at `MAX_EVIDENCE_LINES=32` (`commands/status/data/sanitize.rs`), because `crash_classification.rs` builds up to 21 evidence lines for a startup-refusal crash. A cap that bites pushes an explicit truncation marker.

`AttentionEntry.reason` is the header text for every view: `close_reason` for a Blocked stage, `review_reason` only while the stage is in review (an old dispute's `review_reason` never reaches a Blocked stage's header). `StageSummary.block_detail` (omitted from the wire when empty) carries `failing_criteria` (index, command, diagnostic when exactly one failed), read by `data/criteria_results.rs` from the latest checkpoint's `acceptance-<i>` results, and `disputes` (the last 5 records, read leniently by `data/disputes.rs` from both `patches` and single `patch` verdicts). No free-text reason is parsed, so the failing list is a guide for the operator and the block refusal, not proof.

`close_reason` reaches `StageSummary` only for a Blocked stage (`data/stopped.rs` `blocked_close_reason`), neutralized like `flatten_multiline` but keeping its line breaks and bounded at 4000 characters with a marker naming the stage file (`bound_close_reason`); `inline_safe`'s 200-character cap would cut the command an agent asks the operator to run. Static `loom status` prints the note whole, wrapped (`render/wrap.rs`); the web panel renders it with `whitespace-pre-wrap`; the `--live` ledger (`ledger/entries.rs`) shows at most three wrapped lines, then `…`, puts each alternative's "If <when>:" on its own line above its command (commands get a two-cell indent so `--op` stays visible at 80 columns), ends a cut line with `…`, and adds a pointer to `loom status` for the numbered criteria. The panel holds up to `ATTENTION_MAX_HEIGHT` (14) rows (`ledger/layout.rs`); `fit_to_rows` (`ledger/panels.rs`) ends a clipped panel with "… N more lines". When the table would fall below `TABLE_MIN_HEIGHT`, the panel gives up one row at a time, never below `ATTENTION_MIN_HEIGHT` (4). `terminal_history_session` names the newest ended tmux session of the stage with a saved transcript ([terminal backends](terminal-backends.md#terminal-history)).

## What the Live TUI Renders Today

The live TUI is a responsive TABLE (the "ledger"), one row per stage, built under `loom/src/commands/status/ui/tui/ledger/` (modules `cells`, `cells_activity`, `columns`, `entries`, `header`, `layout`, `layout_tokens`, `legend`, `mod`, `panels`, `quota`, `rows`, `text`, `tokens_models`, `tokens_pane`). Entry point `ledger::render` (`ledger/layout.rs:20`) is called from `TuiApp::render` (`app.rs:350`).

Nine columns (`ColumnKind` in `ledger/mod.rs`), in `DESIGNED_COLUMNS` order with their designed widths (`ledger/columns.rs`): State 12, Stage 25, DependsOn 16, Models 16, Activity 14, Context 9, Tokens 10, Time 7, Merge 8. They are laid out at designed width when the terminal has `FULL_WIDTH` = 132 cells (`ledger/mod.rs:55`) and dropped in the order Time, Models, DependsOn, Tokens, Context as it shrinks (`columns_for_width`); the rest expand to fill. Below `MIN_COLS` = 64 or `MIN_ROWS` = 16 a notice replaces the dashboard entirely. The MODELS column is 16 cells wide, pinned by the `FULL_WIDTH` budget and the drop-order termination proof at 64; content wider than that truncates (e.g. `opus›sonnet+1`). The TOKENS cell shows an executing stage's fresh rate per minute and a dimmed total for any other stage that has a flow (`cells.rs`); a stage without a flow shows a blank cell. The quota line keeps its own tiers at `QUOTA_FULL_WIDTH` = 120 (`ledger/quota.rs:19`) so they do not move with `FULL_WIDTH`. Token-flow data, its sources and the surfaces that show it: [token-flow-sources.md](token-flow-sources.md).

`budget` (`ledger/layout_tokens.rs:52`) apportions rows across the alert band, token pane, attention panel, activity panel, footer and the table itself; `render` returns a `RenderOutcome` whose `table_viewport_rows` the app writes back into `graph_state.viewport_height`. When the table would fall under `TABLE_MIN_HEIGHT` (6) the bands give way in this order: by-model block, then the token pane, activity panel, attention panel down to `ATTENTION_MIN_HEIGHT` (4), compact mode, then the quota line.

Two keys toggle overlays (`ui/tui/event_handler.rs`): `?` (`ToggleLegend`, line 35) opens the legend overlay listing every state (`ledger/legend.rs`), and `t` (`ToggleTokens`, line 36) flips `TuiApp.show_tokens`, which shows the per-agent token pane (`ledger/tokens_pane.rs`, at most `PANE_MAX_ROWS` = 27 rows: 16 live agent rows, a blank line and the 10-row by-model block of `ledger/tokens_models.rs`) between the table and the attention panel when `data.tokens` is present. When the whole pane does not fit, `render` (`ledger/layout.rs:62`) tries the live rows alone before dropping the pane. Neither overlay is shown by default.

The footer error line is wired end to end: `TuiApp.last_error` (`app.rs:52`) is set on daemon exit and on `Response::Error`, cleared on every successful `StatusUpdate`, passed through `LedgerView.last_error` and rendered by `panels::render_footer` (`ledger/panels.rs:77`).

The header's liveness indicator ("daemon running, tick Ns ago") is computed CLIENT-side from `.loom/work/orchestrator.tick`, not from the daemon broadcast (`app.rs` `refresh_alerts`) — it can keep asserting health while the daemon is failing to push status at all. The daemon-side oversized-payload guard sends `Response::Error` instead of silently skipping a frame, specifically so the footer can show the fault; see mistakes.md for why a silent skip is the wrong shape here.

TUI cell padding measures with `Span::width()` (unicode-width aware, `ledger/header.rs`'s `text_width()` and `ledger/text.rs`), never `chars().count()` — at least one status icon (`⚡` MergeConflict, U+26A1) is East-Asian Wide (one char, two terminal cells), and a char-count pad shifts every later column by one cell on that row.

Colors and icons for `StageStatus` come from `loom/src/models/stage/types.rs:986-1096` (see "StageStatus — 13 Variants" above); the ledger has no separate per-stage color cycle.

`loom status --web [PORT]` (see [web-dashboard.md](web-dashboard.md)) is a second live renderer of the identical `StatusData`/`StageSummary` payload, alongside the compact view and this TUI ledger. It does not reimplement the Rust-side derivation: it reuses `render::attention_entries`, `render::failure_label`, `scheduling_report::alerts`, `tick::read`, and `data::collect_status_data` directly, and ports only the TUI's display semantics (icons, labels, legend text) to TypeScript in `web/src/lib/format.ts`, pinned against this file's serde output by `loom/src/commands/status/web/model_tests.rs`.

The build version reaches all three surfaces from one pair of consts, `crate::version::VERSION` and `crate::version::LABEL` (`loom/src/version/mod.rs`, both derived from `build.rs`'s `LOOM_VERSION`). The static view passes `LABEL` as the logo subtitle (`commands/status.rs:198`); the TUI right-aligns it on the header's merge line and drops the token summary first and then the label when the width cannot hold the counts plus a space plus the label (`ledger/header.rs`'s `merge_line`); the web dashboard reads it off the wire as `WebSnapshot.version` (`commands/status/web/model.rs`, mirrored in `web/src/api/schema.ts`) and prints it at the right end of the footer (`Footer` in `web/src/routes/shell.tsx`), which renders whenever a snapshot has arrived and carries the quota gauges on its left only when a provider has data. The web fixture pair (`web/src/api/fixtures/snapshot.json` and `fixture_snapshot()` in `model_tests.rs`) holds the same synthetic `0.0.0-fixture` on both sides, so that comparison never tracks a release; `tests/pure.rs`'s `poll_files_once_carries_the_degrade_notice` is what pins the real publish path to `crate::version::VERSION`.

## What Does Not Exist on the Status Path

Stated explicitly because these are natural things to expect and go looking for:

- **Subagent state.** Subagent STATE (Done/ToolWait/Generating classification from `loom subagents`, `classify.rs:83-99`) is still never read by the status path, and `peak_resident_tokens`/`request_count` remain outside the status models. What the status path DOES now read from `.loom/work/subagents` is the spawn ledger (`spawns.jsonl`) and the codex ledger (`codex.jsonl`), for `StageSummary.execution_models` — distinct execution-model display names in first-seen order, empty until a subagent spawns (see "Execution-Model Ledgers" below). Both ledgers are written by PreToolUse hooks running outside the stage's Bash sandbox, so a worktree stage session records normally and `execution_models` is empty only until the first subagent spawns.
- **Per-criterion acceptance results.** `CriterionResult`/`AcceptanceResult` (`loom/src/verify/criteria/result.rs:7-20,87-95`) are never serialized to disk; only `failure_info.evidence` survives a run. The pass cache (`verify/criteria/cache.rs:93,369-386`) is keyed by command digest and is not read by status.
- **Historical handoff counts.** Status only checks that the handoffs directory exists (`diagnostics.rs:11,37`); compact mode counts stages currently in `NeedsHandoff`, not historical handoff events (`render/compact.rs:30-37`).
- **A persisted orchestrator event log.** There is none.
- **`plan_id`, the plan's source path, the IN_PROGRESS/DONE filename prefix, or a run start time** on `StatusData`.
- **A `RemoteControl` struct on the status path.** Only `[remote_control]` and `[terminal]` config sections exist in `.loom/work/config.toml`.

## Execution-Model Ledgers

`StageSummary.execution_models` is populated by `execution_models_for_stage(work_dir, stage_id)` (`commands/status/data/execution_models.rs:33-54`), which reads two append-only ledgers under `.loom/work/subagents/<stage_id>/`: `spawns.jsonl` (written by the `loom-hooks/spawn-guard.sh` PreToolUse hook, running outside the stage's Bash sandbox) and `codex.jsonl` (written by the `loom-hooks/codex-forward-guard.sh` PreToolUse hook, at the moment it authorizes a forward, also outside that sandbox — a blocked command records nothing). `codex-forward.sh` does not write that row: it runs INSIDE the stage's Bash sandbox, where an append through the worktree's `.loom/work` symlink is denied and silently swallowed. The forwarder's own `spawns.jsonl` row carries the shim's sonnet tier and is deliberately skipped. Each ledger is capped at `MAX_LEDGER_BYTES=256*1024` (`commands/status/data/execution_models.rs:23`) and the result at `MAX_EXECUTION_MODELS=8` distinct names (`commands/status/data/execution_models.rs:30`).

Row values are untrusted: the `model` field is the caller-controlled `.tool_input.model` written verbatim by the hook. `normalize_model`/`strip_date_suffix` (`commands/status/data/execution_models.rs:118-140`) strip a `claude-` prefix and a trailing `-YYYYMMDD` stamp using `rsplit_once('-')` (byte-index slicing panicked on a multi-byte model name before this fix). Flattening (`context::untrusted::inline_safe`) runs BEFORE dedup/normalize, not after — deduping on the raw ledger string let `sonnet` and `sonnet<zero-width char>` count as two distinct models.

`valid_stage_id` guards the ledger path on both sides of the read/write boundary, but at different strengths: the Rust reader (`commands/status/data/sanitize.rs:84`) explicitly rejects `.` and `..`; the shell writers (`loom-hooks/spawn-guard.sh:309`, `loom-hooks/codex-forward.sh:43`) are character-class allowlists (`[A-Za-z0-9._-]`) that accept `.` and `..` because both are made only of allowed characters — a stage id of `..` resolves the ledger directory to the work dir itself. See concerns.md for the outstanding shared-helper cleanup.

## Status Command Module Layout

The status command is organized as a sub-module tree:

```text
commands/status.rs          # Entry: dispatches to 3 modes + validate/doctor
commands/status/
  data.rs                   # collect_status_data() → StatusData struct
  render/                   # Pure render functions (progress, graph, merge, compact)
  ui/                       # TUI backed by daemon IPC subscription
  diagnostics.rs            # Workspace integrity checks
  display.rs                # count_files() helper
  merge_status.rs           # Merge section data
  validation.rs             # Markdown + cross-reference validation
```

**Data flow (static mode):** `collect_status_data()` loads plan name, stage list (with status/context), session list, merge state, and progress counts into a single `StatusData`. Renderers receive `StatusData` and write to `impl Write`.

**TUI mode:** `ui::run_tui(work_path)` subscribes to the daemon's Unix socket (`orchestrator.sock`) and re-renders on each update. Requires daemon running; errors with hint if not.
