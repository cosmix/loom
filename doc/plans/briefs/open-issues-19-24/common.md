# Common brief: PLAN-open-issues-19-24

Every worker of every stage in `doc/plans/PLAN-open-issues-19-24.md` reads this file first, then
its own brief. The plan's "Decisions this plan settles" section is binding; where this file and
the plan's YAML differ, the YAML wins and you report the difference.

## Base: PR #25

Stage worktrees are cut from `fix/macos-review-round-recording` (PR #25, head `11859505`, five
commits on `ff3fe947`) plus the plan's own commit. The briefs were written against `ff3fe947`, so a
line number quoted from it is off in every file the PR touched; each brief says what the PR changed
there, and anchors by symbol. Files the PR touched (package-relative paths under `loom/` unless a
hook path), grouped by directory:

- Hooks: `loom-hooks/_lifecycle.sh`, `codex-forward-result.sh`, `subagent-stop.sh`,
  `tests/subagent-stop-review-harvest.sh`; `loom/maintainability-baseline.txt`.
- `src/commands/`: `run/{mod,objc_fork_safety}.rs`, `stage/control_complete.rs`,
  `subagents/wait/{lease,lease_tests}.rs`, `status/data/{collector,heartbeat_facts,
  heartbeat_facts_tests,mod,sanitize}.rs`, `status/render/{attention_model,attention_model_tests,
  attention_tests,graph,graph_tests}.rs`, `status/ui/tui/{ledger/rows,ledger/tests,state_tests}.rs`,
  `status/web/{model,model_tests_stages}.rs`.
- `src/daemon/`: `mod.rs`, `rpc.rs`, `rpc_tests.rs`, `wire_tests.rs`, `server/lifecycle.rs`.
- `src/models/stage/`: `defaults.rs`, `mod.rs`, `stall.rs`, `types.rs`.
- `src/orchestrator/`: `core/{event_handler,mod}.rs`, `core/event_handler/{recover_hung,
  recover_hung_tests}.rs`, `notify.rs`, `terminal/native/wrapper/{script_text,tests}.rs`.
- `tests/stage_exits_contracts.rs`.

| PR change | Plan | Owner |
| --- | --- | --- |
| Inline `\| tr -d "[:space:]"` at the five padded-`wc` hook sites | Kept; no `loom_lifecycle_file_bytes` helper | W3 |
| `record_reviewer_skip` writes free-text `hook-skips.log` | Replaced by the JSONL `stop-skips.jsonl` row; no `hook-skips` text remains | W3 |
| Padded-`wc` shim and a `hook-skips.log` assertion block in `subagent-stop-review-harvest.sh` | Shim kept; the block asserts the JSONL row (test-integrity event, orchestrator disputes) | W3 |
| Contract `padded-wc-resolves-the-start-row` holds at the base | Frozen red anyway; mutation check removes the inline `tr` | W3 |
| `macos_boot_id` falls back to `kern.boottime` (`macos_boot_time`, `macos_boot_tests`) | Deleted with the OS variants; `os_boot_id` reads `kern.bootsessionuuid` only | W4, W5 |
| `lease_tests.rs` canonicalizes the temp dir | Kept | none |
| `rpc.rs` `pub(crate) fn socket_path`, blanket `InvalidInput` arm, tests moved to `rpc_tests.rs` (+2 tests) | Body moves to `daemon/socket.rs`; arm replaced by `socket_path_fits(`; PR tests kept, new tests appended | W1, W2 |
| `control_complete.rs` `send_request` calls `daemon::socket_path` | Function deleted, `daemon::send_request` used | W2 |
| Other clients still join the raw spelling | Switched to `socket_path` | W2 |
| `objc_fork_safety.rs` re-exec with `OBJC_DISABLE_INITIALIZE_FORK_SAFETY` | Deleted (no fork left) | W1 |
| `print_log_location` in `run/mod.rs` | Left alone; folded into `guidance.rs` in stage 2 | E1 |
| `start` merged EOF arms (ledger 99) | `start` rewritten for the re-exec launch | W1 |
| Non-blocking `send_desktop_notification` (`notifier_for`, `loom-notify` thread) | Kept; `notify_stall_recovery_exhausted` deleted | E3 |
| Stall marker (`stall.rs`, `stall_exhausted`, `stalled_after_recoveries`, `"STALLED"` arm, `write_stall_hint`, `leave_stalled_stage`, `stall_reason`, `stall_takeover_command`) | Retired: the stall parks in needs-human-review | E2, E3 |
| Wrapper env list gains `USER LOGNAME` (+ test) | Kept; generated from `AGENT_SESSION_ENV_NAMES` plus the host layer | E1 |
| Ledger raises `types.rs` to 700 and `defaults.rs default` to 77, lowers `graph.rs render_graph` to 64 and `start` to 99 | Base values; E3's retirement lowers the two raised entries; the ledger is the orchestrator's | orchestrator |

## Worker rules

- You own exactly the files your brief's "Files owned" list names. Read anything; write nothing
  else. A needed edit outside your list is reported to the orchestrator, not made.
- You are a leaf: never spawn subagents. Never run `git` write commands, `loom stage complete`,
  or `loom knowledge`. Record mistakes, decisions and surprises with `loom memory note` /
  `loom memory decision` as they happen; a knowledge claim the tree contradicts is
  `loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y (file:line)"`.
- The crate does not compile mid-wave: other workers write the items you call. Write against the
  pinned interfaces below exactly. Run at most the one check your brief names, once; never
  `cargo fmt` (the orchestrator formats once after the wave).
- Paths: briefs give repository-relative paths (`loom/src/...`). Stage YAML uses
  package-relative ones (`src/...`) because every code stage has `working_dir: "loom"`.
- Limits: a file you touch stays at or under 400 lines and a function at or under 50 lines;
  past that, split into a sibling module. The maintainability ledger
  (`loom/maintainability-baseline.txt`) is the orchestrator's, never yours: report each ledgered
  unit you shrank.
- Tests: existing assertion lines are never edited (a test-integrity event); add new tests or
  new lines. The test-integrity base is the stage's base, so assertion lines PR #25 added are
  existing lines. A test file you move keeps its assertion lines verbatim. Tests never spawn the
  real loom binary except through `helpers::loom_cmd()`, never start a detached process that
  outlives the test (`mistakes/detached-spawn-in-tests.md`), never touch the live `.loom/work`
  or the real `HOME` (`mistakes/live-state-pollution.md`), and guard any Unix-socket bind with
  `crate::process::sandbox_probe::skip_unless(...)` as `loom/src/daemon/rpc_tests.rs` does. Git in
  tests runs with `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` pointed at missing files and
  `GIT_CONFIG_NOSYSTEM=1`, `user.name`/`user.email` set locally (pattern:
  `loom/src/verify/impact_tests_tests.rs`). Env-mutating tests are `#[serial]`.
- Frozen contracts: the stage's `tests/<stage>_contracts.rs` is frozen before you start. Never
  edit it; make it pass. `loom stage contracts show <stage-id>` prints the freeze record (the frozen
  paths and their hashes), not the contract source: read the frozen file itself.
- No AI attribution anywhere. Every fenced code block in Markdown names its language. No
  backwards-compatibility shims or migration code: the project is unreleased.
- Report: files changed, the one check you ran and its result, anything surprising or
  unresolved, every pinned interface the code made you deviate from (with why).

## Pinned interfaces: platform-portability

- **Socket (W2 writes `loom/src/daemon/socket.rs`; W1 declares it).** W1's
  `loom/src/daemon/mod.rs` gets `mod socket;` and
  `pub use socket::{socket_path, socket_path_fits, socket_path_problem, SOCKET_FILE, SUN_PATH_MAX};`.
  - `pub const SOCKET_FILE: &str = "orchestrator.sock";`
  - `pub const SUN_PATH_MAX: usize = 104;` (moved from
    `loom/src/daemon/server/lifecycle/socket_limit.rs`, which W1 deletes)
  - `pub fn socket_path_fits(path: &Path) -> bool` (byte length strictly below `SUN_PATH_MAX`)
  - `pub fn socket_path(work_dir: &Path) -> PathBuf` (canonicalized `work_dir` joined with
    `SOCKET_FILE`; on canonicalisation failure, the given spelling joined with it)
  - `pub fn socket_path_problem(work_dir: &Path) -> Option<String>` (None when
    `socket_path(work_dir)` fits; else a message with the byte count, `104` and the path, and
    the advice to move the repository to a path of at most 74 bytes)
  - The daemon's own bind keeps `work_dir.join(SOCKET_FILE)` (the daemon's work root is already
    real); every client uses `socket_path`.
  - `socket_path` already exists at the base as `pub(crate) fn socket_path` in `rpc.rs` (PR #25) with
    exactly these semantics; W2 moves it into `socket.rs` and W1 swaps its re-export in
    `daemon/mod.rs`. PR #25's blanket `InvalidInput => Unreachable` arm in `try_send_request` (and its
    doc bullet) is replaced by the `socket_path_fits(` pre-check: only a path that does not fit
    answers `Unreachable`, so any other `InvalidInput` (a path with an interior NUL) stays an error.
  - `loom/src/daemon/rpc.rs` keeps no `fn socket_path` of its own (an acceptance grep fails on the text)
    and calls `socket_path_fits(` before connecting (a wiring check greps the literal).
  - The run refusal (W1) sits in `prepare_background_run` (`loom/src/commands/run/mod.rs`), immediately
    after `work_dir.load()?` and before `plan_inputs::mark_plan_in_progress`, so a refusal never leaves
    the plan marked in progress. It is never in `run_startup_preflights`: `loom run --foreground` shares
    that function, binds no socket and is not refused. `loom init` warns from `create_or_adopt_work_dir`
    (`commands/init/execute.rs`, W2), so the ledgered `execute` (110 lines) does not grow.
- **Launch (W1).** `loom/src/daemon/server/launch.rs`:
  `pub struct ReadyTiming { pub deadline: Duration, pub grace: Duration }`,
  `pub fn await_ready(child: &mut Child, reader: std::io::PipeReader, log_path: &Path, timing: ReadyTiming) -> anyhow::Result<()>`,
  `pub(crate) fn spawn_daemon(...) -> anyhow::Result<()>` (W1 settles its parameters; it builds its
  command through a pure `daemon_command(.., terminal: Option<&str>)` so a test needs no terminal
  emulator), called from `DaemonServer::start` as `launch::spawn_daemon(`. Re-exported:
  `loom::daemon::{await_ready, ReadyTiming}`. Readiness bytes: `0x01` ready, `0x02` output now
  in `orchestrator.log`. Deadline 10 s, grace 1 s in production.
- **Daemon-child argv (W1; W2's repair test pins it).** `<loom> run --daemon-child <ABS_WORK_ROOT>`
  followed by the run's own config flags. The flag is `#[arg(long, hide = true)]` and conflicts
  with `--foreground`. `loom/src/commands/run/mod.rs` dispatches it as
  `daemon_child::execute(`. `is_loom_run_cmdline` (`loom/src/commands/repair/daemon_checks.rs`)
  must return true for `/usr/local/bin/loom run --daemon-child /repo/.loom/work`.
- **Skip ledger (W3 writes, W4 reads).** A skipped `loom-code-reviewer` stop appends one row to
  `.loom/work/subagents/<stage>/stop-skips.jsonl`:
  `{"ts":"<UTC>","agent_id":"<id>","agent_type":"loom-code-reviewer","reason":"<code>"}`. It replaces
  PR #25's free-text `hook-skips.log`, which no longer exists anywhere under `loom-hooks/`. The gate and
  `loom stage review status` print "N reviewer spawns, M rounds, K stop events not harvested" with the
  recorded reasons.
- **Boot ID (W5 writes `loom/src/process/boot_id.rs`; the orchestrator adds `pub mod boot_id;`
  to `loom/src/process/mod.rs` before the wave).**
  - `pub const BOOT_ID_ENV: &str = "LOOM_BOOT_ID";`
  - `pub fn resolve_boot_id(env_value: Option<&str>, os_boot_id: impl FnOnce() -> anyhow::Result<String>) -> anyhow::Result<String>`
  - `pub fn os_boot_id() -> anyhow::Result<String>` (Linux `/proc/sys/kernel/random/boot_id`;
    macOS `sysctl -n kern.bootsessionuuid` only; the bodies move here from
    `SystemBootClock::boot_id` and `macos_boot_session_uuid` in
    `loom/src/commands/subagents/wait/lease.rs`, which W4 then deletes. PR #25's `kern.boottime`
    fallback is not carried over: macOS recomputes it when the clock is stepped, so a lease could
    straddle two sources)
  - `pub fn current_boot_id() -> anyhow::Result<String>` =
    `resolve_boot_id(std::env::var(BOOT_ID_ENV).ok().as_deref(), os_boot_id)`
  - W4 renders `LOOM_BOOT_ID` into the session wrapper from
    `WrapperHostEnv::boot_id: Option<String>`, filled in `launch/host.rs` with
    `crate::process::boot_id::os_boot_id().ok()`.

## Pinned interfaces: session-auth-and-stalls

- **Environment (E1, `loom/src/process/environment.rs`, re-exported from
  `loom/src/process/mod.rs`).** `pub const AGENT_SESSION_ENV_NAMES: &[&str]` (the wrapper's
  forwarding names, `USER` and `LOGNAME` included; `HOME` and `PATH` are handled separately);
  `pub fn agent_session_environment_from<I, K, V>(source: I) -> Vec<(OsString, OsString)>`;
  `pub fn apply_stage_environment_from<I, K, V>(command: &mut Command, source: I)` (today
  private; made pub). `STAGE_HOST_ENV_ALLOWLIST` gains `USER` and `LOGNAME`.
- **Auth probe (E1, `loom/src/claude/auth.rs`, declared `pub mod auth;` in
  `loom/src/claude.rs`).** `#[derive(Debug, Clone, PartialEq, Eq)] pub enum AuthProbe {
  LoggedIn { method: String }, NotLoggedIn, Unknown(String) }`;
  `pub fn parse_auth_status(stdout: &str, exit_success: bool) -> AuthProbe`;
  `pub fn stage_auth_status(claude_path: &Path) -> AuthProbe` (runs
  `<claude_path> auth status --json` with `env_clear()` plus
  `agent_session_environment_from(std::env::vars_os())`, bounded at 30 s through
  `crate::process` bounded-run helpers; never logs `email`, `orgId`, `orgName`). The claude
  binary is found with `crate::claude::find_claude_path()`.
- **Session tail (E3, `loom/src/orchestrator/terminal/session_tail.rs`, re-exported from
  `loom/src/orchestrator/terminal/mod.rs`).** `pub fn session_tail(session:
  &crate::models::session::Session, work_dir: &Path, lines: usize) -> Option<String>`: the last
  `lines` non-empty lines of the session's tmux pane (`capture-pane -p -J -S -<lines>` on the
  session's own `-L` socket, bounded by the tmux probe timeout), or for a native session the
  tail of its stderr log (read as `lines.saturating_mul(4).max(lines)` raw lines, then cut to the last
  `lines` non-empty ones); `None` when nothing is readable. Output is control-character stripped.
- **Park reasons (E2).** Exhaustion: `stalled: session <id> silent <N>s (budget <B>s, last:
  <activity>) after <k> automatic recoveries; pane: "<last line>"`. Never worked, logged in:
  `session <id> never started work (no tool activity <N>s after start, budget <B>s); pane:
  "<last line>"`. Not logged in: `session <id> is not logged in to claude in the stage
  environment; run claude /login (as the operator) and then loom stage human-review <stage>
  --approve`. `Stage` has no `review_notes` field (the web view derives it from
  `review_reason`), so the pane tail is appended to `review_reason` after a blank line and
  `Last pane lines:`. The park itself calls `stage.force_status_with_reason(StageStatus::
  NeedsHumanReview, "stall park")`: that reason is logged at ERROR, so it is the short constant, and the
  full text (pane tail included) lives only in `review_reason`. PR #25's stall marker is retired
  here: E2 deletes `leave_stalled_stage`, `stall_reason`, `stall_takeover_command` and their
  re-export in `event_handler.rs`; E3 deletes the rest (`stall.rs`, `Stage.stall_exhausted`,
  `StageSummary.stalled_after_recoveries`, the `"STALLED"` attention arm, `write_stall_hint`) and
  `notify_stall_recovery_exhausted`. The park's desktop notification is the existing
  needs-human-review announcement, sent through the PR's non-blocking notifier.
- **Never worked (E2, `loom/src/orchestrator/monitor/never_worked.rs`).** A Stage session whose
  own heartbeat shows `last_tool` None, `subagent` false and `context_tokens == 0` (SessionStart
  also fires on compaction and resume), or that has no heartbeat past `created_at` plus its
  non-zero budget. `MonitorEvent::SessionHung` and `HungReport` keep their shape; the handler
  recomputes the verdict through this predicate. It covers `SessionType::Stage` sessions only:
  Contract, Knowledge, Merge, BaseConflict and Adjudication sessions keep today's handling.

## Pinned interfaces: daemon-owned-commits

- **Commit core (C1, `loom/src/git/stage_commit.rs`, `pub mod stage_commit;` in
  `loom/src/git/mod.rs`).** As the plan's contract surface:
  `CommitRequest { message, expected_head, expected_tree }`,
  `CommitScope { StageBranch { stage_id }, Knowledge { target_branch, prefix }, Merge { stage_id } }`,
  `CommitRefusal` (Display, Debug), `pub fn commit_staged(repo: &Path, scope: &CommitScope,
  request: &CommitRequest) -> Result<String, CommitRefusal>` (a wrapper over
  `pub struct Committer { git, repo_root }` and its `commit_staged(&scope, &request)` method, which
  the daemon builds with a pinned `WorktreeGit`); `CommitRefusal::{Signing { detail }, Refused {
  reason }}`. The daemon handler (C2), not the core, takes the merge lock for a Knowledge scope and
  appends the target-guard attestation. A merge scope requires
  `MERGE_HEAD` and records it as the second parent, then clears the merge state
  (`git merge --quit`); any other scope refuses when `MERGE_HEAD` exists.
  `pub fn validate_commit_message(message: &str) -> Result<(), String>` (non-empty after trim, at most
  16 KiB, no NUL, no AI attribution) is called by `commit_staged` and by U1; neither keeps a copy. The
  staged-state-path check compares the first component with `eq_ignore_ascii_case` (`.LOOM/work/x` is
  refused). In a Merge scope a gitlink or state path is refused only when its entry in the new tree
  differs from both HEAD's tree and MERGE_HEAD's tree, so a submodule bump made on the target still
  merges.
- **Signing (C1, `loom/src/git/signing.rs`, `pub mod signing;`).**
  `#[derive(Debug, Clone, Default)] pub struct SigningEnv { pub gnupghome: Option<OsString>,
  pub ssh_auth_sock: Option<OsString> }`; `impl SigningEnv { pub fn capture_from_process() ->
  Self }`; `pub fn install(env: SigningEnv)` (a `OnceLock`; a second call is ignored);
  `pub fn installed() -> &'static SigningEnv` (the default when nothing was installed);
  `pub fn signing_enabled(repo: &Path) -> anyhow::Result<bool>`
  (`git config --type=bool commit.gpgsign`); `pub fn probe(repo: &Path, env: &SigningEnv) ->
  anyhow::Result<()>` (an empty-tree `commit-tree -S` under a 30 s bound, through the new
  `crate::git::runner::run_git_with_env_within`). Also exported: `pub const SIGN_TIMEOUT: Duration`
  (30 s) and `impl SigningEnv { pub fn env_pairs(&self) -> Vec<(&'static str, &OsStr)> }` (`GNUPGHOME`
  and `SSH_AUTH_SOCK`, each when set); `commit_merge` keeps its `CommitTreeError` downcastable in the
  anyhow chain (`anyhow::Error::from`, no string conversion). Test support shared with C2 and C4:
  `#[cfg(test)] pub(crate) mod tests` in `signing.rs` exposes `fake_signer(repo: &Path, fail: bool)`
  (sets `commit.gpgsign=true`, `gpg.format=openpgp` and `gpg.program` repo-locally; the script reads its
  stdin to EOF first) and `git_in(repo: &Path, args: &[&str]) -> String`. `pub fn
  take_from_process()` captures, installs, and removes `GNUPGHOME` and `SSH_AUTH_SOCK` from the
  process environment. The daemon child (`daemon_child::execute`) and `loom run --foreground`
  (`foreground::execute`) each call it as their first statement, before any thread. `pub fn current()
  -> SigningEnv` returns the installed environment when `take_from_process` ran, otherwise
  `capture_from_process()`. The startup probe adds `current().env_pairs()` to both of its sources. `daemon/server/environment.rs` also exports
  `pub(crate) fn daemon_environment_pairs() -> Vec<(OsString, OsString)>` (the allowlisted subset of the
  process environment, exactly what the daemon child receives; the allowlist gains `XDG_CONFIG_HOME` and
  `GIT_CONFIG_GLOBAL`), which the startup probe's second run uses under `env_clear()`.
- **Signing failure handling (Decision 3; C2 and C4 own the routes).** Every failure leaves the ref
  unmoved. By commit kind:
  - Stage or Knowledge scope (C2's handler): the request is refused with `signing failed: <detail>` and
    the stage is blocked through `handle_block_stage` with the remedy and `loom stage retry`. That
    function reports a refused transition as `Ok(Response::Error { .. })` as well as `Err`; either one
    means the block failed, and the settle reason then says `signing failed: <detail>; the stage could
    not be blocked: <why>`.
  - Merge scope (C2 calls, C4 implements): no block is attempted. `MergeBlocked -> Blocked` is not a
    legal edge, a blocked `MergeConflict` stage keeps its resolver, and `loom stage retry` would re-run
    the whole stage. The handler calls `InboxHost::hold_merge_for_signing(stage_id, detail) -> String`.
    The `Orchestrator` impl forwards to C4's
    `pub(in crate::orchestrator::core) fn hold_merge_for_signing(&mut self, stage_id: &str, detail:
    &str) -> String` in `merge_handler/landing.rs`. That function stops the resolver
    (`stop_gated_resolvers`) and routes the stage to `NeedsHumanReview`
    (`route_merge_stage_to_review`) with the remedy. The request settles `Refused` with the signer
    text and the returned outcome.
  - Loom's own merge commit (C4): a `CommitTreeError { signing: true }` anywhere in the error chain
    routes the stage to `NeedsHumanReview` through `route_to_human_review`, so no resolver is spawned
    and nothing is retried every tick:
    - at the landing, `land_stage_merge`, which returns `Landing::Held`;
    - at the first auto-merge, the `Err` arm of `apply_auto_merge_outcome`, which skips
      `persist_merge_blocked`.
  - The plan-completion commit (C4, `fs/plan_lifecycle/commit.rs`): runs through
    `run_git_with_env_within` with `signing::installed().env_pairs()` and `SIGN_TIMEOUT`.
- **Signing consumers (C4, `loom/src/orchestrator/core/merge_handler/landing.rs`,
  `landing_tests.rs`, `landing_signing_tests.rs`, `auto_merge_outcome.rs`,
  `loom/src/fs/plan_lifecycle/commit.rs`).** C4 consumes C1's
  `crate::git::signing::{CommitTreeError, SigningEnv, installed, signing_enabled, SIGN_TIMEOUT,
  tests::fake_signer}` and `crate::git::runner::run_git_with_env_within`. It owns the
  merge-commit routes, the merge-scope hold function and the plan-completion commit above. It never
  touches `stage_commit.rs`, `signing.rs` or the inbox-drain files.
- **End-to-end commit test (C5, wave 2, `loom/tests/integration/relay_commit_e2e.rs`).** C5 is
  specified in the plan's stage YAML amendment. It runs after wave 1 compiles and edits no wave-1
  file.
- **Relay (C2).** `RequestKind::Commit`, wire name `commit`, a control kind. Payload
  `pub struct CommitPayload { pub message: String, pub expected_head: String, pub
  expected_tree: String }` in `loom/src/relay/payload.rs`, exported from `loom::relay`, carried
  as JSON (`serde_json::to_value`). Matrix: Apply for `Stage`, `Knowledge`, `Merge`; Refuse for
  `Contract`, `Adjudication`, `BaseConflict`.
- **Completion guard (C2, `loom/src/daemon/server/completion_evidence.rs`).** `fn
  refuse_uncommitted_index(stage: &Stage, repo_root: &Path) -> Result<()>`, called last in
  `verify_evidence_bindings`: when the stage worktree (`crate::git::get_worktree_path`) exists and its
  index differs from HEAD (`diff --cached --quiet --ignore-submodules=none HEAD` through a pinned
  `WorktreeGit`), completion bails `staged changes are not committed: run loom stage commit and wait
  for it with loom request status <id> --wait 90`. The commit is applied asynchronously, so a session
  that completed before its commit applied would bind the old HEAD.
- **`loom stage commit` (U1, `loom/src/commands/stage/commit.rs`, tests in
  `loom/src/commands/stage/commit/tests.rs`; C2 adds `pub mod commit;`
  and the CLI variant `StageCommands::Commit { stage_id: String, #[arg(short = 'm', long =
  "message")] message: String }`, dispatched as `commit::execute(stage_id, message)`).**
  `pub fn execute(stage_id: String, message: String) -> anyhow::Result<()>`: refuses a
  `stage_id` other than the session's stage id when that is set (read from
  `EnvSnapshot::from_process_env().stage_id`, never `std::env::var`); validates the message through
  `crate::git::stage_commit::validate_commit_message` (no local validator); runs
  `git hook run --ignore-missing pre-commit` and
  `git hook run --ignore-missing commit-msg -- <message file>`; normalises the message file with
  `git stripspace` (`git commit -m`'s default whitespace cleanup) and relays the result; reads
  `git rev-parse HEAD` and `git write-tree` after the hooks; then relays `RequestKind::Commit` with `CommitPayload`
  through `RelayContext::check` and `RelayContext::emit` exactly as
  `loom/src/commands/stage/merge/relay.rs` does (operator and legacy modes call
  `crate::git::stage_commit::commit_staged` in-process instead). No `--amend`, `--author`,
  `--no-verify` or `--no-gpg-sign` flags exist.
- **`loom request status --wait` (U2, `loom/src/commands/request/status.rs`, tests in
  `loom/src/commands/request/status/wait_tests.rs`; C2 adds the
  `--wait <SECS>` arg to `RequestCommands::Status` in `loom/src/cli/types_ops.rs` and passes
  it in `loom/src/cli/dispatch.rs`).** `pub fn execute(id: String, session: Option<String>,
  wait_secs: Option<u64>) -> anyhow::Result<()>`: without `wait_secs`, today's behaviour; with
  it, poll the request's state every 500 ms until applied (exit 0, printing the commit id when
  the outcome carries one), refused (error naming the refusal), or the deadline (error
  "request <id> still pending after <N>s").
- **Doctrine sentence (C3; every surface uses this wording).** "Commit with
  `git add <specific-files>` then `loom stage commit <stage-id> -m "type(scope): description"`,
  and wait for it with `loom request status <id> --wait 90`; never run `git commit`." (90 s stays
  under the Bash tool's default 120 s timeout; the wait is its own Bash call.) The
  knowledge prefix substitutes `doc/loom/knowledge/` for `<specific-files>` (an existing knowledge
  signal test forbids the generic placeholder there). The merge signal keeps the substring
  `git merge main` its tests pin, naming `git merge --no-commit --no-ff <target>` as the command.
