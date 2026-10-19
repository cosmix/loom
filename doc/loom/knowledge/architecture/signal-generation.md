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

The standard prefix contains worktree and path boundaries, the working-directory reminder, execution rules and knowledge navigation, the orchestration-skill pointer, the subagent verification and context-ceiling blocks, adversarial review, and verifier/completion rules. Its generator is `generate_stable_prefix`; there is no `generate_standard_stable_prefix`.

Integration verification adds its mission, review dimensions, and one canonical verifier for the complete gate. Knowledge distillation has a single curator and memory-ordering guidance. Knowledge/bootstrap has its own mission and completion workflow. Documentation prefixes omit the code-review block.

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
| `cache/blocks.rs` | `append_verifier_completion_header`: standard-stage gate owner |
| `helpers.rs` | `append_commit_timing_rules`, `append_completion_rules`, `append_settled_completion_rules`: completion ordering |
| `helpers.rs` | `append_knowledge_commit_header`, `append_knowledge_commit_bullet`, `append_knowledge_mission`: knowledge workflow |

Call helpers explicitly from the intended prefix generators. Keep pinned worker blocks identical across their delivery surfaces; the doctrine tests enforce that contract. Standard and IntegrationVerify receive adversarial review; Knowledge and KnowledgeDistill do not. The recovery formatter embeds `stable_prefix_for(stage.stage_type)`, preserving the same selection on resume. The removed staging, anti-slop and subagent-restriction helpers are not extension points.

## Soft Signals

Hung-session detection is entirely heartbeat-driven; nothing reads session JSONL. A silent stage
session is warned about, recovered, or parked in `NeedsHumanReview`, depending on whether it ever
worked.

**Detection (`orchestrator/monitor/heartbeat.rs`, `hung_latch.rs`, `never_worked.rs`):**

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
4. Adjudication sessions (judges) are measured and latched separately (`adjudicator_stall_event`
   → `MonitorEvent::AdjudicatorStalled`): a stalled judge has no stage to hand off, so it is closed
   and the stage it left in `NeedsAdjudication` is re-judged on the next poll under the dispute's
   own attempt budget, latched once.
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
- **Reminder box**: Knowledge, IntegrationVerify and KnowledgeDistill stages get "KNOWLEDGE UPDATES REQUIRED" (`append_knowledge_updates_required_box`). A Standard stage gets no box; its memory reminder sits in `## Stage Memory`.
- **`## Knowledge Management`** (Knowledge, IntegrationVerify, KnowledgeDistill): with `knowledge_tree_empty` a "KNOWLEDGE BASE IS EMPTY" warning and the exploration order, otherwise "Extend the knowledge base as you work"; then the `loom knowledge update` command table. KnowledgeDistill adds `loom review` and the DOCUMENTATION UPDATE REQUIRED box.
- **`## Stage Memory`** (Standard): the SESSION MEMORY REQUIRED table (`mistake:`, `found:` and `gotcha:` notes, decisions, changes, questions, with `--evidence`), "NEVER use 'loom knowledge' in implementation stages" and the auto-memory prohibition.
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
