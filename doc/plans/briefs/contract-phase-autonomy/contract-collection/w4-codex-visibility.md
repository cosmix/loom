# W4: direct-mode codex forwards in lifecycle reconcile and status

Tier: opus, effort high (loom-senior-software-engineer). Wave 1, beside W1 and W3. Independent of W1-W3; you need not read their common.md. Read `doc/loom/knowledge/mistakes/codex-direct-mode-invisible.md` and the plan's Decision 13 first. Line numbers are from plan time; locate each symbol with `rg -n` before editing.

## The defect

`loom-hooks/codex-forward.sh` (about L247-253) sets `mode=direct` when `nested_bubblewrap_refused` (Linux, since 27a62e80) or `nested_seatbelt_refused` (macOS, older), and runs `codex exec` through `_codex-direct.py`. No companion job exists under `~/.codex/plugin-data/state/<workspace>-<hash>/jobs`. The daemon's `codex_lifecycle::reconcile_codex_jobs` (`src/codex_lifecycle.rs:46`, calling `reconcile.rs:22` `reconcile_with_state_root`) only locates companion jobs (`jobs::locate_job`, `jobs.rs:25`), so every direct forward is reported Unknown every monitor tick (`orchestrator/monitor/core.rs:40-69` warns when `entries.iter().any(|e| e.is_unknown())`), `lifecycle.jsonl` is never written (the `append_locked` call at `reconcile.rs:217-218` is its only writer), and `status/data/execution_models.rs:85-93` `correlated_requested_model` drops the model because `has_correlated_lifecycle` (`reconcile.rs:87-111`) is false.

## What the evidence is, and when it exists

- The wrapper prints `LOOM-FORWARD-START` and `LOOM-FORWARD-END` together after the run ends (`codex-forward.sh:298-300`). The receipt hook (`commands/hook/forward_receipt.rs:76-99`) runs at PostToolUse and stamps both receipt rows with the same `now`. So a direct forward has **no receipt while it runs**; a `running` receipt never appears alone. The model shows in execution models once the forward ends; mid-run visibility would need a thread id that does not exist yet.
- Companion forwards also get receipts only at PostToolUse (`backend_evidence`, `forward_receipt.rs:145-168`), so "no receipt" says nothing about the backend. The existing test `codex_lifecycle/tests.rs:86-96` asserts a live companion job's Running records appear mid-run; it must keep passing.
- The direct lane prints the outcome word `cancelled` for its 540 s timeout (`codex-forward.sh:290`, `cancelled:124:false`), while `ForwardState` (`models/forward_receipt.rs:68-81`) spells it `Canceled`. The marker then fails to parse (`models/forward_receipt/marker.rs:139-151`) and no receipt is written. Add `#[serde(alias = "cancelled")]` to `ForwardState::Canceled` (the only change to that file). `unknown` stays non-terminal (`marker.rs:171`) and writes no receipt; the window rule below covers it.

## Real evidence (captured 2026-10-23 from liquidmon; use these rows verbatim as test fixtures, with paths shortened as shown)

`subagents/fix-instance-lock/codex.jsonl` row:

```json
{"v":2,"ts":"2026-10-23T16:55:45.000Z","stage_id":"fix-instance-lock","session_id":"session-04b928d3-1792774508","parent_session_id":"cc32e257-3029-4638-9ca4-d5113f0e1247","forwarder_agent_id":"a1e1311ac1c59874b","tool_use_id":"toolu_01QYmUhYVRs82rXC1TawQ4oe","unit_id":"L1-device-lock","invocation_id":"inv-0f4dcc11d81ad6f26cb08d4b1a0c59ba","model":"gpt-5.6-terra","effort":"xhigh","workspace_root":"/repo/.worktrees/fix-instance-lock","companion_version":"1.0.6","companion_path":"/home/u/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs","state_root":"/home/u/.codex/plugin-data/state"}
```

`subagents/fix-instance-lock/forward-receipts.jsonl` rows (running, then terminal):

```json
{"schema":1,"receipt_id":"096464e5f3005de684f92df63d8bbdca0585c7cef4fc98b8365ada2b4c052e74","parent_session_id":"cc32e257-3029-4638-9ca4-d5113f0e1247","agent_id":"a1e1311ac1c59874b","tool_use_id":"toolu_01QYmUhYVRs82rXC1TawQ4oe","stage_id":"fix-instance-lock","loom_session_id":"session-04b928d3-1792774508","backend":"direct","backend_id":"01a169b0-a2fd-7793-9874-db0889fa035c","state":"running","observed_at":"2026-10-23T17:00:37.322417869Z","exit_code":null,"codex_thread_id":"01a169b0-a2fd-7793-9874-db0889fa035c","locator":null,"model":"gpt-5.6-terra","effort":"xhigh"}
{"schema":1,"receipt_id":"096464e5f3005de684f92df63d8bbdca0585c7cef4fc98b8365ada2b4c052e74","parent_session_id":"cc32e257-3029-4638-9ca4-d5113f0e1247","agent_id":"a1e1311ac1c59874b","tool_use_id":"toolu_01QYmUhYVRs82rXC1TawQ4oe","stage_id":"fix-instance-lock","loom_session_id":"session-04b928d3-1792774508","backend":"direct","backend_id":"01a169b0-a2fd-7793-9874-db0889fa035c","state":"succeeded","observed_at":"2026-10-23T17:00:37.322417869Z","exit_code":0,"codex_thread_id":"01a169b0-a2fd-7793-9874-db0889fa035c","locator":null,"model":"gpt-5.6-terra","effort":"xhigh"}
```

Both rows deserialize, and `receipt_id` is `ForwardIdentity::receipt_id()` of (parent, agent, tool use, stage, loom session) (`models/forward_receipt.rs:127-137`). Replay validation needs more than the rows: substitute `workspace_root` with the test's temp canonical workspace (`subagent_lifecycle/codex_evidence.rs:48-51` requires an existing canonical directory), write `stages/fix-instance-lock.md` with `session: session-04b928d3-1792774508` frontmatter (`subagent_lifecycle/claude_files.rs:180-199`), and pass a `Session` with that id and stage, status Running. Add a `liquidmon_direct_fixture` constructor to `codex_lifecycle/test_support.rs` that does all of this.

## Files you own

`src/codex_lifecycle.rs`, `src/codex_lifecycle/reconcile.rs`, `src/codex_lifecycle/direct.rs` (new), `src/codex_lifecycle/direct_tests.rs` (new, declared from direct.rs as `#[cfg(test)] #[path = "direct_tests.rs"] mod tests;`, the directory's convention at `progress.rs:110-111`, so the test path is `codex_lifecycle::direct::tests::`), `src/codex_lifecycle/test_support.rs`, `src/subagent_lifecycle/codex_evidence.rs`, `src/subagent_lifecycle/tests.rs`, `src/commands/subagents/wait/codex_binding.rs`, `src/commands/subagents/wait/mod.rs`, `src/commands/subagents/wait/direct_tests.rs` (new, declared from `wait/mod.rs` as `#[cfg(test)] mod direct_tests;`), `src/models/forward_receipt.rs` (the alias only).

Size: `reconcile.rs` is 387 lines (`reconcile_one` 43 of 50), `codex_binding.rs` 377, `codex_lifecycle/tests.rs` 290. Put the direct record builders and the direct classification in `direct.rs`; keep the `reconcile.rs` diff to about 10 lines (an early branch in `reconcile_one` plus the `correlated_identity` arm).

## Steps

1. **Read first**: `models/forward_receipt.rs` and its loader (`ReceiptLoad`, `get`, the `truncated`/`malformed` fields), `commands/hook/forward_receipt.rs` (the writer), `reconcile.rs` (`reconcile_one`, `worker_identity`, `authorization_record`, `observation_record`, `lifecycle_record` at about L222-305, `correlated_identity` at about L355-387), `subagent_lifecycle/codex_evidence.rs` (`validate_direct` L119-135, `validate_terminal_fields` L169-180), `wait/codex_binding.rs` (`receipt_matches` L256-264, `receipt_matches_authorization` L279-296, the `!truncated && malformed == 0` check L243-246).
2. **direct.rs**: `pub(super) fn receipt_outcome(row: &CodexAuthorization, receipts: &ReceiptLoad, now: DateTime<Utc>) -> DirectOutcome` (name pinned: the stage's wiring check greps `direct::receipt_outcome(` in reconcile.rs). Load receipts once per stage, not per row. A load with `truncated` or `malformed > 0` fails closed (`Unknown("forward receipt journal is damaged")`). Look the receipt up by the authorization's receipt id. Outcomes: a folded `backend: "direct"` receipt → `Direct(state)` with Succeeded → completed, Failed → failed (with `exit_code`), Canceled or TimedOut → cancelled, Running → active, Unknown or Queued → unknown (a receipt whose first observation is already terminal folds to Unknown, `forward_receipt.rs:276`). Only the folded `state` decides the outcome: a non-null `exit_code` never makes one terminal, because a conflicting history folds to Unknown without clearing the earlier exit code (`forward_receipt.rs:295-320`). A companion receipt → `Companion` (today's path); no receipt → `NoReceipt`.
3. **reconcile.rs**: per row, `receipt_outcome` first. `Direct` → append `LifecycleProducer::CodexDirect` records with `CodexExecution::Direct { thread_id: backend_id, tool_use_id }`, evidence `thread_id` and `tool_use_id` both `Some`, `job_id: None`; build the Running observation with `observed_at = authorized_at` exactly as the companion path does (a receipt-derived time would change between ticks and hit `Conflict` on the same event id); a terminal record has `terminal_at = receipt.observed_at`. `Companion` or `NoReceipt` → `jobs::locate_job` first; a job found → today's companion path. `NoReceipt` and no job → `Active` with a debug log and no append while `now < authorized_at + 600 s`, then `Unknown("no forward receipt 600 s after authorization")`. `correlated_identity` accepts `(CodexDirect, Direct)` beside `(CodexCompanion, Companion)`. The window constant is 600 s (the 540 s unit deadline plus slack); take `now` as a parameter so the 2026-10-23 fixture is not "expired" on every later day.
4. **codex_evidence.rs**: `turn_id` is required for `CodexCompanion` and optional for `CodexDirect` in `validate_terminal_fields`; every other check stays. Test in `subagent_lifecycle/tests.rs`: a terminal direct record without `turn_id` validates; a terminal companion record without one still fails.
5. **Watch**: a direct forward already lands in the Direct arm of the wait engine (`engine.rs:126-129`), which reads the journal; it needs no engine change. Make `receipt_matches_authorization` `pub(crate)` if `direct.rs` reuses it. Test in `wait/direct_tests.rs`: `direct_forward_watch_binding_resolves_terminal_after_reconcile` (after reconcile writes the direct records, binding finds exactly one execution and the Direct arm returns Succeeded). Do not change stall or progress handling.

## Tests (in `codex_lifecycle/direct_tests.rs`; each name starts `direct_forward_`; the stage requires at least 14)

- `direct_forward_running_receipt_is_live_not_unknown`
- `direct_forward_succeeded_receipt_writes_correlated_lifecycle` (also asserts `report.entries.iter().all(|e| !e.is_unknown())`, the monitor's warn predicate)
- `direct_forward_failed_receipt_is_failed_not_unknown`
- `direct_forward_receipt_for_other_session_is_ignored` (a receipt differing only in `loom_session_id`, then only in `parent_session_id`, `agent_id`, `tool_use_id`: none correlates)
- `direct_forward_shows_requested_model_in_execution_models` (through `reconcile_with_state_root` with an empty temp state root, then `execution_models_for_stage` contains `gpt-5.6-terra (requested)`, as `tests.rs:94` already calls it)
- `direct_forward_companion_rows_keep_companion_path`
- `direct_forward_live_companion_without_receipt_still_reconciles_from_job`
- `direct_forward_without_receipt_within_window_is_active`
- `direct_forward_without_receipt_past_window_is_unknown`
- `direct_forward_two_units_join_their_own_receipts`
- `direct_forward_damaged_receipt_journal_fails_closed`
- `direct_forward_cancelled_marker_word_yields_a_canceled_receipt`
- `direct_forward_conflicting_terminal_receipts_stay_unknown` (running, then failed with exit 2, then succeeded with exit 0: the loader folds to Unknown with an exit code still set; no terminal record is written and the entry is unknown)
- `direct_forward_terminal_without_running_stays_unknown` (a first observation already terminal folds to Unknown; nothing is appended)
- `direct_forward_without_receipt_or_job_writes_nothing` (no receipt and no job inside the window: `Active`, `lifecycle.jsonl` absent)

Your one check, at the end: `cargo test --lib codex_lifecycle::`. If the build fails in a file you do not own (W1 builds in parallel), report it and stop.

## Do not

Change `codex-forward.sh`, the receipt format (the serde alias reads an existing word; it writes nothing new), the wait engine, stall detection, or the web/status rendering. Add status fields.
