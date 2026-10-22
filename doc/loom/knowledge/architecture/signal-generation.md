---
sources:
- loom/src/telemetry/mod.rs
- loom/src/telemetry/spool.rs
- loom/src/commands/knowledge/telemetry.rs
- loom/src/orchestrator/core/stage_telemetry.rs
verified: 5546d3c47ddc1f8890b40157134f057393b8b90e
---
# Signal Generation

> Signal assembly, cache, prefixes

## Signal Generation Pipeline (orchestrator/signals/) [DETAILED]

Signals use four sections: stable prefix, semi-stable context, dynamic stage details and recitation. `format/mod.rs::format_signal_with_metrics` assembles them and computes `SignalMetrics`. `cache.rs::stable_prefix_for` selects the prefix for Standard, IntegrationVerify, KnowledgeDistill or Knowledge; `recovery_format.rs` uses the same selector.

The standard prefix contains worktree and path boundaries, the working-directory reminder, execution rules and knowledge navigation, the orchestration-skill pointer, the subagent verification and context-ceiling blocks, adversarial review, and the completion rules, which open with `ORCHESTRATOR_GATE` (the orchestrator's `loom check`). Its generator is `generate_stable_prefix`; there is no `generate_standard_stable_prefix`.

Integration verification adds its mission, review dimensions, and the `INTEGRATION_VERIFY_OVERRIDE` block, under which the orchestrator runs `loom check integration-verify` once for the complete gate. Knowledge distillation has a single curator and memory-ordering guidance. Knowledge/bootstrap has its own mission and completion workflow. Documentation prefixes omit the code-review block.

Delegation details live in `skills/loom-orchestration/SKILL.md`. The prefix uses `LOAD_ORCHESTRATION_SKILL`; it does not emit a separate delegation paragraph. The signal assemblers do not call `append_subagent_restrictions`, `append_anti_slop_guidance`, `append_git_staging_full`, `append_git_staging_rules` or `append_common_footer`. Source: `loom/src/orchestrator/signals/cache.rs`, `cache/blocks.rs`, `helpers.rs`.

## Shared signal helpers

Shared signal blocks live in `cache/blocks.rs` and `signals/helpers.rs`.

| Location | Helpers and responsibility |
| --- | --- |
| `cache/blocks.rs` | `append_path_boundaries`, `append_isolation_boundaries_simple`: confinement |
| `cache/blocks.rs` | `append_execution_rules_header`: binding rules pointer and knowledge/source navigation |
| `cache/blocks.rs` | `append_no_verify_block`, `append_subagent_ceiling_block`: pinned worker contracts |
| `cache/blocks.rs` | `append_adversarial_review`: six review dimensions for code-producing stages |
| `cache/blocks.rs` | `append_review_dimension_details`: integration-verification dimensions |
| `cache/blocks.rs` | `append_gate_completion_header`: opens a standard stage's `**Completion:**` list with `ORCHESTRATOR_GATE`, which makes the orchestrator's `loom check` the gate |
| `helpers.rs` | `append_commit_timing_rules`, `append_completion_rules`, `append_settled_completion_rules`: completion ordering |
| `helpers.rs` | `append_knowledge_commit_header`, `append_knowledge_commit_bullet`, `append_knowledge_mission`: knowledge workflow |

Call helpers explicitly from the intended prefix generators. Keep pinned worker blocks identical across their delivery surfaces; the doctrine tests enforce that contract. Standard and IntegrationVerify receive adversarial review; Knowledge and KnowledgeDistill do not. The recovery formatter embeds `stable_prefix_for(stage.stage_type)`, preserving the same selection on resume. The removed staging, anti-slop and subagent-restriction helpers are not extension points.

## Soft Signals

Hung-session detection is entirely heartbeat-driven; nothing reads session JSONL. A silent stage
session is warned about, recovered, or parked in `NeedsHumanReview`, depending on whether it ever
worked.

**Detection (`orchestrator/monitor/heartbeat.rs`, `hung_latch.rs`, `judge_watchdog.rs`, `running_command.rs`, `never_worked.rs`):**

1. `HeartbeatWatcher::check_session_hung(stage_id, session_id, timeout)` compares the session's
   last heartbeat against its response budget (`Stage::effective_subagent_timeout_secs()`, falling
   back to `MonitorConfig.hung_timeout`, default 300 s / `DEFAULT_HUNG_TIMEOUT_SECS`). A heartbeat
   naming another session is a previous attempt's and says nothing about this one. A session with
   no heartbeat at all is measured from `created_at` (`silence_without_heartbeat`).
2. `hung_latch::is_stall_escalation` reports a silence twice, never more: once when the budget is
   first crossed (a warning), and once more when the silence reaches `STALL_ESCALATION_MULTIPLIER`
   (3x) the budget. A session that answers again clears both latches (`clear_hung_report`), so a
   blip cannot creep toward escalation across unrelated silences.
3. A session that **never worked** (`never_worked::never_worked`) escalates at 1x its budget
   (`is_escalation`): a `SessionType::Stage` session with `context_tokens == 0` whose own heartbeat
   names no tool and no subagent (the one `session-start.sh` writes, `last_tool: null`), or that has
   no heartbeat past `created_at` plus its budget. The token count decides because `session-start.sh`
   also fires on compaction and resume and rewrites the heartbeat in the same shape. Contract,
   Knowledge, Merge, BaseConflict and Adjudication sessions keep their own handling.
4. Adjudication sessions (judges) are measured and latched separately (`judge_watchdog.rs`
   `adjudicator_stall_event` → `MonitorEvent::AdjudicatorStalled`): a stalled judge has no stage to
   hand off, so it is closed and the stage it left in `NeedsAdjudication` is re-judged on the next
   poll under the dispute's own attempt budget, latched once. A judge whose own process has a
   running command (`running_command.rs`: a shell, `bwrap` or `sandbox-exec` child that itself runs
   something) is held and announced once with `JUDGE WAITING:`, up to a ceiling of the disputed
   stage's summed criterion timeouts plus one budget from its last heartbeat; a judge running nothing
   is closed at the budget as before.
5. A `subagent_timeout_secs: 0` stage never escalates (warnings only), so a mis-configured zero
   budget cannot get a session killed on its first poll.

**Action (`orchestrator/core/event_handler/recover_hung.rs`, `core/loop_recovery/`):** the
`SessionHung` arm calls `on_session_hung`, which prints the warning and then, on an escalation line:

- takes no action when the stage no longer names this session or is not `Executing`, or when a
  current trusted completion blocker owns the stage;
- **never worked:** parks the stage at the first report. No handoff is written and no recovery is
  charged. It runs `claude auth status` under the stage environment (`stage_auth_status`) and uses
  the login remedy as the reason when the answer is a definite `NotLoggedIn`, else the
  never-worked reason; the pane tail is captured before the takedown;
- **worked, `stall_recoveries < MAX_STALL_RECOVERIES` (2):** writes the stall handoff, charges a
  recovery and re-queues the stage for a continuation session;
- **worked, recoveries exhausted:** takes the agents down, writes the stall handoff and parks the
  stage. The reason names the session, the silence, the budget, the last activity, the recovery
  count and the last pane line, then a blank line, `Last pane lines:` and up to 40 pane lines
  (`PANE_TAIL_LINES`, each capped at 200 characters by `with_pane_notes`). `review_notes`, the web
  view's field, is derived from `review_reason`.

`finish_handoff_and_park` parks only once every agent is confirmed gone (approving the review
re-queues the stage, which must not admit a second writer), releases the session, and sets the
status with the short constant reason `"stall park"` because `force_status_with_reason` logs its
reason at ERROR: agent-controlled pane text lives only in `review_reason`. The needs-human-review
announcement prints and notifies the reason's one-line `review_headline` (the text before the pane
markers, flattened and cut to 200 characters), never the pane tail. `human-review --approve`,
`stage reset` and `stage retry` reset `stall_recoveries` to 0. There is no STALLED status marker:
a stall that gives up leaves the stage in `NeedsHumanReview`, which every surface already renders.

## Telemetry (`loom/src/telemetry/`)

One append-only JSON-lines file, `.loom/work/telemetry/events.jsonl`, recording five best-effort event
kinds (`TelemetryEvent`, `telemetry/mod.rs`): `ContextDelivered`/`ContextUnavailable` for a spawned
session's context brief, `PromptBrief`/`PromptAbstained` for the `UserPromptSubmit` hook's per-turn
brief (see [Context Retrieval](context-retrieval-state.md#brief-delivery-sanitization-and-telemetry)), and
`ContextPulled` for a `loom knowledge context` retrieval. `emit` may never fail a spawn and
`read_events` skips a malformed line rather than failing the file; every count is an item count,
never a token saving.

`emit` first checks the process's relay mode (`crate::relay::emit::mode`, since `dbec517c`). In
`RelayMode::Relay` it never touches `.loom` at all: `relay_telemetry` writes a relay ticket through
`RelayContext::emit_quiet`, and a relay `check` refusal or a serialization error is silently
swallowed (still best-effort). In Legacy/Operator mode the earlier behavior applies: a sandboxed
session cannot write through the worktree's state-root symlink, so a denied direct write falls back
to a per-worktree spool (`telemetry/spool.rs`, `.loom/telemetry-spool.jsonl`) that the daemon later
drains into the canonical event file.

`ContextDelivered`/`ContextUnavailable` are written by `orchestrator/core/stage_telemetry.rs`
(called from `stage_executor.rs:570`), deriving their fields from the `DeliveryRecord` signal
generation already wrote — no second retrieval. `loom knowledge telemetry`
(`commands/knowledge/telemetry.rs`) is `read_events`'s production caller: it summarizes every event
kind per stage (briefs, prompt briefs emitted/abstained with the top abstain reason, pulls with
average budget/items and unmet-required count, last event time).

## Signal Sections: Semi-Stable, Dynamic, Recitation and EmbeddedContext

### Semi-Stable Section (`format/sections.rs::format_semi_stable_section`)

Changes per **stage type**, not per session. Sub-sections, in order:

- **Knowledge Brief**: the stage's retrieved pack (`format/brief.rs::format_stage_brief`), only when retrieval selected anything; its footer names `loom knowledge context --stage <id> --query "<question>" --budget-tokens <n>`.
- **Reminder box**: Knowledge and KnowledgeDistill stages get "KNOWLEDGE UPDATES REQUIRED" (`append_knowledge_updates_required_box`). Standard and IntegrationVerify stages get no box; their memory reminder sits in `## Stage Memory`.
- **`## Knowledge Management`** (Knowledge, KnowledgeDistill): with `knowledge_tree_empty` a "KNOWLEDGE BASE IS EMPTY" warning and the exploration order, otherwise "Extend the knowledge base as you work"; then the `loom knowledge update` command table. KnowledgeDistill adds `loom review` and the DOCUMENTATION UPDATE REQUIRED box.
- **`## Stage Memory`** (Standard, IntegrationVerify): the SESSION MEMORY REQUIRED table (`mistake:`, `found:` and `gotcha:` notes, decisions, changes, questions, with `--evidence`), "NEVER use 'loom knowledge' in this stage — knowledge-distill curates memory into knowledge." and the auto-memory prohibition (`format_semi_stable_section`, `sections.rs:199`).
- **Codex implementers** (`format/codex.rs::format_codex_implementers_section`): only when `implementers` includes codex.
- **`## Ultracode Mode`**: only when `embedded_context.ultracode`. It licenses the Workflow tool and states that Workflow fan-out spawns Claude subagents only: the codex lane (`gpt-5.6-terra`/`gpt-6-luna`) is not addressable from a Workflow script, so on a stage licensed for both, codex-tier work goes through ordinary `loom-codex-forwarder` Agent spawns outside the Workflow.
- **Sandbox restrictions** (`format/sandbox_section.rs::format_sandbox_section`): when a sandbox summary exists. With the sandbox enabled it always carries `append_package_cache_note()`, the package-manager-cache carve-out, after the filesystem deny/allow-write block and before the network section.
- **Skill recommendations** (`format/skills.rs::format_skill_recommendations`): skill-index matches, when there are any.

The semi-stable section carries no delegation guidance of its own.

### Dynamic Section (`format/sections.rs::format_dynamic_section`)

Per-session content: Target (session, stage and plan ids, working_dir, execution path), Worktree Isolation, Plan Overview, the cross-stage summary and the wiring checklist (IntegrationVerify and KnowledgeDistill only), Assignment, Dependencies Status and Dependency Outputs, the previous session's handoff (structured, prose, or a pointer to `.loom/work/handoffs/<file>.md`), Acceptance Criteria, Goal-Backward Verification (artifacts, wiring, wiring_tests, dead_code; `format/sections/goal_backward_section.rs`) and Files to Modify.

### Recitation Section (`format/sections.rs::format_recitation_section`)

End of the signal, for maximum attention: a single `Context: {tokens} of {ceiling} tokens` line (absolute resident tokens against the resolved ceiling), `## Immediate Tasks`, the stage end sequence line (`append_stage_end_sequence()`, `format/helpers.rs`, recited right after the task list, before the trailing blank line), and `## Stage Memory` (with a prominent warning when it is empty).

### EmbeddedContext Struct (`types.rs`, `EmbeddedContext`)

Single container flowing through all four sections.

```rust
pub struct EmbeddedContext {
    pub handoff_content: Option<String>,      // V1 prose handoff
    pub parsed_handoff: Option<HandoffV2>,    // V2 structured handoff
    pub plan_overview: Option<String>,
    pub context_pack: Option<ContextPack>,    // this stage's retrieved brief, when retrieval selected anything
    pub knowledge_tree_empty: bool,           // doc/loom/knowledge/ has no real content; distinct from context_pack (see the field's doc comment)
    pub memory_content: Option<String>,       // Last 10 entries
    pub skill_recommendations: Vec<SkillMatch>,
    pub context_ceiling_tokens: Option<u32>,  // absolute resident-token ceiling, not a percentage
    pub context_tokens: Option<u32>,          // absolute resident tokens read from the heartbeat
    pub sandbox_summary: Option<SandboxSummary>,
    pub cross_stage_summary: Option<String>,  // IV/KnowledgeDistill only
    pub wiring_checklist: Option<String>,     // IV/KnowledgeDistill only
    pub ultracode: bool,
    pub implementers: Implementers,           // Licensed lanes, in preference order
    pub codex_available: bool,                // codex CLI + plugin installed; resolved once at build time
    pub subagent_timeout_secs: Option<u64>,   // Per-stage override; None emits nothing
}
```

### Caching

SHA-256 of stable prefix text → first 16 hex chars → `SignalMetrics::stable_prefix_hash`. Cache invalidated whenever the stable prefix Rust code changes. Semi-stable, dynamic, recitation sections are always regenerated.

## Signal Replay Harness

`loom/tests/signal_replay.rs` (modules in `tests/signal_replay/`: `input.rs`, `render.rs`,
`report.rs`) renders every stage signal of a frozen plan, and every worker query, and prints bytes
per section. It measures what a change to signal or worker-brief content costs in resident bytes.
Run it at the base and the candidate revision on the same host and diff the two reports. Later
plans locate it by path (`test -f tests/signal_replay.rs` from `loom/`); a grep cannot, because
`tests/doctrine_guard_contracts.rs` also names `format_signal_with_metrics`.

- **Test and invocation.** Test `signal_replay`, marked `#[ignore]`, so plain `cargo test` never
  runs it and a gate must pass `--ignored`. From `loom/`:
  `cargo test --test signal_replay -- --ignored --nocapture`. With `SIGNAL_REPLAY_OUT=<file>`
  set, the printed lines are also written to that file, both marker lines included.
- **Self-checks.** The harness renders twice, once under a nested root, and asserts the two
  reports are identical (a leaked temp path or clock fails it), checks the section set and the
  record shape, and requires at least one worker delivery record to deliver something.
- **Frozen input** (`tests/fixtures/signal_replay/`, never edited after the first commit;
  markdownlint skips `loom/tests/fixtures/`, so the pre-commit `--fix` pass cannot rewrite it).
  Provenance is in the fixture's `README.md`: `plan.md` is the archived PLAN-token-flow plan,
  `knowledge/` holds copies of the seven tier-1 files, `memory/token-core.md` is token-core's
  archived journal, `recorded-spawn.json` is one recorded worker spawn payload, and
  `requests.json` holds recorded request counts (unique assistant message ids per stage session;
  worker: the median of token-core's 14 worker transcripts, 9.5 rounded half up).
- **Rendering.** Signals go through `generate_signal_with_metrics` (the production context
  build, then `format_signal_with_metrics`) and `generate_signal_with_skills` with no skill index,
  so skill-catalog changes do not show; the asset lines report skill files by size. Worker
  queries run through `loom hook worker-brief`. `parse_plan_content` takes the plan id from the
  file stem, so the harness copies `plan.md` to `<root>/doc/plans/PLAN-token-flow.md` and parses
  that path: every `stage.plan_id` reads `PLAN-token-flow` and the `worker_material` filter drops
  `doc/plans/PLAN-*` items from briefs as in production. The skipped knowledge stage is set
  `Completed` with `merged = true` through `verify::transitions::update_stage`; `save_stage` is
  an alias of `create_stage` and refuses an existing stage.
- **Output format `signal-replay v1`.** Between `=== signal-replay v1 ===` and
  `=== end signal-replay ===`, tab-separated records. The header is
  `signal-replay v1 plan=PLAN-token-flow estimate=bytes-times-recorded-requests`. Record kinds:
  `signal` (stable prefix, semi-stable, dynamic, recitation, total and file bytes per stage),
  `worker` (query, brief and envelope bytes per query), `asset` (template, preamble, agents and
  skills file sizes) and `total`, whose fields are
  `signal_bytes=<n> file_bytes=<n> brief_bytes=<n> asset_bytes=<n> resident_estimate=<n>`.
  `resident_estimate` multiplies bytes by the recorded requests after the admission point and is
  an estimate, not a measurement.
- **Baseline.** On this tree the total reads `signal_bytes=214337 file_bytes=227124
  brief_bytes=26569 asset_bytes=1619933 resident_estimate=19651501`; the usage-evidence plan
  changed no signal content, so the number was the same before and after it.
- **Known limits.** Signals render with the real `HOME` while `worker-brief` runs with an
  isolated one, so developer-level config could shift byte counts between machines even though
  the two-root check passes on one. `check_shape` compares asset paths as strings but
  `asset_paths()` sorts `PathBuf`s component-wise; a skill `loom-md` beside `loom-md-tables`
  would fail the self-check (see
  [concerns](../concerns/token-accounting-and-proof-defects.md#unimplemented-reviewer-suggestions-from-the-usage-evidence-plan)).
