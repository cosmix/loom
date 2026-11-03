# spawn-recovery W1: the daemon restarts itself onto its installed binary

Tier: opus, effort xhigh (process lifecycle, singleton lock, cross-platform). Wave 1, in
parallel with W2. Read `doc/plans/briefs/autonomous-recovery/common.md` first, then knowledge
`mistakes/daemon-singleton.md`, `mistakes/daemon-fork-after-threads.md` and
`architecture/daemon-launch.md`. Frozen contracts: `src/daemon/server/restart_contract_tests.rs` and
`src/orchestrator/restart_budget_contract_tests.rs`.

## Why

A daemon started before a reinstall keeps its old code in memory. Its spawn preflight compares
the installed hook scripts with the copy embedded in ITS build, finds them different, and
refuses every spawn as a sandbox setup failure (plan 05's adjudicator spawn; commit `e06badb5`
only changed this repository's `dev-install.sh`, so `loom update` and every other installer
still leave a stale daemon). The correct repair is a restart onto the installed binary, which
nothing performs today. The same restart is the last-resort recovery for a stalled scheduler
(stall-recovery, a later stage, calls `request_restart`).

## Interface (pinned; contracts and stall-recovery call it)

In `src/orchestrator/terminal/native/launch/host.rs` (next to `DAEMON_BINARY`):

```rust
pub(crate) struct BinaryFingerprint { dev: u64, ino: u64, len: u64, modified: SystemTime }
impl BinaryFingerprint {
    pub(crate) fn of(path: &Path) -> std::io::Result<Self>;
    pub(crate) fn changed(&self, path: &Path) -> bool; // missing file reads as changed
}
/// The recorded binary path when the file there is no longer the one this daemon started
/// from AND the replacement has been stable (same fingerprint) for at least 10 s across two
/// checks; `None` otherwise or when nothing was recorded.
pub(crate) fn daemon_binary_replaced() -> Option<PathBuf>;
```

`record_daemon_binary` records the fingerprint with the path.

New `src/orchestrator/restart_budget.rs` (declared `pub(crate) mod restart_budget;` in
`src/orchestrator/mod.rs`; it sits under `orchestrator` so the scheduler loop, the stall
alert and the watchdog can reach it, while `daemon::server` is private):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RestartReason { BinaryReplaced { path: PathBuf }, Stalled { operation: String, age_secs: i64 } }
pub(crate) enum RestartDecision { Restart, Refused(String) }
pub(crate) const MAX_RESTARTS_PER_HOUR: usize = 3;
/// Holds the pending reason the scheduler loop or the watchdog recorded.
#[derive(Default)]
pub(crate) struct RestartSignal { /* Mutex<Option<RestartReason>> */ }
impl RestartSignal { pub(crate) fn request(&self, reason: RestartReason); pub(crate) fn take(&self) -> Option<RestartReason>; pub(crate) fn pending(&self) -> bool; }
/// Record the request in `<work_dir>/daemon-restarts.json` (entries older than one hour
/// dropped) and decide. The fourth request inside an hour is refused with a reason naming the
/// count, the reasons and what still needs attention; a refused request is recorded too.
pub(crate) fn request_restart(work_dir: &Path, reason: &RestartReason, now: DateTime<Utc>) -> Result<RestartDecision>;
/// The refusal text of the latest recorded request when it was refused, for the stall alert.
pub(crate) fn last_refusal(work_dir: &Path) -> Option<String>;
```

New `src/daemon/server/restart.rs`:

```rust
/// `<binary> run --daemon-child <work_root> --take-over <pid>:<start_time>` plus the run's
/// config flags (`launch.rs::daemon_command`), the allowlisted environment
/// (`DaemonEnvironment::capture`), stdin `/dev/null`, stdout and stderr to a pipe.
pub(crate) fn successor_command(binary: &Path, work_root: &Path, config: &DaemonConfig, predecessor: ProcessIdentity) -> Command;
/// Wait up to `deadline` for the predecessor to release the singleton lock, then hold it.
/// `Err` naming the predecessor when it is still alive at the deadline.
pub(crate) fn take_over_lock(work_dir: &Path, predecessor: ProcessIdentity, deadline: Duration) -> Result<File>;
```

The restart file is written atomically with `O_NOFOLLOW` (`crate::fs::safe_fs` helpers, as
other control files are).

## Flow

1. The orchestrator loop (`src/orchestrator/core/run.rs`, `run_until_stopped`) checks
   `daemon_binary_replaced()` once per tick, before `run_tick`. When it returns a path and no
   request is pending, the loop records `RestartReason::BinaryReplaced` on the restart signal
   the daemon passed it (a new `Option<Arc<RestartSignal>>` on the orchestrator config;
   `RestartSignal` lives in `orchestrator/restart_budget.rs`) and keeps
   ticking. The loop never stops itself for a restart.
2. The daemon's accept loop (`src/daemon/server/lifecycle.rs`, `run_server`) takes a pending
   reason (at most once a second) and calls `request_restart`.
   - `Restart`: spawn `successor_command(recorded binary, ...)` and wait up to 10 s for its
     `WAITING_BYTE` (a new constant beside `READY_BYTE` in `launch.rs`; a `--take-over` child
     writes it once its arguments parsed). Byte received: set the shutdown flag, so the
     orchestrator leaves its loop at the next tick boundary and no operation is cut; join the
     threads (the existing bounded joins), run `cleanup()`, return; the process exits and the
     kernel releases the lock the successor waits on. No byte, or the spawn failed: kill the
     successor if it runs (verified identity), log the error once, record it in the restart
     file and keep serving: the old daemon still holds the lock, the socket and its loop.
   - `Refused`: log the reason once and keep serving; the stale daemon's spawns keep failing
     with the precise reason W2 and W3 surface. The pending reason is cleared either way, and
     the loop re-records it only after `daemon_binary_replaced()` reports a DIFFERENT
     fingerprint, so a refused or failed restart is not retried every tick.
   - The stalled-loop path (stall-recovery) records `RestartReason::Stalled` from the accept
     loop itself; on `Restart` it skips the wait for the orchestrator thread beyond the
     existing 5 s join (the thread is stuck) and proceeds the same way.
3. `serve` with `--take-over`: after `setsid`, write `WAITING_BYTE`, call `take_over_lock`
   (60 s), then continue as a normal start (record binary, remove the stale socket, publish
   PID and tokens, rotate the log, bind). It writes no `LOG_ACTIVE_BYTE`/`READY_BYTE`: nobody
   waits on them.
4. `src/commands/run/daemon_child.rs` and the run command's clap args (`rg -n 'daemon_child'
   src/cli`): a hidden `--take-over <pid>:<start_time>` parsed into a `ProcessIdentity`
   (refuse a malformed value), only valid with `--daemon-child`. The arg goes on
   `Commands::Run` in `src/cli/types.rs`; `src/cli/dispatch.rs` passes it through in
   `dispatch_run` only.

## Tests (beside each module)

- The accept loop with a successor binary that exits before writing `WAITING_BYTE` (a TempDir
  script `exit 1`): the old daemon keeps its lock, socket and loop, and the restart file
  records the failure.
- `--take-over` parsing refuses `12`, `a:b` and a value without `--daemon-child`.
- `daemon_binary_replaced` returns `None` until the replaced fingerprint has been seen twice
  10 s apart (inject the clock through a function argument; never sleep 10 s in a test).

## Size limits

`run_server` is pinned at 126 lines in `maintainability-baseline.txt`: every line you add must
be paid for by extracting code from it (the thread-join helper closure is the obvious
candidate), so it stays at or under 126. Never edit the baseline.

The live-test-lane plan edited both CLI files before you. Its live-profile P2 added
`Commands::Live` to `src/cli/types.rs` and lowered the exact ledger row of `dispatch` in
`src/cli/dispatch.rs`; its live-checks R1 routes `loom check --live`
in `run_check`. Never touch `dispatch` or `run_check`. P2 first moved the inline test module to
`src/cli/types_tests.rs` (392 lines before P2, about 357 after it; run `wc -l`), so the
`--take-over` arg (a doc line, the attribute, the field) leaves `types.rs` near 361 lines with
no condensing. Keep `types.rs` at or under 400.

## Traps

- Never fork. The successor is a fresh `exec` of the recorded binary, as `launch.rs` starts a
  daemon today.
- `spawn_daemon` uses `std::env::current_exe()`; the successor must use the RECORDED path:
  after a replacement, `current_exe()` names the old (possibly deleted) file.
- A restart while a git child of the old daemon still runs is safe: `process::run_bounded`
  starts every child in its own process group, so it survives the exit and finishes; the
  successor's first tick sees `index.lock` until then and retries.
- Sessions survive (tmux / native lanes are independent of the daemon); the successor's
  `initialize_run` re-adopts them. Host jobs left running are settled by
  `host_verify::runner::recover_after_restart`.
- Tokens are regenerated by the successor; sessions never hold them (`daemon/rpc.rs` reads
  `user.token` per request, and sessions are denied it).

## Done

Your one check after the stage compiles: `cargo test --lib orchestrator::restart_budget`.
