# W4: direct-mode codex forwards in lifecycle reconcile and status

Tier: sonnet (loom-software-engineer). Independent of W1-W3; you need not read their common.md. Read `doc/loom/knowledge/mistakes/codex-direct-mode-invisible.md` and the plan's Decision 10 first.

## The defect

Since commit 27a62e80, `loom-hooks/codex-forward.sh` (about L247-253) sets `mode=direct` when `nested_bubblewrap_refused` (every Linux stage sandbox) or `nested_seatbelt_refused`, and runs `codex exec` through `_codex-direct.py`. No companion job exists under `~/.codex/plugin-data/state/<workspace>-<hash>/jobs`. The daemon's `codex_lifecycle::reconcile_codex_jobs` (`src/codex_lifecycle.rs:46`, `reconcile.rs:22` `reconcile_with_state_root`) only locates companion jobs (`jobs::locate_job`), so every direct forward is reported Unknown every monitor tick (`orchestrator/monitor/core.rs:40-69` warns each time), `lifecycle.jsonl` is never written (`reconcile.rs:217-218` is its only writer), and `status/data/execution_models.rs:85-93` `correlated_requested_model` drops the model because `has_correlated_lifecycle` (`reconcile.rs:87-111`) is false.

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

The authorization's `forwarder_agent_id`/`tool_use_id`/`session_id` equal the receipt's `agent_id`/`tool_use_id`/`loom_session_id`.

## Files you own

`src/codex_lifecycle.rs`, `src/codex_lifecycle/reconcile.rs`, `src/codex_lifecycle/direct.rs` (new), `src/codex_lifecycle/tests.rs`, `src/codex_lifecycle/test_support.rs`, `src/commands/subagents/wait/engine.rs`, `src/commands/subagents/wait/codex_binding.rs`, `src/commands/subagents/wait/stall.rs`, `src/commands/status/data/execution_models.rs`.

## Steps

1. Read the receipt model first: `src/models/forward_receipt.rs` and `load.rs` (the reader and its safety checks), `src/commands/hook/forward_receipt.rs` (the writer). Reuse the existing loader; do not parse the file a second way.
2. `direct.rs`: for one `CodexAuthorization`, find its receipts (match per the line above, plus stage id) and classify: no receipt yet → not Unknown, "awaiting forward receipt" (the forward may not have started); latest `backend: "direct"` and `running` → live; `succeeded`/`failed` (or a non-null `exit_code`) → terminal with that outcome; `backend` other than `direct` → `None` (companion path applies). Map onto the existing `WorkerOutcome` / reconcile entry types; read `reconcile.rs` and `jobs.rs` to find them, and add no parallel type.
3. `reconcile.rs`: before locating a companion job for a row, try `direct.rs`; on a direct classification append the authorization and observation records to the lifecycle journal exactly as the companion branch does at L217-218, so `has_correlated_lifecycle` holds. A direct row never produces "reading companion directory" Unknown.
4. `companion_outcome` / `companion_outcome_with_progress` (`codex_lifecycle.rs:55-86`) and their callers in `commands/subagents/wait/` (`engine.rs:118`, `codex_binding.rs`, `stall.rs:69`): a direct forward gets its outcome and progress from the receipt (progress = the receipt's `observed_at`), so `loom subagents watch` and hung-worker detection treat it like a companion job. Read each caller before changing it; keep the 540 s unit deadline behaviour.
5. `execution_models.rs`: no logic change should be needed once step 3 lands. Add a test that builds the fixture ledgers above under a temp work dir, runs reconcile (through the state-root-injected `reconcile_with_state_root` with an empty temp state root), and asserts `execution_models_for_stage` contains `gpt-5.6-terra (requested)`.

## Tests (each name contains `direct_forward`, the stage's acceptance filter selects them)

- `direct_forward_running_receipt_is_live_not_unknown`
- `direct_forward_succeeded_receipt_writes_correlated_lifecycle`
- `direct_forward_receipt_for_other_session_is_ignored` (a receipt whose `loom_session_id` differs does not correlate)
- `direct_forward_shows_requested_model_in_execution_models`
- `direct_forward_companion_rows_keep_companion_path` (a `backend` other than `direct` still goes through `jobs::locate_job`)

Run `cargo test --lib codex_lifecycle::`, `cargo test --lib commands::subagents::wait::`, `cargo test --lib direct_forward`, `sh ../loom-hooks/tests/codex-forward-direct.sh`, then `cargo clippy --all-targets -- -D warnings`, once each at the end.

## Do not

Change `codex-forward.sh`'s mode selection, the receipt format, or the web/status rendering. Add status fields.
