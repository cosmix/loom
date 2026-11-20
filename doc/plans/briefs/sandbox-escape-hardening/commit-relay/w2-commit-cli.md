# W2: `loom commit`, `loom request status --wait`, and the completion gate

Stage `commit-relay`, wave 2 (after W1), tier sonnet. Read `../common.md` first (decision D1).
Line numbers were read at `3fc28031` (unchanged at `2908339a`); locate edits by symbol.

## Why

Under D1 a stage session cannot run `git commit`. It runs

    loom commit -m "<type(scope): description>" -- <path> [<path> ...]

in one Bash call, which writes a relay ticket of kind `commit`, then confirms in the next call
with `loom request status <request-id> --wait 90`. `loom stage complete` must refuse while one
of the session's commit requests is not yet applied, or the stage completes on the commit before
it. `loom stage block` and the dispute filings refuse for the same reason (section 2).

## Files you own

- `loom/src/commands/mod.rs` (one line: `pub mod commit;`)
- `loom/src/commands/commit/mod.rs`, `loom/src/commands/commit/paths.rs`,
  `loom/src/commands/commit/pending.rs`, `loom/src/commands/commit/tests.rs`,
  `loom/src/commands/commit/tests_pending.rs` (all new)
- `loom/src/commands/request/mod.rs`, `loom/src/commands/request/status.rs`,
  `loom/src/commands/request/wait.rs` (new)
- `loom/src/cli/types.rs`, `loom/src/cli/types_ops.rs`, `loom/src/cli/dispatch.rs`,
  `loom/src/cli/dispatch_stage.rs`

`loom/src/commands/stage/complete.rs` is NOT edited: it is ledgered at 759 lines with `complete`
at 173 (`loom/maintainability-baseline.txt:13,79`), so the gate goes in the CLI dispatch instead.

## What W1 gives you (already merged into the tree when you start)

- `crate::relay::RequestKind::Commit` (a control kind; the matrix applies it for a `Stage`
  session only).
- `crate::relay::CommitRequest { message: String, paths: Vec<String> }` (serde, deny unknown).
- `crate::relay::check_commit_request(request: &CommitRequest, worktree: &Path) -> Result<(), String>`:
  message and path rules plus the symlinked-parent check; `worktree` must be canonical.

Verify each exists with `rg -n 'pub fn check_commit_request|pub struct CommitRequest|Commit,' loom/src/relay`
before you start; if one is missing, stop and report.

## 1. `loom commit` (`commands/commit/mod.rs`, `paths.rs`)

Args, in `commands/commit/mod.rs` (the `Subagents { args: crate::commands::subagents::SubagentsArgs }`
and `Pressure(PressureArgs)` shapes are the precedent):

```rust
/// Flags for `loom commit`.
#[derive(Debug, clap::Args)]
pub struct CommitArgs {
    /// Commit message, `type(scope): description`
    #[arg(short = 'm', long = "message", value_name = "MESSAGE")]
    pub message: String,
    /// Paths to stage and commit, relative to the current directory
    #[arg(required = true, value_name = "PATH")]
    pub paths: Vec<String>,
}

pub fn execute(args: CommitArgs) -> Result<()>
```

`execute` mirrors `commands/stage/state.rs::block` (`:48-52`): `mode(&EnvSnapshot::from_process_env())`,
`std::env::current_dir()`, `StdSink::default()`, then a testable
`fn execute_with(args: CommitArgs, relay_mode: RelayMode, cwd: &Path, sink: &mut dyn RelaySink) -> Result<()>`.
Pattern to mirror: `commands/stage/state_relay.rs::block_relayed` (`:57-74`). Do not copy its
socket/spool fallback: `loom commit` has no path other than the relay.

`execute_with`, in order:

1. `RelayMode::Legacy` or `RelayMode::Operator`: bail with the checkout message below.
2. `RelayMode::Relay(ctx)`: require `ctx.session_type.as_deref() == Some("stage")`,
   `ctx.stage_id` and `ctx.worktree_path` set; otherwise bail with the checkout message (naming
   the missing variable when one is unset).
3. `ctx.check(RequestKind::Commit, None, cwd, uid)?` (uid from `libc::getuid()` in an `unsafe`
   block with the same SAFETY comment as `state_relay.rs:64-65`). It validates the scratch
   directory, confines `cwd` to the worktree and applies the matrix.
4. Canonicalize `ctx.worktree_path` and `cwd`; map every raw path through
   `paths::worktree_relative(&worktree, &cwd, raw)?`, then through
   `paths::expand_directories(&worktree, &relative)?` (below), so the ticket names files only.
   The daemon reads each file itself and refuses a directory path (W1).
5. `check_commit_request(&request, &worktree).map_err(anyhow::Error::msg)?`.
6. `ctx.emit(RequestKind::Commit, serde_json::to_value(&request)?, "commit", false, sink)?`
   (not `end_turn`: the agent confirms and continues).
7. Then write to `sink.stderr()`:
   `Confirm it in your NEXT Bash call (the relay hook runs after this one returns): loom request status <id> --wait 90`.
   Nothing else goes to stdout: the `LOOM_RELAY_V1` line must stay its last line.

The checkout message (one constant, used for every refusal of steps 1-2):

    loom commit makes a commit for a stage session in its worktree (LOOM_SESSION_ID,
    LOOM_STAGE_ID, LOOM_WORKTREE_PATH and LOOM_SCRATCH_DIR set, LOOM_SESSION_TYPE=stage).
    Outside one, or in a session in the main checkout (a knowledge stage, a merge or
    base-conflict resolution), commit with `git add <files>` and `git commit -m "<message>"`;
    a contract session does not commit.

`paths.rs`:

```rust
/// `raw`, given relative to `cwd`, as a `/`-separated path relative to the
/// worktree root. `.` and empty components are dropped; leading `..`
/// components climb out of `cwd` but never past the worktree root; a `..`
/// after a name is refused (lexical and kernel resolution can differ there);
/// an absolute path, a result naming the whole worktree, or a non-UTF-8
/// component is refused. `worktree` and `cwd` are canonical, `cwd` inside
/// `worktree`.
pub(crate) fn worktree_relative(worktree: &Path, cwd: &Path, raw: &str) -> Result<String>
```

A trailing `/` is accepted (`src/`); `./a.rs` is accepted. From `<wt>/loom`: `src/a.rs` becomes
`loom/src/a.rs`, `../docs/x.md` becomes `docs/x.md`, `../../x` is refused ("escapes the
worktree"), `src/../a.rs` is refused; from `<wt>`: `.` is refused ("names the whole worktree:
name the files").

```rust
/// `paths` (worktree-relative, from `worktree_relative`) with every directory
/// replaced by the files under it that differ from the index or are untracked
/// and not ignored, in argument order, each directory's files sorted, no
/// duplicates. An absent path whose tracked entries lie under it (a deleted
/// directory) becomes those entries. Files and symlinks pass through.
pub(crate) fn expand_directories(worktree: &Path, paths: &[String]) -> Result<Vec<String>>
```

- A path is a directory when `std::fs::symlink_metadata` says so (a symlink to a directory is a
  symlink and passes through: the daemon commits it as a link).
- Expand with git inside the session (read-only git works there), run through
  `crate::git::worktree::WorktreeGit::discovered(worktree)` (its doc allows exactly this: "a
  stage agent's own CLI inside its sandbox", `git/worktree/pinned.rs:49-58`):
  `ls-files -z --modified --others --deleted --exclude-standard -- :(literal)<dir>`. Output paths
  are relative to the worktree root. `--modified` and `--deleted` both list a deleted file:
  deduplicate.
- The `WorktreeGit::discovered(` call in `paths.rs` runs in-session, not in the daemon.
  integration-verify classifies `src/commands/commit/paths.rs` as `in-session` in
  host-git-integrity's allowlist `tests/host_git_call_sites.txt` (this stage does not edit that
  file).
- An entry ending in `/` is a nested repository (`ls-files --others` prints it that way): refuse
  the command, naming it ("'<dir>' is a nested repository; loom commit never commits one").
- A directory with nothing to expand refuses ("no changed file under '<dir>'").
- For an absent path, `ls-files -z -- :(literal)<p>`: entries under `<p>/` replace it; otherwise
  it passes through as a file (the daemon removes it if HEAD tracks it).
- Check the result against `MAX_COMMIT_PATHS` again: a directory can expand past 256 files;
  refuse and ask for smaller commits.

## 2. The completion gate (`commands/commit/pending.rs`, `cli/dispatch_stage.rs`)

```rust
/// Ids of this session's commit requests the daemon has not settled, sorted:
/// a `<id>.req` ticket of kind `commit` still in `scratch_dir`, an inbox entry
/// of kind `commit` not yet drained, or a ledger id whose LATEST row is
/// `applying` with no outcome.
pub fn unsettled_commits(work_dir: &Path, session_id: &str, scratch_dir: &Path) -> Result<Vec<String>>

/// `action` (`stage complete`, `stage block` or `dispute`) inside a session
/// (Relay mode) refuses while `unsettled_commits` is not empty; every other
/// mode passes.
pub fn refuse_unsettled_commits(env: &EnvSnapshot, action: &str) -> Result<()>

/// True when a `loom stage block` reason starts with `commit request `: the
/// reason the `--wait` timeout line tells the agent to give when the daemon
/// never settles its commit (section 3). That block skips the gate.
pub fn block_reason_names_unsettled_commit(reason: &str) -> bool
```

- Scratch: `read_dir`, files named `<id>.req`, skip any over `crate::relay::MAX_TICKET_BYTES`,
  `crate::relay::Ticket::decode`, keep `kind == RequestKind::Commit`. A missing directory is
  empty.
- Inbox: `crate::fs::inbox::pending_entries(work_dir, session_id)?.entries` (a missing inbox is
  empty, `fs/inbox/pending.rs:33-45`), each read bounded and decoded with `InboxEntry::decode`.
- Ledger: `crate::fs::inbox::read_ledger(work_dir, session_id)?` (missing is empty,
  `fs/inbox/ledger.rs:128-131`). Take the LAST row per id; see the quoted mistake below.
- The work dir: `ctx.work_dir` when set, else `crate::commands::common::work_dir_path()?`, then
  canonicalized with `commands/request/status.rs::canonical_root` (make it `pub(crate)`; in a
  worktree `.loom/work` is a symlink and the inbox readers refuse to follow one,
  `status.rs:19-21`).
- Refusal text, one line per id. For an id whose `<id>.req` ticket is still in the scratch
  directory (check `scratch_dir.join(format!("{id}.req")).is_file()` per id, so
  `unsettled_commits` keeps its signature):
  "<action> refused: commit request <id> was never relayed: delete <scratch>/<id>.req, then run
  the same loom commit again as its own foreground Bash call; run <action> again once every
  commit reports applied." with the real scratch path printed. A control ticket is never swept
  and rerunning `loom commit` writes a NEW ticket, so a never-relayed ticket left in scratch
  would otherwise block completion forever. For every other id:
  "<action> refused: commit request <id> is not applied yet. Run
  `loom request status <id> --wait 90` first; run <action> again once every commit reports
  applied."

In `cli/dispatch_stage.rs::dispatch_complete`, before `resolve_completion_proof`:
`crate::commands::commit::refuse_unsettled_commits(&EnvSnapshot::from_process_env(), "stage complete")?;`.
Under `cfg(test)`, `EnvSnapshot::from_process_env()` is empty (`relay/emit.rs:79-82`), so existing
tests see Operator mode. The control broker runs with `LOOM_CONTROL_BROKER=1` and is Operator
mode too, so only the in-session call is gated.

**Block and dispute race.** `cli/dispatch_stage.rs` also calls `refuse_unsettled_commits(` in the
`StageCommands::Block` arm (action `stage block`) and before every dispute filing
(`DisputeFindings`, `DisputeContract`, `DisputeIntegrity`, `DisputeCriteria`; action `dispute`).
The refusal text above tells the agent to run `loom request status <id> --wait 90` first.
Reason: the inbox drains in file-name order with random ids, and a commit applies only while the
stage is Executing, so a block or dispute filed before the commit settles can land first and the
commit is then refused. Wiring, without growing functions that sit at their line limits
(`dispatch_stage` is 46 lines): keep every arm one line and put each gate in a small helper in the
same file. `StageCommands::Block` calls a helper that runs the gate, then `stage::block`; it skips
the gate when `block_reason_names_unsettled_commit(&reason)` holds. The three arms that call
`stage::file_dispute` and the `DisputeCriteria` arm in `dispatch_stage_criteria` each call a
helper that runs the gate, then the existing call.

The skip is needed because the `--wait` timeout line (section 3) tells the agent to block the
stage with `commit request <id> unsettled` when the daemon never settles its commit: without it
that block would be refused for the very commit it names, leaving the agent no way out.

## 3. `loom request status --wait` (`cli/types_ops.rs`, `commands/request/*`)

`RequestCommands::Status` gains:

```rust
/// Wait up to SECS seconds for the daemon to settle the request
#[arg(long, value_name = "SECS", value_parser = clap::value_parser!(u64).range(1..=600))]
wait: Option<u64>,
```

`status::execute(id, session, wait: Option<u64>)`. Without `--wait`, behavior and exit codes
are unchanged, except that an applied request whose latest ledger row carries a `reason` prints
`<id>: applied: <reason>` (for a commit: `applied: committed <sha>`). Find that note with a new
`fn applied_note(root: &Path, session: Option<&str>, id: &str) -> Result<Option<String>>` that
reads `inbox::read_ledger` for the given session, or every session `list_inbox_sessions` lists,
and takes the LAST row for the id whose outcome is `Applied`. Leave `resolve_status` and
`format_status` and their tests untouched.

`commands/request/wait.rs` (declare `mod wait;` in `request/mod.rs`):

```rust
/// Fixed interval between two looks at the ledger.
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// How a wait ended.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Waited {
    Settled(ReportedStatus),
    TimedOut(ReportedStatus),
}

/// Look up the request until it settles or `timeout` passes. `PendingRelay`,
/// `NotFound`, `Applied`, `Refused` and `UnknownAfterRestart` end the wait at
/// once; `RelayedAwaitingDaemon` and `Applying` are polled every
/// `POLL_INTERVAL`. `now` and `sleep` are injected for tests.
pub(super) fn wait_for(
    resolve: &mut dyn FnMut() -> Result<ReportedStatus>,
    timeout: Duration,
    now: &mut dyn FnMut() -> Instant,
    sleep: &mut dyn FnMut(Duration),
) -> Result<Waited>
```

`PendingRelay` ends the wait because the relay hook runs after each Bash call returns: a ticket
still in the scratch directory during the next call was never relayed, and a control ticket is
never swept. Rerunning the command writes a NEW ticket, so the stale one stays in scratch and
`loom stage complete` refuses on it forever (section 2); the line therefore tells the agent to
delete it. Lines under `--wait` are printed as `<id>: <line>`, as `execute` prints every status.
They are built where the scratch directory and `LOOM_STAGE_ID` are in scope (`status::execute`),
not in `format_status`, which stays untouched. The `PendingRelay` line, with the real scratch
path (`<scratch>/<id>.req` is the ticket file `resolve_status` found):

    pending relay: the relay hook never received this ticket; delete <scratch>/<id>.req, then run the same loom commit again as its own foreground Bash call

The `UnknownAfterRestart` line (the daemon recorded `applying` and stopped before the outcome):

    the daemon stopped mid-commit: check git log -1 --stat; run the same loom commit again (one that already landed answers nothing to commit)

Exit codes under `--wait` only: `Applied` 0; `Refused`, `UnknownAfterRestart`, `NotFound`,
`PendingRelay` 1; a timeout 2, with the line (BLOCK-F doctrine from
PLAN-stage-exits-and-environment, which merged first; `<stage-id>` is `LOOM_STAGE_ID`):

    <id>: still <status> after <SECS>s; the daemon has not settled it. Run `loom request status <id> --wait 90` again, and if it stays unsettled, block the stage: loom stage block <stage-id> "commit request <id> unsettled"

Keep the exit mapping and the printed lines pure functions so they are testable (the line
functions take the scratch directory and the stage id as arguments), and call
`std::process::exit` only in `execute` (as `status.rs:26-28` does).

Trap: Claude Code's Bash tool times out at 120 s by default, so the D1 sentence waits 90 s,
leaving the command time to print its timeout line before the tool kills it; the daemon applies
within one poll tick (about five seconds), so a real wait ends long before either. Do not change
the sentence (it is common.md's); keep your loop's own overhead below 1 s.

## 4. Routing (`cli/types.rs`, `cli/dispatch.rs`)

- `Commands`: add, with a doc comment, `Commit(crate::commands::commit::CommitArgs),`
  ("Commit files in a stage worktree through the daemon (stage sessions only)").
- `cli/dispatch.rs::dispatch` is ledgered at 86 lines (`maintainability-baseline.txt:55`) and
  must stay 86. Replace the one line `Commands::Request { command } => dispatch_request(command),`
  with `cmd @ (Commands::Request { .. } | Commands::Commit(_)) => dispatch_request(cmd),` (one
  line, under 100 columns), and make `dispatch_request(command: Commands)` match
  `Commands::Request { command: RequestCommands::Status { id, session, wait } }` and
  `Commands::Commit(args)`, with `_ => unreachable!(...)`, exactly as `dispatch_tools`
  (`dispatch.rs:221-237`) does. Update its doc comment.

## Tests

`commands/commit/tests.rs` (drive `execute_with` with a hand-built `RelayContext` and a `VecSink`,
as `state_relay.rs:84-108` does; scratch dir named for the session, mode 0700):

- `relay_mode_writes_one_commit_ticket_with_worktree_relative_paths` (cwd `<wt>/loom`; raw
  `src/a.rs`, `../docs/x.md`, `./b.rs`; the ticket's payload decodes to paths
  `["loom/src/a.rs", "docs/x.md", "loom/b.rs"]`, kind `Commit`)
- `a_path_escaping_the_worktree_writes_no_ticket`
- `a_control_root_path_writes_no_ticket` (`../.loom/work/x` from `<wt>/loom`)
- `the_whole_worktree_is_refused`
- `a_dotdot_after_a_name_is_refused`
- `outside_a_stage_session_names_git_commit` (Operator and Legacy)
- `a_knowledge_session_is_told_to_use_git`
- `the_confirmation_names_request_status_wait` (stderr contains
  `loom request status <id> --wait 90`)
- `a_directory_argument_becomes_its_changed_files` (a TempDir git repository with a linked
  worktree; `src/a.rs` untracked, `src/b.log` ignored by `.gitignore`, `src/c.rs` tracked and
  modified, `src/d.rs` tracked and deleted: `-- src` from `<wt>/loom` gives
  `["loom/src/a.rs", "loom/src/c.rs", "loom/src/d.rs"]`; run the fixture's git with
  `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` at missing files and `GIT_CONFIG_NOSYSTEM=1`)
- `a_nested_repository_under_a_directory_argument_is_refused`
- `a_directory_with_nothing_changed_is_refused`

`commands/commit/tests_pending.rs`:

- `stage_complete_refuses_while_a_commit_is_unsettled` (a commit ticket in scratch refuses and
  names its id; an inbox entry refuses; after an `applied` ledger row it passes)
- `an_applying_commit_blocks_completion`
- `settled_and_non_commit_requests_do_not_block_completion`
- `a_never_relayed_ticket_names_its_discard_path` (a commit ticket in scratch: the refusal for
  `stage complete` contains the real `<scratch>/<id>.req` path, "delete" and "run the same loom
  commit again"; an inbox entry for another id gets the `loom request status <id> --wait 90`
  text instead)
- `block_and_dispute_refuse_while_a_commit_is_unsettled` (the gate refuses with actions
  `stage block` and `dispute`, and each refusal contains `loom request status <id> --wait 90`;
  `block_reason_names_unsettled_commit` is true for `commit request abc unsettled` and false for
  `tests are broken`)

`commands/request/wait.rs` inline tests:

- `a_wait_returns_as_soon_as_the_request_settles`
- `a_wait_times_out_after_its_deadline` (the timeout line names `loom stage block <stage-id>
  "commit request <id> unsettled"`)
- `a_pending_relay_is_not_waited_on` (the line names the discard path `<scratch>/<id>.req`)
- `wait_exit_codes_follow_the_outcome` (the `UnknownAfterRestart` line contains
  `check git log -1 --stat`)

The plan pins these tests by exact module path with `--exact`; keep each name as written:
`commands::commit::tests::a_path_escaping_the_worktree_writes_no_ticket`,
`commands::commit::tests::a_nested_repository_under_a_directory_argument_is_refused` and
`commands::request::wait::tests::a_pending_relay_is_not_waited_on`.

`commands/request/status.rs`: add `the_applied_line_carries_the_ledger_note`.

## Knowledge you need (quoted)

> Inbox Ledger Has Two Rows Per Request Id — a Lookup Must Take the Latest: ... any reader of the
> inbox ledger must take the LATEST row for an id, never the first match. Fix: `fs/inbox/status.rs`
> uses `.rev().find(...)`. (`mistakes/concurrency-and-locking.md`)

## Check

One run: `cargo test --lib commands::commit::`. The contract
`commit-cli-writes-a-worktree-relative-ticket` drives your command through the binary (scenario
in the plan's YAML).
