# Brief: plan-stats (record, publish, finish hook)

Tier: `loom-software-engineer` (sonnet). Wave 2 (after `phases` is on disk: `Stage.phases` exists). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`loom/src/plan_stats/mod.rs`, `record.rs`, `build.rs`, `publish.rs`, `tests.rs` (all new), `loom/src/lib.rs` (one `pub mod plan_stats;` line), `loom/src/daemon/server/orchestrator.rs`.

Frozen, read-only: `loom/src/plan_stats/contract_tests.rs`.

## Facts (verified)

- `finish_orchestration` (`daemon/server/orchestrator.rs:174-190`) is the single success seam for daemon and foreground runs (`finish_foreground` 192-205). `result.is_success()` is true when every stage is Completed or Skipped.
- Token totals: `tokens/checkpoint.rs` `Checkpoint { stages: BTreeMap<String, StageAgents>, .. }` in `.loom/work/tokens/checkpoint.json`; per-agent model and effort and usage in `tokens/accumulate.rs` (`AgentState.by_model`, :49, merged 233-249). `tokens::tracker::scan_pass(work_root, codex_root, now)` (`tokens/tracker.rs:61-76`) runs a synchronous pass and writes the checkpoint; the tracker thread stops at shutdown, so call it first. Find the `codex_root` the tracker uses with `loom map --find-all spawn_token_tracker`.
- `TokenUsage` (`tokens/model.rs:28`). A Codex agent is identifiable in the checkpoint (`unit_id` or kind); lane "codex" for those, "claude" otherwise.
- Stages: `verify::transitions::persistence::list_all_stages(work_dir)`; plan name and file from `plan_context` (orchestrator.rs:156-172) or the status collector's H1 reader (`status/data/collector.rs:332-339`).
- Loom home: `user_config::config_path()` (`user_config/mod.rs:157-175`, honours `LOOM_HOME` and the test redirect); its parent is the home.
- Atomic write helpers: `fs::cache_io::create_dir` (0700, :6-13), `fs::cache_io::atomic_write` (:28-58, no-follow temp, O_EXCL); bounded no-follow read `fs::safe_read::read_bounded`.

## Change

Implement the `plan_stats` surface exactly as the stage PUBLIC SURFACE pins it (types, `build`, `file_name`, `Published`, `publish`, `loom_home`, `write_current`, `read_current`, `finish_plan`). Details:

- `build`: wall time from the earliest `phases[0].started_at` to the latest `completed_at`; `execution_secs` is the sum of stage `execution_secs`; per-stage tokens and the model rows from the checkpoint, grouped by (model, effort, lane) with an agent count.
- `publish`: `create_dir` for `stats/plans`, then write with create-new semantics (a file that exists returns `AlreadyPublished` untouched). A symlinked `stats` or `plans` directory is refused.
- `read_current`: bounded read (256 KiB) of `<work_dir>/plan-stats.json`; unreadable or invalid -> None.
- `finish_orchestration`: on the success branch, after `mark_plan_done_if_all_merged`, call `plan_stats::finish_plan(work_dir)`; log an error with `tracing::error!` and continue; the `TerminalOutcome` never changes because of it.

## Tests (`tests.rs`)

`file_name` slugging (`IN_PROGRESS-PLAN-Foo bar.md` -> `...-plan-foo-bar.json`), `read_current` on a missing, oversized and symlinked file, `build` on a two-stage fixture work dir with a checkpoint (one claude and one codex agent).

## Done

`cargo test --manifest-path loom/Cargo.toml --lib plan_stats::` passes with the frozen contracts; `cargo build --manifest-path loom/Cargo.toml --all-targets` clean.

## Traps

- Never `dirs::home_dir()`; the contract redirects the config location.
- The record must deserialize back (`read_current`) with the same types the web schema mirrors: timestamps RFC 3339, phases as `{ phase, started_at, ended_at }`.
