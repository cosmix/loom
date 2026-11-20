# P1: the `provision` field, its executor and the spawn gate

Stage `plan-environment`, wave 1, tier sonnet. Read `../common.md` first. P2 (wave 2) reads
the field you add; P3 writes the docs in parallel.

You own `loom/src/plan/schema/{types.rs, types_v2.rs, mod.rs, validation/v2_fields.rs,
tests/v2_tests.rs}`, `loom/src/orchestrator/{provision.rs (new), provision_tests.rs (new),
mod.rs}`, `loom/src/orchestrator/core/{provision_gate.rs (new), provision_gate_tests.rs (new),
mod.rs, stage_executor.rs}` and `loom/src/commands/init/plan_setup.rs`.

## Why

A worktree is cut from git, so it has no `node_modules`. JS tests cannot run there, and a stage
session sandbox may not reach the npm registry. The settled design: a plan-level

    loom:
      provision:
        - working_dir: web
          command: bun install --frozen-lockfile

that the daemon runs on the host, in the stage's worktree, each time the stage leaves the queue
for a session (`start_stage`). A failure blocks the stage with the error as its reason.
Commands must be idempotent: a retry, a handoff successor or an adjudication requeue runs them
again. They may write only files git ignores (plan Choice 8).

The daemon never reads the entries from the plan file at spawn time. The plan file is
ordinary merged content: this plan's own sandbox grants `doc/**`, the merge gate
(`merge_handler/merge_gate.rs::is_control_path`) does not protect plan files, so a sandboxed
stage could add an entry that the daemon would then run on the host, outside the sandbox.
`loom init` snapshots the entries into the work directory's `config.toml` (section
`[plan_provision]`, beside the `[plan_sandbox]` snapshot, which exists for the same reason),
and the gate reads only that snapshot. The work directory is a control path. To change the
entries during a run, the operator edits `[plan_provision]` and runs `loom stage retry <id>`.

## 1. Schema (`version: 2` only)

- `plan/schema/types_v2.rs`:

      /// A command the daemon runs on the host, in `<worktree>/<working_dir>`, each time a
      /// stage leaves the queue for a session (plan `version: 2`). It must be idempotent and
      /// may write only files git ignores.
      #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
      #[serde(deny_unknown_fields)]
      pub struct ProvisionEntry {
          /// Directory to run in, relative to the repository root; no `..`.
          pub working_dir: String,
          /// Shell command, run with `sh -c`.
          pub command: String,
      }

- `plan/schema/types.rs` is ledgered at exactly 416 lines and must stay there (net-zero):
  - line 5 becomes `use super::types_v2::{deserialize_reasoning_effort, ProvisionEntry};`;
  - add to `LoomConfig`, after `ratchet_files`, exactly three lines:

        /// Commands the daemon runs on the host in each stage worktree before a session spawns.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub provision: Vec<ProvisionEntry>,

  - offset them by replacing the four-line comment above `pub use
    crate::models::stage::PermissionMode;` (lines 7-10) with the one line
    `/// Claude Code permission mode; defined in [`crate::models::stage::PermissionMode`].`
  - `wc -l` must still say 416. `cargo test --test maintainability` is in acceptance.
- `plan/schema/mod.rs`: `pub use types_v2::ProvisionEntry;` so it is
  `loom::plan::schema::ProvisionEntry`. `types_v2.rs`'s module doc says "Everything here is
  re-exported from `types.rs`"; rewrite it to say its items are re-exported through
  `crate::plan::schema` (`ProvisionEntry` directly from `mod.rs`).
- `plan/schema/validation/v2_fields.rs` (`validate()` calls `push_v2_field_errors`; do not
  touch `validation.rs`, whose `validate` is ledgered at 357 lines):
  - `push_v1_uses`: a non-empty `provision` on a v1 plan gives "`provision` requires
    `version: 2`", as `ratchet_files` does.
  - `push_v2_rules`: for entry `#n` (1-based), reuse `path_problem` for `working_dir`
    ("provision entry #n working_dir '<dir>' must be a relative path" / "cannot contain a `..`
    component"); an empty or whitespace `working_dir` gives "provision entry #n working_dir
    must not be empty" (use "." for the repository root); an empty command gives "provision
    entry #n has an empty command". Each message starts "provision".
- Tests in `plan/schema/tests/v2_tests.rs`: a v1 plan with provision; a v2 plan with `..`, an
  absolute path, an empty dir and an empty command; a valid entry passing. Existing assertion
  lines stay.

## 2. The executor: `loom/src/orchestrator/provision.rs`

Declare `pub mod provision;` in `orchestrator/mod.rs` (alphabetical among the `pub mod`s).

    /// How long one provision command may run.
    pub const PROVISION_TIMEOUT: Duration = Duration::from_secs(600);

    /// Run the plan's provision entries in `worktree`, in order, stopping at the first
    /// failure. `Err` is the stage's block reason.
    pub fn run_provision(entries: &[ProvisionEntry], worktree: &Path) -> Result<(), String>

For each entry:

1. `dir = worktree.join(&entry.working_dir)`. Canonicalize `worktree` and `dir`; a missing
   directory, or a canonical `dir` that does not start with the canonical worktree (a symlink
   pointing out), is an `Err` and nothing runs.
2. Run it the way `before_stage` checks run (`verify/before_after.rs::run_before_stage_checks`
   → `verify/goal_backward/truths.rs::verify_truth_checks`): on the host, in the daemon's
   process, through `crate::verify::criteria::run_spec_with_timeout(&CommandSpec::shell(..),
   Some(&dir), PROVISION_TIMEOUT, CommandConfinement::Confined)`. `Confined` keeps `HOME` and
   `PATH` (`process/environment.rs::STAGE_HOST_ENV_ALLOWLIST`, applied by
   `verify/criteria/confine.rs::apply_stage_environment`), so `bun` finds its cache.
3. A spawn error, a timeout or a non-zero exit is an `Err`.

The reason is exactly

    provision `<command>` in `<working_dir>` failed: <detail>

with `<working_dir>` as the plan wrote it, and `<detail>`: the last 10 non-blank lines of
stderr joined with `\n`; when stderr is blank, the last 10 non-blank lines of stdout; when both
are blank, `exit <code>` (or `killed by a signal`). Cut each kept line to its first 300
characters (on a char boundary), so the reason stays small in `loom status` and the stage
file. A timeout's detail is `timed out after 600 s`; a missing directory's is `the directory
does not exist in the worktree`; an escape's is `the directory resolves outside the worktree`.

The snapshot, in the same file:

    /// Section of the work directory's `config.toml` that holds the provision entries
    /// `loom init` copied from the plan. The gate reads only this, never the plan file.
    const PROVISION_SECTION: &str = "plan_provision";

    /// Persist the plan-level snapshots `loom init` takes: the sandbox (as today, through
    /// `crate::fs::work_dir::write_plan_sandbox`) and the provision entries.
    pub fn persist_plan_snapshots(work_dir: &Path, loom: &LoomConfig) -> Result<()>

    /// Replace the provision snapshot; no entries removes the section.
    pub fn write_provision_snapshot(work_dir: &Path, entries: &[ProvisionEntry]) -> Result<()>

    /// The snapshot's entries; a missing section is no entries.
    pub fn read_provision_snapshot(work_dir: &Path) -> Result<Vec<ProvisionEntry>>

Store the entries as `[[plan_provision.entries]]` tables (a wrapper struct
`{ entries: Vec<ProvisionEntry> }`, since a TOML section is a table). Write through
`crate::fs::work_dir::update_config` and read through `crate::fs::work_dir::read_config`, both
public; `fs/work_dir/config_sections.rs::write_section`/`read_section` are the pattern (they
are private, and `fs/work_dir.rs` is ledgered at 433 lines, so do not add re-exports there).

In `commands/init/plan_setup.rs::initialize_with_plan` (ledgered at 206 lines; keep it there),
replace the two lines

    work_dir::write_plan_sandbox(work_dir.root(), &parsed_plan.metadata.loom.sandbox)
        .context("Failed to persist plan-level sandbox config")?;

with

    persist_plan_snapshots(work_dir.root(), &parsed_plan.metadata.loom)
        .context("Failed to persist the plan-level sandbox and provision snapshots")?;

and add `use crate::orchestrator::provision::persist_plan_snapshots;` to the file's imports
(the file is 396 lines; one more is fine).

Unit tests in `provision_tests.rs` (`#[cfg(test)] #[path = "provision_tests.rs"] mod tests;`):
runs in the working dir, stops at the first failure, the stderr tail, the stdout fallback, the
300-character cut, a missing dir, a symlink escape, a snapshot that round-trips, and a
snapshot written with no entries reading back as none. Keep each file under 400 lines and each
function under 50.

## 3. The spawn gate

`orchestrator/core/stage_executor.rs` is ledgered (597 lines; `start_stage` 149;
`before_stage_gate_passed` 62). Keep every count unchanged:

- In `start_stage` (about lines 298-302) replace the two comment lines and the call
  `if !self.before_stage_gate_passed(&stage, &worktree.path, resolved.branch_name())? {` with
  two comment lines ("Run the before-stage checks on the pristine worktree, then provision
  it. Either one failing blocks the stage.") and `if !self.pre_spawn_gates_passed(&stage,
  &worktree.path, resolved.branch_name())? {` (under 100 columns, one line).
- `fn before_stage_gate_passed` becomes `pub(super) fn before_stage_gate_passed` (same line
  count).

New `orchestrator/core/provision_gate.rs`, declared `mod provision_gate;` in
`orchestrator/core/mod.rs`:

    impl Orchestrator {
        /// Run the stage's `before_stage` gate on the pristine worktree, then provision it.
        /// `Ok(false)` means the stage was blocked and must not spawn.
        pub(super) fn pre_spawn_gates_passed(&mut self, stage: &Stage, worktree_path: &Path,
            base_branch: &str) -> Result<bool>
    }

- First `self.before_stage_gate_passed(stage, worktree_path, base_branch)?`; on `false` return
  `Ok(false)` without provisioning. The order matters: `before_stage_gate_passed` skips its
  delta proof when `verify/before_after.rs::find_prior_stage_work` sees any uncommitted or
  untracked, non-ignored change in the worktree, so a provision that writes such a file (a
  lockfile, a `.venv`, an `*.egg-info`) before it would silently disable the check on the
  first attempt. Before-stage checks therefore run without provisioned dependencies.
- Read the entries with `crate::orchestrator::provision::read_provision_snapshot(
  &self.config.work_dir)`, never from the plan file (see "Why"). A snapshot that cannot be read
  blocks the stage with the reason `provision: cannot read the snapshot: <error:#>`.
- No entries: return `Ok(true)`.
- Otherwise print `Provisioning '<stage-id>' (<n> command(s))...` and call `run_provision`
  inside `std::thread::scope` with a second thread that calls
  `crate::orchestrator::tick::record(&self.config.work_dir, tick::Phase::Spawning)` every 10 s
  (checking an `AtomicBool` "done" flag every second, so it exits within a second of the
  provision finishing). The daemon's tick is stamped before `start_ready_stages`
  (`orchestrator/core/run.rs`) and a tick older than `tick::STALL_THRESHOLD_SECS` (60 s) makes
  `loom status` raise a critical "orchestrator loop stalled ... restart with `loom stop`"
  alert, which would tell the operator to kill the daemon mid-install.
- Around `run_provision`, list `git status --porcelain=v1 -z --untracked-files=all` in the
  worktree with `crate::git::runner::run_git_checked` (its hook suppression is harmless here),
  once before and once after. Every entry present only after is a file the install left that
  git does not ignore; any such entry is a failure with the reason
  `provision left files git does not ignore in the worktree: <up to 10 paths, comma-separated,
  then "and <n> more">; a provision command may write only files git ignores (add them to
  .gitignore or change the command)`. Comparing the two listings, instead of requiring an empty
  one, keeps a retry's own prior work out of it. Why: such a file counts as the contract
  session's own non-contract edit (`commands/stage/contracts/freeze.rs::checked_reports` refuses
  the freeze) and, on a retry, as prior stage work (`find_prior_stage_work` then skips the
  before-stage delta proof). When either listing fails (a plain TempDir, a worktree git cannot
  read), warn and skip the comparison, as `find_prior_stage_work` does. Keep the listing in a
  small private function of `provision_gate.rs`; `pre_spawn_gates_passed` stays under 50 lines.
- On `Err(reason)` from `run_provision` or the comparison, block the stage and return
  `Ok(false)`. Blocking mirrors
  `stage_executor.rs::persist_blocked_stage` (lines 18-34) and the before-stage failure path:
  in one `self.update_stage(..)`, `try_mark_blocked()`, set
  `failure_info = Some(FailureInfo { failure_type: FailureType::InfrastructureError,
  detected_at: Utc::now(), evidence: <the reason's lines> })` and `close_reason =
  Some(reason)`; then `self.graph.mark_status(id, StageStatus::Blocked)`. Setting
  `failure_info` keeps `loom status` from reading the block as an agent's
  (`InfrastructureError` is never auto-retried: `orchestrator/retry.rs:12-21`).
- The gate runs synchronously, like the before-stage gate, and only on the worktree spawn path.
  It runs for every stage that gets a worktree: a standard stage (before its contract session
  spawns), integration-verify and knowledge-distill. It does not run for knowledge-bootstrap
  (`start_stage` returns before it for knowledge stages), adjudication or merge-resolution
  sessions, or the contract phase's own spawns
  (`event_handler/contract_phase.rs::spawn_on_stage_worktree`: the contract-to-implementation
  handoff and a replacement contract session after `on_contract_session_ended`), which reuse
  the install the same `start_stage` made. That is safe because provision output is ignored
  files no session is expected to delete; a session that breaks them is recovered by
  `loom stage retry`, which requeues through the gate. A live session the executor adopts never
  reaches the gate. Do not edit `contract_phase.rs`.
- A `loom stop` during provisioning does not kill the command: the provision child runs in its
  own process group and finishes or hits its 600 s timeout. The stage is still Queued (it is
  marked Executing only after the gates), so the next `loom run` provisions again; the
  commands are idempotent. The doc comment of `pre_spawn_gates_passed` says so.

Tests in `provision_gate_tests.rs` (`#[cfg(test)] #[path = "provision_gate_tests.rs"] mod
tests;` inside `provision_gate.rs`). Model the orchestrator setup on
`stage_executor_tests.rs::work_dir`/`orchestrator_for` (copy the few lines; that file is not
yours). `work_dir()` pins the tmux backend by MERGING a `[terminal]` section into
`config.toml` (`write_terminal_config`); write the provision snapshot with
`write_provision_snapshot` (it merges too) and never overwrite `config.toml` with
`std::fs::write`, or `Orchestrator::new` runs real terminal detection and fails on a headless
runner. Save the stage with `status = StageStatus::Queued` (`WaitingForDeps -> Blocked` is
refused).

- `a_failing_provision_blocks_the_stage_with_its_reason` (exact name; acceptance runs
  `orchestrator::core::provision_gate::tests::a_failing_provision_blocks_the_stage_with_its_reason`):
  an entry `exit 3` in `.`; `pre_spawn_gates_passed` returns `Ok(false)`; the stage file is
  Blocked, `close_reason` starts "provision `exit 3` in `.` failed", `failure_info` is
  `InfrastructureError`.
- `a_failing_before_stage_check_blocks_before_provisioning` (exact name; acceptance runs it):
  a stage whose `before_stage` holds `false` with `exit_code: 0`, and a snapshot entry
  `touch provisioned` in `.`; `Ok(false)`, the stage is Blocked, and the worktree has no
  `provisioned` file. This proves the gate chains to the before-stage checks and runs them
  first. A plain (non-git) TempDir worktree counts as pristine: both git lookups in
  `find_prior_stage_work` fail, warn and fall through to `None`, so the checks run.
- `a_snapshot_without_entries_leaves_the_gate_to_before_stage`: `Ok(true)` and the stage is
  unchanged.
- `a_passing_provision_runs_in_the_worktree`: `touch provisioned` in `.` leaves the file in the
  worktree and returns `Ok(true)`.
- `a_provision_that_leaves_an_unignored_file_blocks_the_stage` (exact name; acceptance runs
  it): the worktree is a TempDir git repo (isolated git config, as
  `verify/impact_tests_tests.rs` sets it) with a committed `.gitignore` holding
  `node_modules/`; the snapshot entry `mkdir -p node_modules && touch node_modules/x && touch
  provisioned` in `.`; `Ok(false)`, the stage is Blocked with `failure_info`
  `InfrastructureError`, and `close_reason` names `provisioned`, contains "may write only files
  git ignores" and does not name `node_modules`.
- `a_provision_that_writes_only_ignored_files_spawns` (exact name; acceptance runs it): the
  same repo with the entry `mkdir -p node_modules && touch node_modules/x`; `Ok(true)` and the
  stage is unchanged.
- `the_gate_reads_the_snapshot_not_the_plan`: `config.toml` also carries `[plan] source_path`
  pointing at a v2 plan whose `provision` holds `touch from-plan`, and the snapshot holds
  `touch from-snapshot`; only `from-snapshot` appears.

## Contracts your code must satisfy

`provision-runs-in-its-working-dir`, `failed-provision-names-command-and-dir`,
`provision-refuses-a-symlinked-escape`, `provision-working-dir-cannot-escape` (scenarios in the
plan's YAML).

## Check

`cargo test --lib orchestrator::provision::`, once. P3 writes prose in parallel and does not
touch the crate; a compile error in a file you do not own is not yours (`common.md`).
