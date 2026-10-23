# Host Verification: shared brief

Every worker of `doc/plans/PLAN-host-verification.md` reads this file first, then its own brief. Paths are relative to `loom/` (the crate) unless they start with `../` or name the repository root. Re-read every file you edit at run time and anchor edits by symbol or text: line numbers here were measured on 2026-10-21 at `7aa927ac` and drift.

## 1. Why this plan exists

Claude Code 2.1.306 starts its Linux Bash sandbox with `bwrap --disable-userns`, so no command inside a session can create a user namespace. Loom ran three kinds of confined command inside sessions, each through `verify/criteria/confine.rs::prepare` → `confine_host.rs::wrap`, a nested bubblewrap on Linux:

- `loom stage complete <stage>` in a stage or knowledge session: acceptance, goal-backward commands, after_stage, contracts, impact tests (`commands/stage/complete_pipeline.rs::complete_normal_stage`, `knowledge_complete.rs::verify_knowledge_for_broker`);
- `loom check <stage>` in any session: the `loom-verifier`'s gate and every judge's first step (`commands/verify.rs::execute`; `orchestrator/adjudication/prompt/criterion.rs` tells the judge to run `loom check <stage> --no-cache`);
- `loom stage contracts freeze <stage>` in a contract session (`commands/stage/contracts/freeze.rs::checked_reports`, `run_contracts`, `check_formatting`).

Commit `d7ef6a8e` makes every Linux capsule carry `sandbox.allowNestedUserNamespaces: true` (`sandbox/settings/policy.rs::sandbox_settings`). Claude Code documents that key as letting a sandboxed command bypass `denyWrite`, `denyRead` and credential denies in any folder it may write. On macOS the same commands run unconfined inside the session's own Seatbelt, authorized by the `ValidateInheritedConfinement` RPC over a daemon socket the macOS capsule grants (`session_settings/contents.rs`, the `#[cfg(target_os = "macos")]` block; `daemon/inherited/`).

The daemon also trusts the session for completion today: the in-session run prints a `LOOM_CONTROL_EVIDENCE_V1` record, the PostToolUse hook `../loom-hooks/loom-control-complete.sh` hands it to the broker (`commands/stage/completion_evidence/broker.rs`), and the daemon checks identity, commit and policy bindings but takes every criterion's `passed` from the session (`daemon/server/completion_evidence.rs`, `control_complete.rs`).

This plan moves all three commands to the daemon. A session relays a request; the daemon runs the job as a child process on the host under loom's own confinement (bubblewrap on Linux, `sandbox-exec` on macOS, never nested); the session reads per-criterion progress and the result from the state directory; and the daemon completes a stage, or freezes contracts, only from its own run.

## 2. Facts every worker relies on

- **The orchestrator tick is single-threaded** (`orchestrator/core/run.rs::run_tick`). Nothing long may run inline on it: the runner starts children and polls them with `try_wait`.
- **A relayed request must settle in the pass that drains it.** `inbox_drain/session_pass.rs::load_ledger` settles any `Applying` record without an outcome as `UnknownAfterRestart` on the next pass. A drain handler that starts a job settles `Applied(Some("host job <id> ..."))` at once.
- **A Bash call that prints a relay line must exit 0.** Claude Code runs PostToolUse hooks, `loom-relay.sh` among them, only for a Bash call that exits 0. A non-zero exit raises `PostToolUseFailure`, which loom does not register. Session transcripts show it: 1,403 Bash results carrying `LOOM_RELAY_V1` that exited 0 all have a PostToolUse hook row, and the 15 that exited non-zero have none. The progress heartbeat (`post-tool-use.sh`) is PostToolUse-only too. The precedent `commands/stage/commit.rs::relay_commit` exits 0. Section 3.7's exit codes follow from this.
- **The relay hook decides which kinds a Bash call may write.** `../loom-hooks/loom-relay.sh::relay_kind_at` maps each loom invocation to the kind it writes and passes the set as `--allowed-kinds`; `commands/hook/relay.rs::admit` refuses any other kind ("did not run the loom command that writes … requests"). A new kind needs a `relay_kind_at` case and, for a control kind, a `drop_control_kinds` entry (`CONTRIBUTING.md`, "Hook Scripts"). `../loom-hooks/tests/loom-relay-kinds.sh` pins the mapping. The relay line must be the last stdout line of the call (`relay/emit.rs`).
- **Relay mechanics.** `relay/emit.rs`: `EnvSnapshot::from_process_env` (empty under `cfg(test)`), `mode(&env)` → `RelayMode::{Relay(RelayContext), Legacy, Operator}`, `RelayContext::check(kind, Some(stage), cwd, uid)` then `emit(kind, payload, what, end_turn, sink)` → `RelayLine { id, .. }`. Kinds: `relay/kind.rs::RequestKind` (wire names via `wire_name`, `all()` returns `[RequestKind; N]`, `is_control`). Payloads: `relay/payload.rs::{RequestPayload, decode_payload}`; `payload` is a private module, so every payload type outside `relay` is reached through the `pub use payload::{..}` line of `relay/mod.rs`. Per-session-type admission: `relay/matrix.rs::verdict`, which panics on a missing (session type, kind) row; there are six session types (`Stage`, `Merge`, `BaseConflict`, `Knowledge`, `Adjudication`, `Contract`). Daemon dispatch: `orchestrator/core/inbox_drain/apply.rs::{admit, apply}` (a control kind from a subagent is refused in `admit`), ownership `apply.rs::require_owner`. The drain's module root is `src/orchestrator/core/inbox_drain.rs`; there is no `inbox_drain/mod.rs`.
- **Exhaustive kind matches.** `inbox_drain/test_support.rs::payload_for` and `inbox_drain/tests_matrix.rs::classify_applied` match `RequestKind` with no `_` arm; `relay/tests_matrix.rs` and `inbox_drain/tests_matrix.rs` zip `RequestKind::all()` with fixed rows. A new kind is appended LAST in `all()`, gets an arm in both matches, and its column is tested in its own test beside the existing ones (as `COMMIT_COLUMN` is); the existing rows and their length assertions are never edited.
- **Sessions can read the state directory from Bash**; the native Read tool gets `Read(...)` allows for `sandbox/settings.rs::STATE_READ_DIRS`. Sessions cannot write it.
- **What a session's git sees that the host's does not.** Inside a Claude Code session on Linux, git lists the `/dev/null` mount points of `verify/tool_artifacts.rs::NAMES` (`.bashrc`, `.gitconfig`, `.mcp.json`, …) as untracked; on the host they exist only while a sandboxed command runs. Loom's own scaffold (`.claude/settings*.json`, the spools, `.loom/cache/`, a symlinked `CLAUDE.md` or `.loom/work`) is untracked in a project that does not ignore it. `handoff/completion/content.rs::completion_scaffold` is the one rule that discounts both, and `committed_content` scopes a knowledge stage to the knowledge directory (`crate::git::target_guard::knowledge_prefix()`). Raw `git status` is never compared across the session and the host.
- **Precedent for verification without cwd:** `commands/stage/complete_resolved.rs::verify_resolved_completion` (worktree from `git::worktree::stage_worktree_path(&stage, repo_root)`, acceptance dir from `acceptance_runner::resolve_acceptance_dir`, `VerificationSnapshot::capture`, `run_acceptance_phase`, `complete_verification::run`). A knowledge stage's tree is the repository root (`commands/stage/acceptance_runner.rs::resolve_knowledge_execution_paths`).
- **State root.** `commands::common::work_dir_path()` resolves the state root from the current directory only. A process the daemon spawns is given its state root in `LOOM_WORK_DIR` and its `current_dir` explicitly.
- **Process ownership:** `process/owned.rs::OwnedProcess::new(child)` wraps a spawned child; `retire` kills the whole group and reaps it, blocking at most `CLEANUP_GRACE` (1 s); `Drop` does the same. Spawn children with `process_group(0)`. The daemon is multi-threaded: use `std::process::Command` only, never a bare `fork` (`doc/loom/knowledge/mistakes/daemon-fork-after-threads.md`).
- **Daemon environment:** the daemon runs with `daemon/server/environment.rs::HOST_ENV_ALLOWLIST`; provision already runs host commands under it.
- **Bash timeout.** Claude Code kills a Bash call at 120 s unless the call asks for more (`BASH_DEFAULT_TIMEOUT_MS`); the session wrapper raises only `BASH_MAX_TIMEOUT_MS` (`orchestrator/terminal/native/wrapper.rs`). A session command that waits must return well inside 120 s.
- **Stall lines:** `orchestrator/monitor/stall_line.rs` (`STALL_ESCALATION_MULTIPLIER` 3, `COMPLETION_ACTIVITY`, `completion_line_secs`, `COMPLETION_MARGIN_SECS` 1800). The pinned `loom stage complete <id>` gets the completion heartbeat from the PreToolUse half of `../loom-hooks/loom-control-complete.sh`.
- **Size limits:** every Rust file at most 400 lines and every function at most 50 (`cargo test --test maintainability`). `maintainability-baseline.txt` is an EXACT ledger: a ledgered file or function that grows OR shrinks fails (`tests/maintainability/baseline.rs`), and it is a ratchet file of this plan. Neither stage changes it, so every edit to a ledgered item is line-neutral. Rows this plan's files carry: `commands/verify.rs::execute` 63, `commands/stage/skip_retry.rs::retry` 209, `orchestrator/core/event_handler.rs::handle_one_event` 116, `cli/dispatch.rs::dispatch` 69, `sandbox/settings.rs` 438 and its `generate_settings_json` 106, `sandbox/settings/tests.rs` 1565, `sandbox/settings/policy.rs::filesystem_settings` 52, `orchestrator/signals/cache.rs` 511 and its `generate_knowledge_distill_stable_prefix` 74, `orchestrator/signals/format/sections.rs` 569 and its `format_dynamic_section` 205, `format_semi_stable_section` 278, `format_recitation_section` 58, `orchestrator/signals/knowledge.rs::format_knowledge_signal_content` 77, `orchestrator/signals/format/helpers.rs::format_structured_handoff` 107. Files at the 400-line limit without a row (`orchestrator/core/orchestrator.rs` 399, `event_handler/stage_takedown.rs` 400) cannot grow. Add new code in new files.
- **`cargo test` takes one test-name filter.** `cargo test --lib a b` is rejected; run one filter per command or put several after `--`. Run every proof command as `set -o pipefail; <command> 2>&1 | tail -<n>`, so the pipeline reports cargo's status rather than `tail`'s, and confirm the output's `running <n> tests` line has `n > 0`: a filter that selects nothing passes vacuously.
- **A criterion is its own process group.** `verify/criteria/confine.rs::prepare` and `confine_host.rs::wrap` spawn every confined command with `process_group(0)`, so killing a parent's group does not reach it. On Linux bubblewrap's `--die-with-parent` ends a confined criterion when its parent dies; macOS `sandbox-exec` and `CommandConfinement::Inherit` have no such tie. Every confined spawn goes through `confine.rs::PreparedCommand::spawn_retaining_resources`.
- **Reading files another worker deletes.** Workers run in parallel. When your brief tells you to port code from a file another worker deletes, read it from the stage's base with `git show HEAD:loom/<path>` (read-only; the stage commits nothing until the end), never from the working tree.

## 3. Pinned interfaces

Stage `host-jobs` creates `src/host_verify/` (declared `pub mod host_verify;` in `src/lib.rs`, after `pub mod hooks;`). Stage `daemon-completion` extends it. The names, signatures, serialized shapes and exit codes below are fixed; contract tests call them.

### 3.0 Module layout

`src/host_verify/mod.rs` declares every module and every test module of the directory: `pub mod store; pub mod submit; pub mod runner; pub mod digest; pub mod wait; pub mod child;` and `#[cfg(test)] mod tests_store;` (likewise `tests_submit`, `tests_runner`, `tests_child`, `tests_digest`, `tests_wait`). No module file declares its own `#[path] mod tests`. A test therefore lives at `host_verify::tests_<x>::<name>`. Stage 2 adds `pub mod apply; pub mod freeze;` and `#[cfg(test)] mod tests_apply; #[cfg(test)] mod tests_freeze;`. The crate compiles once every worker of the stage has returned; a worker whose proof fails only on another worker's missing file reports that and stops.

### 3.1 Records (`src/host_verify/mod.rs`, owner W1)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind { Check }            // daemon-completion adds Complete and Freeze

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState { Queued, Running, Passed, Failed, Cancelled, Interrupted }
impl JobState { pub fn is_terminal(self) -> bool }   // Passed | Failed | Cancelled | Interrupted

pub fn new_job_id() -> String;                      // uuid v4, simple form, lowercase: 32 hex

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRecord {
    pub id: String,                    // 32 lowercase hex
    pub stage_id: String,
    pub session_id: String,            // the session whose request created the job
    pub kind: JobKind,
    pub head: String,                  // the HEAD the request named
    pub tree_digest: String,           // digest of the tree the request saw (3.5)
    pub no_cache: bool,
    pub state: JobState,
    pub requests: Vec<String>,         // relay request ids joined to this job, the last 16 kept
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub pid: Option<u32>,
    pub detail: Option<String>,        // why it was cancelled, interrupted or failed without a result; at most 512 chars, cut on a char boundary
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriterionOutcome {
    pub label: String,                 // "acceptance-<i>" (the 0-based [criterion i] index), "goal-backward"; daemon-completion adds others (4.2)
    pub passed: bool,
    pub summary: String,               // the printed block for this criterion, at most 8 KiB
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,           // file name, inside the job directory, of the criterion's full output
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobResult {
    pub job_id: String,
    pub head: String,                  // HEAD the child verified
    pub passed: bool,                  // false whenever error is Some
    pub criteria: Vec<CriterionOutcome>,
    pub error: Option<String>,         // a pipeline error that stopped the run, at most 2 KiB
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_hash: Option<String>,   // set by a Complete job (4.2); None for Check
}
```

### 3.2 Store (`src/host_verify/store.rs`, owner W1)

Layout under the state root (`.loom/work`):

- `verification/<stage-id>/current.json`: the stage's one current `JobRecord`;
- `verification/<stage-id>/.lock`: the stage lock (an `flock` on this file);
- `verification/<stage-id>/<job-id>/record.json`: a copy of the job's record, rewritten on every save, so a waiter can follow a job that is no longer current;
- `verification/<stage-id>/<job-id>/progress.jsonl`: one `CriterionOutcome` per line, appended as each finishes, capped at 2 MiB (past it, one final outcome `{label: "progress-truncated", passed: false}` and later appends are ignored);
- `verification/<stage-id>/<job-id>/result.json`;
- `verification/<stage-id>/<job-id>/child.log`: the child's stdout and stderr, capped at 4 MiB;
- `verification/<stage-id>/<job-id>/groups`: one decimal process-group id per line, appended by the child as it spawns each confined command (3.6), at most 4,096 lines (later appends are ignored);
- `verification/<stage-id>/<job-id>/<label>.log`: one per criterion that ran a command: its full stdout then stderr, capped so that the content plus the final `[truncated at 4 MiB]` line stays within `fs::safe_fs::MAX_LOG_BYTES` (4 MiB). The summaries are cut to the last 20 lines, so the per-criterion log is how a session reads a long failure whole;
- stage 2 adds `<job-id>/contract-reports.json` (4.3) and `<job-id>/applied` (4.4).

Queueing a job creates its directory. When a job is queued, the store removes job directories other than the three newest by mtime; it never removes the current job's directory.

Safety: `work_dir` may itself be a symlink (a worktree's `.loom/work`), so canonicalize it once, then open everything beneath it through `fs::safe_fs`: `safe_open_dirfd` on the canonical root, `open_safely` and the `safe_*_in_workdir` helpers (O_NOFOLLOW, dirfd-anchored) for every directory and file, `flock_exclusive` for the stage lock. A symlinked `verification`, stage directory, job directory or target file is refused before anything is created or locked. Never use `fs::locking::locked_dir_update` here: it creates its directory through symlinks. Every write is atomic (a temp file and a rename in the same directory).

```rust
pub const VERIFICATION_DIR: &str = "verification";
pub fn stage_dir(work_dir: &Path, stage_id: &str) -> Result<PathBuf>;   // crate::validation::validate_id(stage_id)
pub fn job_dir(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<PathBuf>; // job_id must be 32 lowercase hex
pub struct StageLock { /* the flocked fd; unlocks on drop */ }
pub fn lock_stage(work_dir: &Path, stage_id: &str) -> Result<StageLock>;
pub fn load_current(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>>;
pub fn save_current(work_dir: &Path, record: &JobRecord) -> Result<()>;   // also writes <job dir>/record.json, creating the job dir
pub fn load_job_record(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Option<JobRecord>>;
pub fn append_progress(work_dir: &Path, stage_id: &str, job_id: &str, outcome: &CriterionOutcome) -> Result<()>;
pub fn read_progress(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Vec<CriterionOutcome>>;
pub fn save_result(work_dir: &Path, stage_id: &str, result: &JobResult) -> Result<()>;
pub fn load_result(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Option<JobResult>>;
pub fn save_criterion_log(work_dir: &Path, stage_id: &str, job_id: &str, label: &str, bytes: &[u8]) -> Result<String>; // label [a-z0-9:_-]+; returns "<label>.log"
pub fn save_job_file(work_dir: &Path, stage_id: &str, job_id: &str, name: &str, bytes: &[u8]) -> Result<()>;     // name [a-z0-9._-]+, atomic
pub fn load_job_file(work_dir: &Path, stage_id: &str, job_id: &str, name: &str) -> Result<Option<Vec<u8>>>;
pub fn append_group(work_dir: &Path, stage_id: &str, job_id: &str, pgid: u32) -> Result<()>;
pub fn load_groups(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Vec<u32>>;       // missing file → empty
```

`save_current` and `save_result` are plain atomic writes; read-modify-write of `current.json` (submit, cancel, every runner transition) happens under `lock_stage`.

### 3.3 Submit and cancel (`src/host_verify/submit.rs`, owner W1)

```rust
pub struct SubmitRequest<'a> {
    pub stage_id: &'a str, pub session_id: &'a str, pub request_id: &'a str,
    pub kind: JobKind, pub head: &'a str, pub tree_digest: &'a str, pub no_cache: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission {
    Queued(JobRecord),
    Joined(JobRecord),
    Superseded { cancelled: JobRecord, queued: JobRecord },
    Refused(String),
}
pub fn submit(work_dir: &Path, request: &SubmitRequest<'_>, now: DateTime<Utc>) -> Result<Submission>;
pub fn cancel(work_dir: &Path, stage_id: &str, reason: &str) -> Result<Option<JobRecord>>;
```

Rules, under `store::lock_stage`. A non-terminal current record **covers** a request when it has the same `head` and `tree_digest`, `current.no_cache || !request.no_cache`, and either the same kind (for `Complete`, also the same `session_id`) or `current.kind == Complete` and `request.kind == Check`.

- No current record, or a terminal one: queue a new job (`Queued`).
- A non-terminal current record that covers the request: append `request_id` to its `requests` (the last 16 kept) and return `Joined`; never a second job.
- A non-terminal current `Complete` or `Freeze` record and a `Check` request it does not cover: `Refused("a completion or contract run is in progress for this stage; run the check again when it ends")`. A check never cancels a completion.
- Any other non-terminal current record: set it `Cancelled` with detail `superseded by <new id>`, queue the new job, return `Superseded`.
- `cancel` sets a non-terminal current record `Cancelled` with `detail = reason` and returns it; a terminal or missing record returns `Ok(None)`.

The record is the single source of truth. Whoever cancels only writes the state; the runner kills a child whose record is no longer `Running` on its next tick.

### 3.4 Runner (`src/host_verify/runner.rs`, owner W1)

```rust
pub struct RunnerConfig { pub program: PathBuf, pub args_prefix: Vec<String>, pub slots: usize }   // exactly these three fields
impl RunnerConfig { pub fn daemon_default() -> Result<Self> }   // current_exe(), ["stage", "verify-host"], slots 2
pub struct HostJobRunner { /* owns one OwnedProcess per running job */ }
impl HostJobRunner {
    pub fn new(work_dir: PathBuf, config: RunnerConfig) -> Self;
    pub fn with_deadline_override(self, deadline: Duration) -> Self;   // tests only use it; replaces job_deadline_secs
    pub fn tick(&mut self) -> Result<Vec<JobRecord>>;
}
pub fn job_deadline_secs(stage: &Stage) -> u64;
pub fn recover_after_restart(work_dir: &Path) -> Result<Vec<JobRecord>>;

// src/orchestrator/core/host_jobs.rs (owner W1; W5 adds the apply calls in stage 2)
impl Orchestrator {
    pub fn tick_host_jobs(&mut self);                          // run_tick calls it after drain_session_inboxes
    pub fn set_host_job_runner(&mut self, runner: HostJobRunner); // replaces the daemon_default runner; the e2e installs one whose program is the built loom binary
}
```

`tick_host_jobs` is `pub` like `drain_session_inboxes`, so the e2e drives the production tick, application included, instead of calling the runner and `apply_finished` by hand.

`job_deadline_secs`: the sum of every acceptance criterion's effective timeout (its `timeout_secs`, else 300), plus 300 for each wiring test, the dead-code check, the regression test, each frozen contract, one impact-test selection and each `after_stage` command, plus `stall_line::COMPLETION_MARGIN_SECS`. A Check job never runs more than its share, so one deadline per stage serves every kind.

`tick` makes every record change as a compare-and-set under `lock_stage`: it reloads `current.json` and applies a transition only when the record's `id` is the job's and its state is the one the transition expects. In order:

1. For each child it holds: a record that is no longer `Running` with this id (cancelled or superseded) → `OwnedProcess::retire` (kill and reap in this tick) and return the record as it stands. A job past its deadline (from `started_at`) → retire, set `Failed` with detail `deadline of <n> s passed`, return it.
2. Reap exited children. Only a record still `Running` with this id changes: with a `result.json` for the job → `Passed` when `result.passed`, else `Failed`; without one → `Failed` with detail `verify-host exited <status> without a result` and the last lines of `child.log`, cut to 512 chars. Return each.
3. Start queued jobs, oldest `queued_at` first, while fewer than `slots` run. Set `Running` and `started_at` BEFORE spawning, then spawn `<program> <args_prefix..> <stage_id> --job <job_id>` with `process_group(0)`, stdin null, stdout and stderr appended to `child.log`, `current_dir(WorkDir::new(work_dir)?.main_project_root())`, and the daemon's environment minus every `LOOM_SESSION*`, `LOOM_STAGE_ID`, `LOOM_SCRATCH_DIR`, `LOOM_WORKTREE_PATH`, `LOOM_SESSION_TYPE`, `LOOM_HOOK_CONTEXT`, `LOOM_MAIN_AGENT_PID`, `LOOM_MERGE_SESSION`, `LOOM_BIN` and `LOOM_HOOK_PATH`, plus `LOOM_WORK_DIR=<canonical state root>`. Then record `pid`. A spawn error sets `Failed` with the error as detail and returns it.

`child.log` over 4 MiB retires the child with detail `child output exceeded 4 MiB`. `tick` returns exactly the records of jobs it ended or reaped in this call.

**Retiring a job kills its criteria too.** Every path that stops a running child (a cancelled or superseded record, the deadline, the output cap, the graceful shutdown) first retires the child's own group, then sends SIGKILL to every process group `store::load_groups` lists for the job (`kill(-pgid, SIGKILL)`, `ESRCH` ignored). The child's group alone does not reach them (section 2). Deliberate limit: a listed group whose leader already exited and whose id the kernel reused within the same tick would be signalled; pid reuse needs a full pid wrap in that window. Revisit if a job ever outlives a pid wrap.

`recover_after_restart` runs once at daemon startup:

- Every `Running` record becomes `Interrupted` with detail `daemon restarted during the run`.
- For each whose `ps -o command= -p <pid>` (through `std::process::Command`) still shows `verify-host <stage_id> --job <job_id>`: SIGTERM to the process group. After all of them, one 2 s sleep, then SIGKILL to any still alive. Never signal a pid whose command line does not match.
- Then, for every interrupted job, SIGKILL each listed group whose leader still leads it (`ps -o pgid= -p <pgid>` prints `<pgid>`); a group whose leader is gone is left alone, because after a restart its id may belong to an unrelated process.
- `Queued` records stay queued.
- It returns the records it changed.

### 3.5 Tree digest (`src/host_verify/digest.rs`, owner W3)

```rust
pub fn tree_digest(checkout: &Path) -> Result<String>;             // 64 lowercase hex, the whole checkout
pub fn knowledge_tree_digest(repo_root: &Path) -> Result<String>;  // scoped to knowledge_prefix()
```

1. Run `git --no-optional-locks status --porcelain=v2 -z --untracked-files=all --no-renames` (for `knowledge_tree_digest`, with `-- <crate::git::target_guard::knowledge_prefix()>`) with `current_dir(checkout)`, `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` removed, and `GIT_CONFIG_GLOBAL=/dev/null` and `GIT_CONFIG_NOSYSTEM=1` set, so a session and the daemon read the same ignore rules.
2. Parse the entries: ordinary `1`, unmerged `u`, untracked `?` (`--no-renames` leaves no `2`; `!` does not appear). Drop every untracked entry `handoff::completion::content::completion_scaffold(path, checkout)` accepts (sandbox mount points and loom scaffold).
3. sha256 over, for each remaining entry in porcelain order: the entry's porcelain record bytes, a NUL, then one type byte and the current content framed by an 8-byte big-endian length: `F` + length + bytes for a regular file (refuse a file over 64 MiB with an error), `L` + length + target bytes for a symlink, `-` + eight zero bytes for a path that is absent or of another type.

A committed clean tree hashes the empty input.

### 3.6 Host child (`src/host_verify/child.rs` and `loom stage verify-host`, owner W2)

```rust
pub fn run_job(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<JobResult>;
```

CLI: `loom stage verify-host <stage-id> --job <job-id>`, hidden from help.

- It refuses to start, exiting 2, when `LOOM_SESSION_ID` or `LOOM_STAGE_ID` is set: it runs only from the daemon.
- Its state root is `LOOM_WORK_DIR`, required and canonicalized; unset or not a directory → exit 2. It never resolves the state root from its current directory.
- It loads the current record, which must be `Running` with this `id`; anything else is an error and writes no result.
- Before running anything it installs a spawn observer, `crate::verify::criteria::set_spawn_observer(SpawnObserver) -> SpawnObserverGuard` (one process-wide slot in a new `verify/criteria/spawn_observer.rs`, cleared when the guard drops; `confine.rs::PreparedCommand::spawn_retaining_resources` calls it with the new child's pid, which is its process-group id, right after each successful spawn; no observer, as in every other process, does nothing). The child's observer calls `store::append_group` for its job. Every confined command the child runs, in either stage, is recorded this way.
- It writes `result.json` itself.
- Exit codes: 0 passed, 1 failed, 2 refused or error. An error is also written as a result with `error: Some(..)` when the record could be loaded.

Check pipeline:

1. Load the stage (`verify::transitions::load_stage`). The tree is the stage worktree (`git::worktree::stage_worktree_path`), or the repository root for a knowledge stage (`acceptance_runner::resolve_knowledge_execution_paths`); the acceptance dir as `verify_resolved_completion` resolves it.
2. Require `git rev-parse HEAD == record.head` and the digest (3.5, `knowledge_tree_digest` for a knowledge stage) `== record.tree_digest`. A mismatch is an error naming both values.
3. Run acceptance (cache on unless `no_cache`), appending one `acceptance-<i>` outcome per criterion as each finishes. Before appending, write the criterion's full output with `store::save_criterion_log` and set `log`. A criterion answered from the cache has no output and no log.
4. Run the goal-backward checks when the stage has any (`commands/verify.rs::run_and_verify_stage_goal`), appending `goal-backward`. The goal checks live in the plan, found through `fs::resolve_source_path(work_dir)` (`config.toml`); a missing plan source is an error result, never a skip.
5. Re-read HEAD and recompute the digest. A HEAD that moved (a clean commit leaves the digest unchanged, so the digest alone misses it) or a changed digest turns the result into an error `worktree changed during the run` naming what changed. A Check changes no stage state, so it carries no policy binding; Complete binds the policy at apply (4.4).

### 3.7 Session side (`src/host_verify/wait.rs`, owner W3; reused by W4)

The session-side protocol for `loom check <stage>` in Relay mode. Stage `daemon-completion` applies it to `loom stage complete <stage>` with kind `complete`.

**First line and exit codes (fixed).** Every call prints, as its first stdout line, `Host job: <state> (<kind>, <short head>)`, and exits:

| `<state>` | Meaning | Exit |
| --- | --- | --- |
| `requested` | a ticket was emitted; nothing has run yet | 0 |
| `pending` | queued, running, or (complete) passed and not yet applied | 0 |
| `passed` | (check) every criterion passed | 0 |
| `completed` | (complete) the daemon completed the stage | 0 |
| `failed` | a criterion or check failed, or the run errored | 1 |
| `rejected` | (complete) the daemon refused to complete a passing run | 1 |
| `refused` | the request was refused, or never reached the daemon | 1 |
| `stopped` | the job was cancelled or interrupted | 4 |

`requested` and `pending` are not results: the agent runs the same command again in its next Bash call. Exit 0 is required for them because the relay hook and the heartbeat run only after a call that exits 0 (section 2).

1. The checkout is the stage worktree (`git::worktree::stage_worktree_path(&stage, repo_root)`) for every session type, judges included, or the repository root for a knowledge stage. Compute `head` (`git rev-parse HEAD` there) and the digest (3.5). `no_cache` is true when `LOOM_ACCEPTANCE_CACHE` is `0` (`cli/dispatch.rs::run_check` sets it for `--no-cache`).
2. Read the scratch state `$LOOM_SCRATCH_DIR/host-verify/<stage>-<kind>.json` (`{request_id, head, tree_digest, no_cache, submitted_at}`, mode 0600, written atomically: a temp file and a rename in the same directory). Two calls in one session (two verifier subagents) may share it: a call deletes the state only when it still holds the `request_id` this call read, so one waiter's terminal outcome never discards a newer request another waiter wrote. With no state, or a state whose `head`, `tree_digest` or `no_cache` differ from step 1:
   - write the state;
   - print `Host job: requested (<kind>, <short head>)` and `This is not a result. Run the same command again in your next Bash call to wait for it.`;
   - emit the relay ticket of the kind with payload `{head, tree_digest, no_cache}` and `end_turn: false`, so the `LOOM_RELAY_V1` line is the last stdout line;
   - exit 0.
3. With a matching state, find the job:
   - Read the session's inbox ledger (`fs::inbox::read_ledger(work_dir, session_id)`) for `request_id`. `Refused` or `UnknownAfterRestart` → print `Host job: refused (...)` and the reason, delete the state, exit 1. `Applied` names the job (`host job <id> ...`).
   - The job record is `current.json` when its `id` matches, else `<job dir>/record.json`. A job directory already pruned → `stopped`, exit 4.
   - No ledger outcome and no record naming `request_id` 120 s after `submitted_at` → `refused` with `the request never reached the daemon`, delete the state, exit 1. Before that, wait.
   - Queued or running: wait, printing each new progress outcome's `summary` as it appears.
   - Terminal: print every failed outcome's summary followed by `full log: <absolute path>` when it has a log, then one line per outcome `✓ <label>` or `✗ <label>`, then the error or detail if any. Delete the state and exit per the table (`Passed` → passed, `Failed` → failed, `Cancelled`/`Interrupted` → stopped, which also prints `Run the same command again to request a new run.`). Stage 2's `complete` keeps the state until it prints `completed`, `failed`, `rejected`, `refused` or `stopped`.
   - Still waiting when the budget ends: print `Host job: pending (<kind>, <short head>): <n> criteria done. Run the same command again to keep waiting.`, exit 0.
4. Wait budget per call: `min(100, 3 * effective_subagent_timeout_secs - 60)` seconds, at least 30. A disabled timeout (0) counts as 100. Poll every second.

In `Legacy` mode (`relay::emit::mode` returns `Legacy`: a session id without a relay scratch directory) `loom check` refuses, exiting 1, with `This session cannot relay verification (no LOOM_SCRATCH_DIR). Run loom check <stage> from the operator shell.` Outside a session (`Operator`) it runs locally as today.

## 4. Stage `daemon-completion` additions (pinned for W4-W7)

### 4.1 Kinds and payloads

- `JobKind` gains `Complete` and `Freeze`.
- `RequestKind` gains `Complete`, wire `complete`, a control kind, appended last in `all()`.
- `RequestKind::Check`, wire `check`, is added in stage `host-jobs` and is not a control kind.
- Payload `HostJobRequest { head: String, tree_digest: String, no_cache: bool }` (`#[serde(deny_unknown_fields)]`, in `relay/payload.rs`, re-exported from `relay/mod.rs`) is shared by `check` and `complete`.
- `StageRequest::FreezeContracts` carries `{ head: String, tree_digest: String }` and no reports. `StageRequest` is `#[serde(tag = "request", rename_all = "snake_case")]`. Decoding rejects unknown keys: a payload with `reports`, alone or beside `head` and `tree_digest`, fails to decode (container-level `#[serde(deny_unknown_fields)]` on `StageRequest` works with the internal tag; every producer serializes from the enum, so none sends an undeclared key).

### 4.2 Complete job (child)

Knowledge stage: snapshot plus acceptance only, in the repository root (`acceptance_runner::resolve_knowledge_execution_paths`).

Any other stage:

1. `VerificationSnapshot::capture` (the tree must be committed and clean, HEAD at the stage branch tip), then require `snapshot.commit == record.head`.
2. Run acceptance.
3. Run `complete_verification::run` with the daemon's gates and fail-closed detection: `VerificationChecks` replaces `control_session: Option<&str>` with `daemon_gates: bool` (skip integrity/review because the daemon checks them at apply) and `fail_closed: bool` (unwired/duplicate detection errors fail). The child sets both true; the operator's host path and `verify_resolved_completion` set both false.
4. Every failed Complete run of a v2 standard stage writes `reviews/<stage>/impact-observed.json` through `verify::impact_tests::observed::record_observed`: the failures when the impact selection failed, an empty `ObservedImpact` otherwise (the rule `completion_evidence/impact_record.rs::after_failed_completion` applies today).
5. `result.policy_hash = Some(snapshot.policy)`.

Outcome labels add `contracts`, `impact-tests`, `after-stage`, `unwired`, `duplicates`, `aggregated-wiring`; a failing check carries its message as `summary`.

### 4.3 Freeze job (child)

1. Run `contracts::freeze::check_changes`; require `git rev-parse HEAD == record.head` and the worktree digest to equal `record.tree_digest`.
2. Read the files the freeze will record (the contract and harness files, through the same code `stage_service/contracts.rs::prepare` reads them with, `read_files`, raised to `pub(crate)`) and hash each; compute `contracts_digest` = sha256 of `serde_json::to_vec(&stage.contracts)` (the effective contract definitions).
3. Run every contract through `judge_run`/`run_contracts`, and the format gate (`check_formatting`), on the host.
4. Re-read HEAD, recompute the digest, re-hash the files of step 2 and recompute `contracts_digest`; any change is an error `worktree changed during the run` naming what changed.
5. Write `contract-reports.json` = `{ "head": <head>, "tree_digest": <digest>, "contracts_digest": <hex>, "files": [{"path": <path>, "sha256": <hex>}, ..], "reports": [ContractRunReport, ..] }` with `store::save_job_file`, and return the `JobResult`. Each contract is `contract:<id>`, `passed` = the run classified it red as the freeze requires (`RED_OUTCOMES`); the format gate is `format`.

### 4.4 Apply (`src/host_verify/apply.rs`, owner W4)

```rust
pub enum Applied { Completed { commit: String }, Recorded, FreezeAccepted { files: usize }, FreezeRefused(String), Ignored(String) }
pub fn apply_finished(work_dir: &Path, record: &JobRecord) -> Result<Applied>;
```

The orchestrator calls this for every record `HostJobRunner::tick` returns, for every record `recover_after_restart` returns, and, at startup, for every terminal current record whose job directory has no `applied` marker. After applying, it writes the `applied` marker (`store::save_job_file(.., "applied", ..)`); a marked job is never applied twice: `apply_finished` returns `Ignored("already applied")` for it.

Durability:

- A binding or ownership failure is an `Ok` outcome (`Recorded` or `Ignored`), never `Err`. `Err` means an I/O or state-file failure that a later attempt may not hit.
- The orchestrator keeps every record whose `apply_finished` returned `Err` in memory and retries it on each later tick, up to 10 attempts; after the tenth it writes the marker with the content `apply failed: <error>` and logs at `error`. A crash loses the in-memory list; the startup scan of unmarked terminal records recovers it.
- Every step is safe to repeat after a crash between the transition and the marker. A Complete job whose stage is already `Completed` with `completed_commit == result.head` skips steps 2-5: it records the attempt and receipt (step 6) only when the latest checkpoint carries no `AcceptedReceipt` with this job id, retires the session (step 7) only when its record is not yet marked, writes the marker and returns `Ignored("already completed")`. A Freeze job whose stage is already frozen returns `Ignored("already frozen")` (4.6).
- The session's scratch state for `complete` survives until it prints `completed`, `failed`, `rejected`, `refused` or `stopped` (3.7), so a waiter whose budget ends before the application keeps following the same job.

- **Check:** `Ignored`.
- **Complete, Passed:**
  1. Load the result.
  2. Run `validate_active_identity`. It moves to this module from `daemon/server/control_complete.rs`: stage `Executing`, session owns the stage as Stage or Knowledge.
  3. Run `crate::daemon::check_completion_gates(work_dir, stage_id)` (W6 makes it reachable: `pub(crate)` in `daemon/server/observer.rs`, re-exported from `daemon/server/mod.rs` and `daemon/mod.rs`).
  4. Under the sessions-directory lock, in `update_stage`, require:
     - status `Executing`;
     - the same session;
     - `committed_content(stage, repo_root) == result.head`;
     - the current `effective_policy_hash` equals `result.policy_hash`.
  5. Set `completed_commit`; `merged = true` for a knowledge stage. Then `try_complete(None)`.
  6. Record the job as an attempt first, then the receipt: `record_attempt_handoff` a `VerifiedPendingAck` attempt (`evidence_nonce` = job id, every outcome as a passed `CriterionResult`), then `crate::handoff::record_accepted_handoff` an `AcceptedReceipt` (`evidence_nonce` = job id, a fresh `completion_nonce`, `commit`). `record_accepted_handoff` fails without a checkpoint, and `record_accepted` refuses a receipt whose nonce no observation carries.
  7. Retire the session as `retire_completed_session` does today (mark the record and remove the signal; the monitor then kills the session and merges, `orchestrator/core/completion_handler.rs`).
  8. Return `Completed`. Any binding that fails returns `Recorded` after writing a `DaemonRejected` attempt through the boundary-failure helper today's broker uses (`commands/stage/completion_evidence.rs::with_boundary_failure`, failure code `daemon_rejected`, ported into `host_verify`), naming the failed binding as its diagnostic.
- **Complete, Failed:** when `record.session_id` no longer owns the stage as Stage or Knowledge (handed off, retired, reset), return `Ignored("session no longer owns the stage")` and write nothing: the successor runs its own completion. Otherwise write a completion checkpoint through `handoff::completion::record::record_attempt_handoff`, then return `Recorded`. The checkpoint has:
  - phase `ToolFailed`;
  - `verification.criteria` = one `CriterionResult { id: label, passed }` per outcome (so `stage_service/control_block.rs` still refuses a block while an `acceptance-<i>` fails);
  - `evidence_nonce` = job id;
  - `exact_command` = `loom stage complete <stage>`;
  - `diagnostic_first_line` from the first failed outcome or the detail.
- **Complete, Cancelled/Interrupted:** `Recorded`, no checkpoint. A cancelled run says nothing about the criteria, would overwrite the `latest` attempt `control_block` reads, and would spend one of the 32 evidence nonces (`MAX_EVIDENCE_NONCES`).
- **Freeze (any state):** `crate::host_verify::freeze::apply_freeze_result(work_dir, record)` (4.6).

### 4.5 What the session prints and what the doctrine says

Every refusal below, and the `Legacy` refusal of `loom check` in 3.7, prints `Host job: refused (<kind>, <short head>)` as its first line, then its message, and exits 1.

- `loom stage complete <stage>` in a session:
  - In `Legacy` mode refuses, exiting 1, with `This session cannot relay completion (no LOOM_SCRATCH_DIR). Run loom stage complete <stage> from the operator shell, or loom stage retry <stage> for a session that can.` The broker route that served such sessions is gone. With `LOOM_SESSION_ID` set, every mode other than `Relay` is `Legacy` (the `LOOM_CONTROL_BROKER` override goes with the broker).
  - Refuses `--no-verify`, `--force-unsafe` and `--assume-merged` with the existing message for sandboxed privileged flags.
  - Refuses at once, exiting 1, with `Commit first: completion verifies committed content only.` and the error when `handoff::completion::content::committed_content(&stage, repo_root)` fails. That check discounts sandbox mount points and scaffold and scopes a knowledge stage to its knowledge directory; raw `git status --porcelain` is never used.
  - Otherwise follows 3.7 with kind `complete`. A passed job not yet applied is `pending`. The stage `Completed` → `completed` plus `Stage '<stage>' was completed by the daemon. End the session now.`. A `DaemonRejected` attempt whose `evidence_nonce` is the job id (read with `load_session_checkpoint`: sessions cannot read the attestation key) → `rejected` with its diagnostic.
- `loom check <stage>` in a session follows 3.7 with kind `check`.
- `loom stage contracts freeze <stage>`:
  1. Runs `check_changes` locally (git only) and refuses with today's text on problems.
  2. Computes `head` and the digest.
  3. In a contract session (`Relay`), relays `freeze-contracts` with `{head, tree_digest}` and `end_turn: true`. Outside a session (`Operator`), sends `Request::FreezeContracts { head, tree_digest, .. }` over the socket, or queues it on the worktree spool when the daemon is unreachable, as `send_freeze`/`queue_freeze` do today. In `Legacy` it refuses like `loom stage complete`.
  4. Prints `Contract run requested. End your turn now: the daemon runs the contracts on the host, then freezes them and starts the implementation session, or parks the stage with the refusal.`
  5. Exits 0. No CLI path runs contracts any more.
- While a stage has a non-terminal host job of any kind, the stall monitor does not escalate that stage's sessions. A `SessionHung` warning may still be logged; escalation waits until the job is terminal or past `job_deadline_secs` from its `started_at`.
- Outside a session (operator shell, no `LOOM_SESSION_ID`), `loom check` and `loom stage complete` keep today's local behaviour; the freeze queues a host job as above.

### 4.6 Freeze seams between W4 and W5

W5 writes `src/host_verify/freeze.rs` with:

```rust
pub fn run_freeze_job(work_dir: &Path, record: &JobRecord) -> Result<JobResult>;      // called from child.rs (W4) for JobKind::Freeze
pub fn apply_freeze_result(work_dir: &Path, record: &JobRecord) -> Result<Applied>;   // called from apply.rs (W4) for JobKind::Freeze
```

`apply_freeze_result`:

- A stage whose contracts are already frozen (`store::load_freeze` is `Some`): `Ignored("already frozen")`, never a park. A repeated application after a crash, or a stale job, must not park a frozen stage.
- `Passed`: load `contract-reports.json`. Park unless all three still hold: the worktree HEAD equals its `head`, the worktree digest equals its `tree_digest`, and `contracts_digest` of the stage as loaded now equals its `contracts_digest` (a policy amendment keeping a contract id must not pair an old run with a new command). Else call `stage_service::contracts::freeze_with_reports(work_dir, stage_id, &record.session_id, &reports, &files)`, where `files` is the report's `[(path, sha256)]`. `freeze_with_reports` runs `prepare` unchanged otherwise (it revalidates that the session is still the stage's contract writer) and refuses with `Error` when any file it reads hashes differently from `files` or the set of paths differs. The freeze record is built from those same bytes, so it records exactly what the run verified. `ContractsFrozen { files }` → `FreezeAccepted { files }`; `Error { message }` → park and `FreezeRefused(message)`.
- `Failed` or `Interrupted`: park with a message listing each failing outcome's label and summary, or the detail (at most 4 KiB), and return `FreezeRefused`.
- `Cancelled`: `Ignored("cancelled: <detail>")`, no park. Whoever cancelled (a superseding freeze, takedown, reset, retry, worktree removal) owns the stage's next state; parking a superseded job would refuse the freeze that replaced it.
- Parking is `verify::contracts::refusal::park_refused_relay(work_dir, stage, session, message)`.

W5 changes the daemon's freeze entry point. It is re-exported as `crate::daemon::handle_freeze_contracts` and defined in `src/daemon/server/contracts.rs` over `src/stage_service/contracts.rs`. The new signature is `handle_freeze_contracts(work_dir: &Path, stage_id: &str, session_id: &str, head: &str, tree_digest: &str, request_id: &str) -> Result<Response>`.

- It runs today's admission checks (stage in its contract phase, the caller its contract writer: read `prepare`) and queues a `Freeze` host job through `submit::submit`.
- It returns `Response::Ok` (queued or joined) or `Response::Error { message }`.
- Today's report-taking body becomes `stage_service::contracts::freeze_with_reports(work_dir, stage_id, session_id, reports: &[ContractRunReport], verified: &[(String, String)]) -> Result<Response>`, which only `apply_freeze_result` calls; `verified` is the `(path, sha256)` list the run hashed (4.3).

The other two routes to it follow the same signature: the worktree spool (`src/fs/stage_request/apply.rs`, W5) and the socket (`src/daemon/server/stage_control.rs`, W5). `Request::FreezeContracts` in `src/daemon/protocol.rs` replaces its `reports` field with `head: String, tree_digest: String`; W6 makes that edit, because it owns `protocol.rs`. The socket and spool routes pass a fresh `new_job_id()` as the request id.

W4's `inbox_drain/apply.rs::stage_request` arm for `StageRequest::FreezeContracts { head, tree_digest }` calls the new entry point with the relay request id. It settles `Applied(Some("contract run queued: host job <id>"))` on `Ok`, and `Refused` plus `park_refused_freeze` (the private helper in `inbox_drain/apply.rs`) on `Error`, as today.

## 5. Memory and conduct

Record mistakes, decisions and surprises with `loom memory note` / `loom memory decision` as they happen, including `stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line). Correction: <text>` for any knowledge the tree contradicts. Never Claude Code auto-memory, never `loom knowledge` writes. Never run git commands that change state (`git show HEAD:<path>` to read a deleted file is fine), never `loom stage complete`. No attribution to any AI system in code, comments or docs (CLAUDE.md Rule 9); naming Claude Code as the session runtime loom drives is fine where loom's text already does.

Tests that start loom's confinement (the host child running real criteria: `tests_child`, `tests_freeze`, `tests/host_verification_e2e.rs`, and the contracts `verify_host_records_each_criterion` and `cancel_stops_the_criteria_of_a_running_job`) open with `if !loom::process::sandbox_probe::skip_unless(loom::process::sandbox_probe::confined_commands_runnable(), "<test name>", "loom's confinement cannot start here") { return; }` (inside the crate, `crate::process::sandbox_probe`), as `tests/integration/relay_commit_e2e.rs` does. They run on the host and in this plan's sessions; they skip in a session that cannot nest.

No test or source file under `src/` or `../loom-hooks/` spells the literals `allowNestedUserNamespaces` or `LOOM_CONTROL_EVIDENCE` after stage 2: a test asserting their absence builds them with `concat!` (for example `concat!("allowNested", "UserNamespaces")`), because the stage gates grep for them.
