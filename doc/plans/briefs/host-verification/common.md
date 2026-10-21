# Host Verification: shared brief

Every worker of `doc/plans/PLAN-host-verification.md` reads this file first, then its own brief. Paths are relative to `loom/` (the crate) unless they start with `../` or name the repository root. Re-read every file you edit at run time and anchor edits by symbol or text: line numbers here were measured on 2026-10-21 at `145d32ac` and drift.

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
- **Relay mechanics.** `relay/emit.rs`: `EnvSnapshot::from_process_env` (returns an empty snapshot under `cfg(test)`), `mode(&env)` → `RelayMode::{Relay(RelayContext), Legacy, Operator}`, `RelayContext::check(kind, Some(stage), cwd, uid)` then `emit(kind, payload, what, end_turn, sink)` → `RelayLine { id, .. }`. Precedent: `commands/stage/commit.rs::relay_commit`. Kinds: `relay/kind.rs::RequestKind` (wire names via `wire_name`, `all()` length, `is_control`), payloads `relay/payload.rs::{RequestPayload, decode_payload}`, per-session-type admission `relay/matrix.rs::verdict`, daemon dispatch `orchestrator/core/inbox_drain/apply.rs::{admit, apply}` (a control kind from a subagent is refused in `admit`), ownership `apply.rs::require_owner`.
- **Sessions can read the state directory from Bash**; the native Read tool gets `Read(...)` allows for `sandbox/settings.rs::STATE_READ_DIRS`. Sessions cannot write it.
- **Precedent for verification without cwd:** `commands/stage/complete_resolved.rs::verify_resolved_completion` (worktree from `git::worktree::stage_worktree_path`, acceptance dir from `acceptance_runner::resolve_acceptance_dir`, `VerificationSnapshot::capture`, `run_acceptance_phase`, `complete_verification::run`).
- **Process ownership:** `process/owned.rs::OwnedProcess` kills the whole group on `retire` and on `Drop`; spawn children with `process_group(0)`. The daemon is multi-threaded: use `std::process::Command` only, never a bare `fork` (`doc/loom/knowledge/mistakes/daemon-fork-after-threads.md`).
- **Daemon environment:** the daemon runs with `daemon/server/environment.rs::HOST_ENV_ALLOWLIST`; provision already runs host commands under it.
- **Stall lines:** `orchestrator/monitor/stall_line.rs` (`STALL_ESCALATION_MULTIPLIER` 3, `COMPLETION_ACTIVITY`, `completion_line_secs`). The pinned `loom stage complete <id>` gets the completion heartbeat from the PreToolUse half of `../loom-hooks/loom-control-complete.sh`.
- **Size limits:** every Rust file at most 400 lines and every function at most 50 (`cargo test --test maintainability`, ledger `maintainability-baseline.txt`, a ratchet file of this plan). Add new code in new files rather than growing a ledgered one.

## 3. Pinned interfaces

Stage `host-jobs` creates `src/host_verify/` (declared `pub mod host_verify;` in `src/lib.rs`, after `pub mod hooks;`). Stage `daemon-completion` extends it. The names, signatures and serialized shapes below are fixed; contract tests call them.

### 3.1 Records (`src/host_verify/mod.rs`, owner W1)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind { Check }            // daemon-completion adds Complete and Freeze

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState { Queued, Running, Passed, Failed, Cancelled, Interrupted }
impl JobState { pub fn is_terminal(self) -> bool }   // Passed | Failed | Cancelled | Interrupted

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRecord {
    pub id: String,                    // 32 lowercase hex (uuid simple)
    pub stage_id: String,
    pub session_id: String,            // the session whose request created the job
    pub kind: JobKind,
    pub head: String,                  // the HEAD the request named
    pub tree_digest: String,           // digest::tree_digest of the tree the request saw
    pub no_cache: bool,
    pub state: JobState,
    pub requests: Vec<String>,         // relay request ids joined to this job, at most 16 kept
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub pid: Option<u32>,
    pub detail: Option<String>,        // why it was cancelled, interrupted or failed without a result; at most 512 chars
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CriterionOutcome {
    pub label: String,                 // "acceptance-<i>" (the 0-based [criterion i] index), "goal-backward"; daemon-completion adds others (4.4)
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
}
```

### 3.2 Store (`src/host_verify/store.rs`, owner W1)

Layout under the state root (`.loom/work`): `verification/<stage-id>/current.json` (the stage's one current `JobRecord`), `verification/<stage-id>/<job-id>/progress.jsonl` (one `CriterionOutcome` per line, appended as each finishes, capped at 2 MiB), `verification/<stage-id>/<job-id>/result.json`, `verification/<stage-id>/<job-id>/child.log` (the child's stdout and stderr, capped at 4 MiB), and `verification/<stage-id>/<job-id>/<label>.log` (one per criterion that ran a command: its full stdout then stderr, capped at 4 MiB with a final truncation line). The summaries are cut to the last 20 lines, so the per-criterion log is how a session reads a long failure whole. The store keeps the current job's directory and the two before it, and removes older job directories when a new job is queued.

```rust
pub const VERIFICATION_DIR: &str = "verification";
pub fn stage_dir(work_dir: &Path, stage_id: &str) -> Result<PathBuf>;   // crate::validation::validate_id(stage_id)
pub fn job_dir(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<PathBuf>; // job_id must be 32 lowercase hex
pub fn load_current(work_dir: &Path, stage_id: &str) -> Result<Option<JobRecord>>;
pub fn save_current(work_dir: &Path, record: &JobRecord) -> Result<()>;
pub fn append_progress(work_dir: &Path, stage_id: &str, job_id: &str, outcome: &CriterionOutcome) -> Result<()>;
pub fn read_progress(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Vec<CriterionOutcome>>;
pub fn save_result(work_dir: &Path, stage_id: &str, result: &JobResult) -> Result<()>;
pub fn load_result(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<Option<JobResult>>;
```

Every write is atomic (temp file plus rename in the same directory) and refuses a symlinked `verification`, stage or job directory and a symlinked target file (`O_NOFOLLOW`, the pattern of `fs/locking` and the receipts append in `models/forward_receipt`).

### 3.3 Submit and cancel (`src/host_verify/submit.rs`, owner W1)

```rust
pub struct SubmitRequest<'a> {
    pub stage_id: &'a str, pub session_id: &'a str, pub request_id: &'a str,
    pub kind: JobKind, pub head: &'a str, pub tree_digest: &'a str, pub no_cache: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submission { Queued(JobRecord), Joined(JobRecord), Superseded { cancelled: JobRecord, queued: JobRecord } }
pub fn submit(work_dir: &Path, request: &SubmitRequest<'_>, now: DateTime<Utc>) -> Result<Submission>;
pub fn cancel(work_dir: &Path, stage_id: &str, reason: &str) -> Result<Option<JobRecord>>;
```

Rules, under one lock on the stage directory (`fs/locking::locked_dir_update` or the same primitive):

- No current record, or a terminal one: queue a new job (`Queued`).
- A non-terminal current record with the same `kind`, `head`, `tree_digest` and `no_cache`: append `request_id` to its `requests` and return `Joined`; never a second job.
- Any other non-terminal current record: set it `Cancelled` with detail `superseded by <new id>`, queue the new job, return `Superseded`.
- `cancel` sets a non-terminal current record `Cancelled` with `detail = reason` and returns it; a terminal or missing record returns `Ok(None)`.

The record is the single source of truth. Whoever cancels only writes the state; the runner kills a child whose record is no longer `Running` on its next tick.

### 3.4 Runner (`src/host_verify/runner.rs`, owner W1)

```rust
pub struct RunnerConfig { pub program: PathBuf, pub args_prefix: Vec<String>, pub slots: usize }
impl RunnerConfig { pub fn daemon_default() -> Result<Self> }   // current_exe(), ["stage", "verify-host"], slots 2
pub struct HostJobRunner { /* owns one OwnedProcess per running job */ }
impl HostJobRunner {
    pub fn new(work_dir: PathBuf, config: RunnerConfig) -> Self;
    pub fn tick(&mut self) -> Result<Vec<JobRecord>>;   // records that turned terminal during this tick
}
pub fn recover_after_restart(work_dir: &Path) -> Result<Vec<JobRecord>>;
```

`tick`, in order:

1. Kill every child whose record is no longer `Running` (cancelled), and every child past its deadline. The deadline is `stall_line::completion_line_secs(&stage)` from `started_at`; a job past it turns `Failed` with detail `deadline of <n> s passed`.
2. Reap exited children:
   - With a `result.json` for that job: `Passed` when `result.passed`, else `Failed`.
   - Without one: `Failed` with detail `verify-host exited <status> without a result` and the last 20 lines of `child.log`.
3. Start queued jobs, oldest `queued_at` first, while fewer than `slots` run. Spawn `<program> <args_prefix..> <stage_id> --job <job_id>` with `process_group(0)` and stdin null, stdout and stderr appended to `child.log`. The environment is the daemon's minus every `LOOM_SESSION_*`, `LOOM_STAGE_ID`, `LOOM_SCRATCH_DIR`, `LOOM_WORKTREE_PATH` and `LOOM_SESSION_TYPE`. Set `Running`, `started_at` and `pid`.

`recover_after_restart` runs once at daemon startup:

- Every `Running` record becomes `Interrupted` with detail `daemon restarted during the run`.
- If `ps -o command= -p <pid>` still shows `verify-host <stage_id> --job <job_id>`, it sends SIGTERM to that process group, then SIGKILL after 2 s.
- `Queued` records stay queued.

### 3.5 Tree digest (`src/host_verify/digest.rs`, owner W3)

```rust
pub fn tree_digest(checkout: &Path) -> Result<String>;   // 64 lowercase hex
```

sha256 over the bytes of `git status --porcelain=v2 -z --untracked-files=all`, run in `checkout` with the `GIT_DIR`, `GIT_INDEX_FILE` and `GIT_WORK_TREE` variables removed, followed by, for each changed or untracked path in porcelain order, its path, a NUL, and its content. A path that is not a regular file contributes its path and a NUL only. A committed clean tree has the digest of empty input plus nothing.

### 3.6 Host child (`src/host_verify/child.rs` and `loom stage verify-host`, owner W2)

```rust
pub fn run_job(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<JobResult>;
```

CLI: `loom stage verify-host <stage-id> --job <job-id>`, hidden from help.

- It refuses to start, exiting 2, when `LOOM_SESSION_ID` or `LOOM_STAGE_ID` is set: it runs only from the daemon.
- It loads the current record, which must be `Running` with this `id`.
- It writes `result.json` itself.
- Exit codes: 0 passed, 1 failed, 2 refused or error. An error is also written as a result with `error: Some(..)` when the record could be loaded.

Check pipeline:

1. Resolve the stage worktree and acceptance dir as `verify_resolved_completion` does.
2. Require `git rev-parse HEAD == record.head` and `digest::tree_digest(worktree) == record.tree_digest`. A mismatch is an error naming both values.
3. Run acceptance (cache on unless `no_cache`), appending one `acceptance-<i>` outcome per criterion as each finishes. Before appending, write the criterion's full output to `acceptance-<i>.log` and set `log`. A criterion answered from the cache has no output and no log.
4. Run the goal-backward checks when the stage has any (`commands/verify.rs::run_and_verify_stage_goal`), appending `goal-backward`.
5. Recompute the tree digest. A changed digest turns the result into an error `worktree changed during the run`.

### 3.7 Session side (`src/host_verify/wait.rs`, owner W3; reused by W4)

The session-side protocol for `loom check <stage>` in Relay mode. Stage `daemon-completion` applies it to `loom stage complete <stage>` with kind `complete`.

1. Compute `head` and `tree_digest` of the session's checkout. The checkout is the worktree root, or the repository root for a knowledge session.
2. Read the scratch state `$LOOM_SCRATCH_DIR/host-verify/<stage>-<kind>.json`, holding `{request_id, head, tree_digest, submitted_at}`. With no state, or a state whose `head`/`tree_digest` differ from step 1:
   - emit a relay ticket of the kind with payload `{head, tree_digest, no_cache}`;
   - write the state;
   - print `Verification requested (<kind>, <short head>). Run the same command again in your next Bash call to wait for the result.`;
   - exit 3.
3. With a matching state, find the current record whose `requests` contain `request_id`.
   - Not there yet: wait for it up to the wait budget.
   - Running or queued: wait for it, printing each new progress outcome's `summary` as it appears.
   - Terminal:
     - Print the result: every failed outcome's summary followed by `full log: <absolute path>` when it has a log, then one line per outcome `✓ <label>` or `✗ <label>`, then the error if any.
     - Delete the scratch state.
     - Exit 0 when `Passed`, 1 when `Failed`, 4 when `Cancelled` or `Interrupted`. Exit 4 prints the detail and `Run the same command again to request a new run.`
   - Still running when the budget ends: print `Still running (<n> criteria done). Run the same command again to keep waiting.` and exit 3.
4. Wait budget per call: `min(270, 3 * effective_subagent_timeout_secs - 60)`, at least 30 s. A disabled timeout (0) counts as 270. Poll the record every second.

Exit codes are fixed: 0 passed, 1 failed (and any other error), 3 pending, 4 stopped.

## 4. Stage `daemon-completion` additions (pinned for W4-W8)

### 4.1 Kinds and payloads

- `JobKind` gains `Complete` and `Freeze`.
- `JobResult` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub policy_hash: Option<String>`. A Complete job sets it to the `VerificationSnapshot` policy the child captured; apply compares it with the current `effective_policy_hash`.
- `RequestKind` gains `Complete`, wire `complete`, a control kind.
- `RequestKind::Check`, wire `check`, is added in stage `host-jobs` and is not a control kind.
- Payload `HostJobRequest { head: String, tree_digest: String, no_cache: bool }` (`#[serde(deny_unknown_fields)]`, in `relay/payload.rs`) is shared by `check` and `complete`.
- `StageRequest::FreezeContracts` carries `{ head: String, tree_digest: String }` and no reports. `StageRequest` is `#[serde(tag = "request", rename_all = "snake_case")]`. A payload `{"request": "freeze_contracts", "reports": [..]}` fails to decode, because `head` and `tree_digest` are required.

### 4.2 Complete job (child)

Knowledge stage: snapshot plus acceptance only. Its tree is the repository root, as `knowledge_complete.rs::resolve_knowledge_execution_paths` resolves it.

Any other stage:

1. `VerificationSnapshot::capture` (the tree must be committed and clean, HEAD at the stage branch tip), then require `snapshot.commit == record.head`.
2. Run acceptance.
3. Run `complete_verification::run` with the daemon's gates and fail-closed detection: `VerificationChecks` replaces `control_session: Option<&str>` with `daemon_gates: bool` (skip integrity/review because the daemon checks them at apply) and `fail_closed: bool` (unwired/duplicate detection errors fail). The child sets both true; the operator's host path sets both false.
4. A failing impact selection writes `reviews/<stage>/impact-observed.json` directly, as host completion does.

Outcome labels add `contracts`, `impact-tests`, `after-stage`, `unwired`, `duplicates`, `aggregated-wiring`; a failing check carries its message as `summary`.

### 4.3 Freeze job (child)

1. Run `contracts::freeze::check_changes`.
2. Run every contract through `judge_run`/`run_contracts`, and the format gate (`check_formatting`), on the host.
3. Return `JobResult` plus the reports in `verification/<stage>/<job>/contract-reports.json`. Each contract is `contract:<id>`, `passed` = the run classified it red as required.

### 4.4 Apply (`src/host_verify/apply.rs`, owner W4)

```rust
pub enum Applied { Completed { commit: String }, Recorded, FreezeAccepted { files: usize }, FreezeRefused(String), Ignored(String) }
pub fn apply_finished(work_dir: &Path, record: &JobRecord) -> Result<Applied>;
```

The orchestrator calls this for every record `HostJobRunner::tick` returns.

- **Check:** `Ignored`.
- **Complete, Passed:**
  1. Load the result.
  2. Run `validate_active_identity`. It moves to this module from `daemon/server/control_complete.rs`: stage `Executing`, session owns the stage as Stage or Knowledge.
  3. Run `daemon::server::observer::check_completion_gates`.
  4. Under the sessions-directory lock, in `update_stage`, require:
     - status `Executing`;
     - the same session;
     - `committed_content(stage, repo_root) == result.head`;
     - the current `effective_policy_hash` equals `result.policy_hash`.
  5. Set `completed_commit`; `merged = true` for a knowledge stage. Then `try_complete(None)`.
  6. Persist an `AcceptedReceipt`: `evidence_nonce = job id`, a fresh `completion_nonce`, `commit`.
  7. Retire the session as `retire_completed_session` does today.
  8. Return `Completed`. Any binding that fails returns `Recorded` after writing a `DaemonRejected` checkpoint naming the failed binding.
- **Complete, Failed/Cancelled/Interrupted:** write a completion checkpoint through `handoff::completion::record::record_attempt_handoff`, then return `Recorded`. The checkpoint has:
  - phase `ToolFailed`;
  - `verification.criteria` = one `CriterionResult { id: label, passed }` per outcome (so `stage_service/control_block.rs` still refuses a block while an `acceptance-<i>` fails);
  - `evidence_nonce` = job id;
  - `exact_command` = `loom stage complete <stage>`;
  - `diagnostic_first_line` from the first failed outcome or the detail.
- **Freeze, Passed:** call `stage_service::contracts::handle_freeze_contracts` with the reports. `ContractsFrozen` → `FreezeAccepted`. `Error` → `park_refused_freeze` and `FreezeRefused`.
- **Freeze, not passed:** `park_refused_freeze` with the failing outcomes, then `FreezeRefused`.

### 4.6 Freeze seams between W4 and W5

W5 writes `src/host_verify/freeze.rs` with:

```rust
pub fn run_freeze_job(work_dir: &Path, record: &JobRecord) -> Result<JobResult>;      // called from child.rs (W4) for JobKind::Freeze
pub fn apply_freeze_result(work_dir: &Path, record: &JobRecord) -> Result<Applied>;   // called from apply.rs (W4) for JobKind::Freeze
```

W5 changes the daemon's freeze entry point. It is re-exported as `crate::daemon::handle_freeze_contracts` and defined in `src/daemon/server/contracts.rs` over `src/stage_service/contracts.rs`. The new signature is `handle_freeze_contracts(work_dir: &Path, stage_id: &str, session_id: &str, head: &str, tree_digest: &str, request_id: &str) -> Result<Response>`.

- It runs today's admission checks (stage in its contract phase, the caller its contract writer) and queues a `Freeze` host job through `submit::submit`.
- It returns `Response::Ok` (queued or joined) or `Response::Error { message }`.
- Today's report-taking body becomes `stage_service::contracts::freeze_with_reports(work_dir, stage_id, session_id, reports: &[ContractRunReport]) -> Result<Response>`, which only `apply_freeze_result` calls.

The other two routes to it follow the same signature: the worktree spool (`src/fs/stage_request/apply.rs`, W5) and the socket (`src/daemon/server/stage_control.rs`, W5). `Request::FreezeContracts` in `src/daemon/protocol.rs` replaces its `reports` field with `head: String, tree_digest: String`; W6 makes that edit, because it owns `protocol.rs`. The socket and spool routes pass a fresh `new_job_id()` as the request id.

W4's `inbox_drain/apply.rs::stage_request` arm for `StageRequest::FreezeContracts { head, tree_digest }` calls the new entry point with the relay request id. It settles `Applied(Some("contract run queued"))` on `Ok`, and `Refused` plus `park_refused_freeze` on `Error`, as today.

### 4.5 What the session prints and what the doctrine says

- `loom stage complete <stage>` in a session:
  - In Legacy mode (`relay::emit::mode` returns `Legacy`: a session id without a relay scratch directory), refuses, exiting 1, with `This session cannot relay completion (no LOOM_SCRATCH_DIR). Run loom stage complete <stage> from the operator shell, or loom stage retry <stage> for a session that can.` The broker route that served such sessions is gone.
  - Refuses at once, exiting 1, with `Commit first: completion verifies committed content only.` when `git status --porcelain` is not empty.
  - Otherwise follows 3.7 with kind `complete`.
  - On exit 0 it prints `Stage '<stage>' was completed by the daemon. End the session now.`
- `loom check <stage>` in a session follows 3.7 with kind `check`.
- `loom stage contracts freeze <stage>` in a contract session:
  1. Runs `check_changes` locally (git only).
  2. Relays `freeze-contracts` with `{head, tree_digest}`.
  3. Prints `Contract run requested. End your turn now: the daemon runs the contracts on the host, then freezes them and starts the implementation session, or parks the stage with the refusal.`
  4. Exits 0.
- While a stage has a non-terminal host job, the stall monitor does not escalate that stage's sessions. A `SessionHung` warning may still be logged; escalation waits until the job is terminal or past its deadline.
- Outside a session (operator shell, no `LOOM_SESSION_ID`), all three commands keep today's local behaviour.

## 5. Memory and conduct

Record mistakes, decisions and surprises with `loom memory note` / `loom memory decision` as they happen, including `stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line). Correction: <text>` for any knowledge the tree contradicts. Never Claude Code auto-memory, never `loom knowledge` writes. Never run git commands that change state, never `loom stage complete`. No attribution to any AI system in code, comments or docs (CLAUDE.md Rule 9); naming Claude Code as the session runtime loom drives is fine where loom's text already does.
