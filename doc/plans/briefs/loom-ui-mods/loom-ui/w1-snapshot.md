# W1: `loom hook ui-snapshot`

Tier: opus (security-sensitive identity check). Load the `loom-rust` skill. Read `common.md` first; it defines the JSON, the reason codes, the fixture and the working directory (`loom/`).

## Files you own

- `loom/src/commands/hook/ui_snapshot/mod.rs` (new): module docs, `pub fn ui_snapshot() -> anyhow::Result<()>` (CLI entry), re-exports.
- `loom/src/commands/hook/ui_snapshot/scope.rs` (new): `SnapshotScope` and the membership check.
- `loom/src/commands/hook/ui_snapshot/model.rs` (new): the serde structs and `fixture_snapshot()`.
- `loom/src/commands/hook/ui_snapshot/collect.rs` (new): gathering the in-stage fields.
- `loom/src/commands/hook/ui_snapshot/tests.rs` (new): unit tests, declared from `mod.rs` with `#[cfg(test)] mod tests;`.
- `loom/src/commands/hook/mod.rs`: add `pub mod ui_snapshot;` (alphabetical, after `pub(crate) mod target;`).
- `loom/src/cli/types_ops.rs`: add to `HookCommands`, after `ReviewHarvest`:

      /// Print a read-only JSON snapshot of this session's loom stage for the
      /// loom-ui Claude Code plugin. Not a user-facing command.
      #[command(hide = true)]
      UiSnapshot,

- The file holding `fn dispatch_hook` (today `loom/src/cli/dispatch.rs`; a sibling plan may have moved it, so find it with `rg -n 'fn dispatch_hook' loom/src/cli`, never by line): add `HookCommands::UiSnapshot => hook::ui_snapshot::ui_snapshot(),`. Do NOT touch `fn dispatch` or any other ledgered unit; no row of `loom/maintainability-baseline.txt` may change.
- `loom/src/commands/status/data/collector.rs`: `fn build_stage_summary` becomes `pub(crate)` with `#[rustfmt::skip]` on the line above it (the signature is 97 characters; with `pub(crate)` it passes rustfmt's 100-column width, rustfmt would break the parameters one per line and take the function from 49 to 53 lines, over the 50-line limit `loom/tests/maintainability/scanner.rs` enforces; `#[rustfmt::skip]` keeps it on one line, as `STATE_READ_DIRS` in `loom/src/sandbox/settings.rs` does). `fn load_plan_name` becomes `pub(crate)`. No other change; the file is at 391 of 400 lines.
- `loom/src/commands/status/data/sanitize.rs`: `sanitize_stage_summary` goes from `pub(super)` to `pub(crate)`. No other change.
- `loom/src/commands/status/data/mod.rs`: `mod collector;` and `mod sanitize;` are private, so visibility on the functions alone reaches nothing outside `data`. Add `pub(crate) use collector::{build_stage_summary, load_plan_name};` and `pub(crate) use sanitize::sanitize_stage_summary;` beside the existing `pub use` lines. No other change.

Read-only for you: `loom/tests/fixtures/loom_ui_snapshot_fixture.ts` (frozen harness), `loom/tests/loom_ui_contracts.rs` and `loom/tests/loom_ui_install_contracts.rs` (frozen contracts).

## Why `loom hook`

`loom/src/main.rs` exempts the `hook` subcommand from terminal recovery (`MACHINE_PROTOCOL_COMMANDS`, which would otherwise write ANSI bytes into stdout) and from the update check (`UPDATE_SILENT_COMMANDS`, which would spawn a refresh child on every poll). The plugin polls every 5 s, so the command lives under `loom hook`, and `main.rs` needs no change. `commands/hook/mod.rs` says no hook subcommand makes a model or network call; keep it so.

## Public surface (the contract tests call it; keep these names exactly)

    pub struct SnapshotScope {
        pub stage_id: String,
        pub session_id: String,
        pub work_dir: std::path::PathBuf,
        pub session_type: Option<String>,      // LOOM_SESSION_TYPE
        pub worktree_path: Option<std::path::PathBuf>, // LOOM_WORKTREE_PATH
        pub cwd: std::path::PathBuf,
    }
    impl SnapshotScope {
        /// None when LOOM_STAGE_ID, LOOM_SESSION_ID or LOOM_WORK_DIR is unset or empty.
        pub fn from_env() -> Option<SnapshotScope>;
    }
    /// Never fails: every failure becomes an out-of-stage snapshot with its reason code.
    pub fn collect_snapshot(scope: Option<&SnapshotScope>) -> UiSnapshot;
    pub fn fixture_snapshot() -> UiSnapshot;   // exactly common.md's fixture
    #[derive(serde::Serialize, Debug, Clone, PartialEq)] pub struct UiSnapshot { .. }

`ui_snapshot()` = `collect_snapshot(SnapshotScope::from_env().as_ref())`, then `writeln!` the `serde_json::to_string` result to a locked stdout, ignoring the write error (`println!` panics on EPIPE when the plugin's timeout kills the reader, which would break "always exits 0"), and return `Ok(())`. The command adds nothing to stderr itself; `list_all_stages` may print a warning per unparseable stage file, which the plugin ignores.

## Membership (scope.rs)

Mirror `scope_from_environment` in `loom/src/commands/subagents/wait/identity.rs` (lines 81-113 at planning time). That file sits at the 400-line cap and its function is private, so copy the logic into `scope.rs`; do not edit identity.rs. The order of checks and the reason codes are in common.md. Specifics:

- `is_safe_id` both ids BEFORE any path is built from them.
- Canonicalize `work_dir`; require a directory. Build the collector's `crate::fs::work_dir::WorkDir` with `WorkDir::new(&canonical_work_dir)`, the pattern `loom/src/commands/hook/worker_brief.rs` and `hook/target.rs` use.
- `load_session_exact`, then `session.stage_id.as_deref() == Some(stage_id)`; when `scope.session_type` is `Some(t)`, require `t == session.session_type.to_string()`, else `session-mismatch`.
- `load_stage`, then `stage.id == stage_id` (else `stage-missing`; `load_stage` matches by file name, and `build_stage_summary` joins `stage.id` into heartbeat paths, the hazard `loom/src/commands/hook/context_ceilings.rs` guards the same way), then `stage.session.as_deref() == Some(session_id)`.
- If the session RECORD is `SessionType::Knowledge`: require `canonicalize(scope.cwd)` to start with `work_dir.main_project_root()` (missing root is `outside-worktree`). Otherwise: canonicalize `session.worktree_path` (missing is `outside-worktree`), require `canonicalize(scope.cwd)` to start with it, and when `scope.worktree_path` is `Some`, require its canonical form to equal it.
- No check may be skipped because an env var is present. Presence is not membership.

## Collection (collect.rs)

- Build the stage's `StageSummary` through `build_stage_summary(&stage, std::slice::from_ref(&session), &work_dir)` with only this session in the slice, then `sanitize_stage_summary`. Take `attention` from `attention_entries(std::slice::from_ref(&summary)).into_iter().next()`. This path reads, and creates if absent, the completion attestation key, and runs one git `rev-parse` when a checkpoint carries a blocker (common.md "Security rules"); nothing else may write or run git.
- Reasoning effort: `StageSummary` has none. Use the effort half of the resolver that `collector.rs` `resolved_model` calls (`resolve_stage_model_effort`, which returns `(String, String)`); map an empty string to `null`.
- Dependencies: `load_stage` each id (cap 32, drop ids failing `is_safe_id`); dependents: `crate::verify::list_all_stages(work_dir)` filtered and sorted by id (cap 32). Do not call `collect_status_data`: it builds the git-backed merge report for every stage.
- Context tiers exactly as common.md (integer arithmetic for warn and red); reuse `resolve_context_ceiling_tokens` and `read_context_config` / `ContextConfig::backstop_tokens` (`loom/src/fs/work_dir/context_config.rs`); `loom/src/commands/hook/context_ceilings.rs` shows the fallback when the workspace `[context]` section is malformed. Copy it.
- `verification_status` and `review_gate` exactly as common.md (hand mapping; `verify::review::gate::covers`). Review-store reads that fail give zeros, never `read-error`.
- Flatten EVERY string from a stage, plan, session, review or ledger source with `inline_safe` (`loom/src/context/untrusted.rs`), including `stage.name`, the plan name, every acceptance command line, attention `label`/`note`/`command`, `review_reason`, blocker `summary`/`next_action`.
- No daemon socket, no network, no writes beyond the collector's attestation-key creation.
- Any `Err` from the stage, session, workspace config or stage-list reads after membership succeeded gives `read-error`.

## Size limits

`cargo test --test maintainability` scans `build/`, `src/` and `tests/`: every function at most 50 lines counted from the `fn` line, every file at most 400 lines, and the baseline ledger may not grow. Build `fixture_snapshot()` from per-section helpers (`stage_fixture()`, `context_fixture()`, `checklist_fixture()`, ...), each under 50 lines after rustfmt; keep every new file under 400 lines.

## Tests you write (tests.rs)

At least:

- tiers at a non-default ceiling (`context_ceiling_tokens = 200000` gives 120000 / 180000 / 250000);
- tiers at the default ceiling (800000 gives warn 480000, red 720000, backstop 950000, subagent 800000; this exercises the backstop clamp the 200000 contract cannot);
- a knowledge session (record `SessionType::Knowledge`) passes without a worktree when its cwd is inside the main project root;
- a non-knowledge session whose cwd is outside its worktree yields `outside-worktree`;
- (the env-only knowledge claim and the stage record whose `id` differs from its file name are frozen contracts, `ui-snapshot-rejects-env-knowledge-claim` and `ui-snapshot-rejects-stage-record-id-mismatch`; do not duplicate them);
- an executing stage whose `completion-attestation.key` path is a symlink to a file outside the work dir gives an in-stage snapshot with `checklist.completion_blocker` `null` (not `read-error`), and the link target is unchanged;
- an unsafe dependency id is dropped; dependents sorted and capped at 32; acceptance list capped at 64 with the true total;
- a malformed review round file gives zero review counts and an in-stage snapshot, not `read-error`;
- collecting an executing stage leaves the work dir's file set unchanged apart from `completion-attestation.key`;
- out-of-stage JSON carries exactly three keys.

Build fixture work dirs in a `TempDir` shaped like a real one: the work dir is `<tmp>/.loom/work` (created with `create_dir_all`; `WorkDir::new` on a bare TempDir resolves to a nonexistent `<tmp>/.loom/work` and every collector read silently comes back empty), stage records written with `crate::verify::save_stage` and session records with `crate::fs::session_files::save_session`, a non-knowledge session's `worktree_path` an existing directory `<tmp>/.worktrees/<stage-id>`. Never touch the real `.loom/work` (`doc/loom/knowledge/mistakes/live-state-pollution.md`). Never assert the resolved `model` or `reasoning_effort` values: `resolve_stage_model_effort` reads the developer's real `~/.loom/config.toml`.

## Your one check

From `loom/`: `cargo test --lib commands::hook::ui_snapshot::`

## Memory

Record surprises and decisions with `loom memory note` / `loom memory decision` as they happen; never Claude Code auto-memory.
