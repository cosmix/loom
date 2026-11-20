# U2: `loom request status --wait`

Codex unit (`loom-codex-forwarder`, gpt-5.6-terra, effort xhigh), one module plus a test file, three
numbered steps.
Read `../common.md` first (its section "Pinned interfaces: daemon-owned-commits", bullet for
`request status --wait`), then this brief.

## Role and issue

Issue #22: after a session asks the daemon to commit, it has to find out whether the commit was applied, in one
blocking call, without a poll loop of its own. You add the waiting mode to the existing status command and
make it print the commit id the daemon recorded. The clap argument and its dispatch are written by C2; the
daemon handler that writes the commit id into the inbox ledger is written by C2 as well.

HARD RULES for you, the codex agent:

- Never run any git command that writes. Do not run git at all.
- Never read, write or list anything under a `.loom/` path (your tests use `tempfile::tempdir()` roots only).
- Do not run cargo. The orchestrator runs the proof command.
- Write no placeholders, no stubs, no unfinished markers. Do not edit any existing test line in the file.

## Files owned and files to read

Own exactly two files: `loom/src/commands/request/status.rs` (it exists: 229 lines, with a `mod tests`)
and `loom/src/commands/request/status/wait_tests.rs` (new). Add the wait code to `status.rs` and declare the
new tests at its end as `#[cfg(test)] mod wait_tests;`, so their module path is
`commands::request::status::wait_tests`. `status.rs` stays at or under 400 lines counting its tests, and
every function under 50 lines. Leave the existing `mod tests` untouched.

Read, in this order:

- `loom/src/commands/request/status.rs` whole (`wait_tests.rs` reaches its items through `use super::*;`): `execute`, `canonical_root`, `ReportedStatus`,
  `resolve_status`, `list_inbox_sessions`, `format_status`, and the existing tests.
- `loom/src/fs/inbox/status.rs` lines 1-57: `RequestStatus` and why the latest ledger row wins.
- `loom/src/fs/inbox/ledger.rs` lines 28-60 and 120-135: `LedgerRecord`, `LedgerOutcome`, `append_ledger`,
  `read_ledger`. Both are exported by `crate::fs::inbox`.
- `loom/src/orchestrator/core/inbox_drain/session_pass.rs` lines 160-170: how an applied note becomes the
  ledger `reason`.

## Pinned interfaces it provides and consumes

Provides: `pub fn execute(id: String, session: Option<String>, wait_secs: Option<u64>) -> anyhow::Result<()>`.
Without `wait_secs` it behaves exactly as it does today (same output, same exit codes, `format_status`
untouched). With it, the command polls the request state every 500 ms until the request is applied (exit
code 0, printing the commit id when the outcome carries one), refused (an error naming the refusal), or the
deadline passes (an error with the text `request <id> still pending after <N>s`). C2 passes the value of the
`--wait <SECS>` argument (1 to 600) as `wait_secs`.

Consumes: `crate::fs::inbox::{read_ledger, LedgerOutcome, RequestStatus, request_status, inbox_root,
validate_request_id}` and the existing `resolve_status` in this file. The commit handler settles an applied commit with
the note `committed <40-hex commit id>`; the ledger row for the request id then has `outcome = Applied` and
`reason = Some("committed <id>")`. `RequestStatus::Applied` carries no data, so you read the ledger yourself.

## Root cause and current behaviour

`execute` (lines 11-24) resolves the status once, prints one line and exits 1 only for an unknown id. There
is no wait. Applied prints a bare `<id>: applied`, so a session cannot learn the commit id. A session has no
way to wait except looping Bash calls, which the stage poll guard discourages. The relay hook moves a ticket
out of the scratch directory only after the Bash call that created it returns, so a status call made in the
same Bash call as the request would see `PendingRelay`; the wait mode must report that clearly instead of
polling forever.

## Step-by-step tasks

1. Pure waiting logic (no I/O, no clock reads, so tests inject both).
   - `const POLL_INTERVAL: Duration = Duration::from_millis(500);`
   - `enum Waited { Settled(ReportedStatus), TimedOut(ReportedStatus) }`.
   - `fn wait_for(resolve: &mut dyn FnMut() -> Result<ReportedStatus>, timeout: Duration, now: &mut dyn
     FnMut() -> Instant, sleep: &mut dyn FnMut(Duration)) -> Result<Waited>`: record `deadline = now() +
     timeout`; loop: resolve; `RelayedAwaitingDaemon` and `Applying` keep waiting, every other status ends the
     wait at once as `Settled`; when `now() >= deadline` return `TimedOut` with the last status; otherwise
     `sleep(POLL_INTERVAL.min(deadline - now()))`.
   - `fn outcome_line(id: &str, waited: Waited, secs: u64, note: Option<String>) -> Result<String>`:
     `Settled(Applied)` gives `<id>: applied: <note>` when a note exists, else `<id>: applied`;
     `Settled(Refused { reason })` is an error `request <id> was refused: <reason>`;
     `Settled(NotFound)` is an error `request <id> not found`; `Settled(PendingRelay)` is an error
     `request <id> was never relayed: the relay hook did not receive its ticket; run the command that created
     it again as its own Bash call`; `Settled(UnknownAfterRestart)` is an error `request <id> is unknown after
     a daemon restart: check the repository state, then run the command again if the change is missing`;
     `TimedOut(_)` is an error whose text is exactly `request <id> still pending after <secs>s`.
2. Ledger note and entry point.
   - `fn applied_note(root: &Path, session: Option<&str>, id: &str) -> Result<Option<String>>`: the sessions
     are `session` alone or every directory `list_inbox_sessions(root)` lists; for each, `read_ledger(root,
     sid)?`, take the LAST row with `record.id == id` and `record.outcome == Some(LedgerOutcome::Applied)`
     (iterate in reverse; the ledger holds an `applying` row and an outcome row per id) and return its
     `reason`. A missing ledger is empty, not an error (`read_ledger` already does this).
   - `pub fn execute(id, session, wait_secs)`: the existing preamble (`resolve_work_dir`, session from the
     argument or `LOOM_SESSION_ID`, scratch from `LOOM_SCRATCH_DIR`, `canonical_root`) is shared. With
     `wait_secs == None` run the existing body unchanged. With `Some(secs)`: build the `resolve` closure over
     `resolve_status(&root, session.as_deref(), scratch.as_deref(), &id)`, run `wait_for` with
     `Instant::now` and `std::thread::sleep`, read `applied_note` only when the result is `Settled(Applied)`,
     print `outcome_line` to stdout on success; on error return it (the process exits 1 through the caller).
     Do not call `std::process::exit` in the wait path.
3. Tests, in `status/wait_tests.rs` (the existing `mod tests` is not touched, so no existing line changes).

## Tests to write

Module `commands::request::status::wait_tests` (file `status/wait_tests.rs`, `use super::*;`). Use
`tempfile::tempdir()` for roots; the ledger fixture is
`crate::fs::inbox::append_ledger(root, "session-1", &LedgerRecord { id, kind: RequestKind::Commit, state:
None, outcome: Some(LedgerOutcome::Applied), reason: Some("committed <40 hex>".to_string()), at:
chrono::Utc::now() })`. The injected clock is a base `Instant` plus a `Cell<Duration>` that `sleep` advances.

- `wait_returns_at_once_when_the_request_is_already_applied` (no sleep call).
- `wait_polls_until_the_daemon_applies`: statuses `RelayedAwaitingDaemon`, `Applying`, `Applied`; exactly two
  sleeps of 500 ms.
- `wait_reports_a_refusal_with_its_reason` (the error text contains the reason).
- `wait_times_out_naming_the_request_and_the_deadline`: timeout 3 s, the status never settles; the error text
  equals `request <id> still pending after 3s`; the total slept time is 3 s; no sleep exceeds 500 ms.
- `wait_ends_at_once_on_not_found_and_on_a_ticket_the_relay_never_took` (no sleep call for either).
- `wait_treats_unknown_after_restart_as_an_error`.
- `the_applied_line_carries_the_commit_note_from_the_ledger` (`<id>: applied: committed <id40>`).
- `the_applied_line_without_a_note_is_plain` (`<id>: applied`).
- `applied_note_takes_the_latest_row` (an `applying` row first, then the `Applied` row with the note).
- `applied_note_scans_every_inbox_session_without_a_session_argument`.

## Patterns to copy, and the property not to copy

Copy the pure-function style of `resolve_status` and `format_status` (inputs passed in, no environment read)
and the injected closures for time. Do not copy the existing `std::process::exit(1)` in `execute` into the
wait path: a wait failure is an `anyhow` error so the caller decides the exit code. Do not take the FIRST
ledger row for an id.

## Traps

- Knowledge (`mistakes/concurrency-and-locking.md`): "Inbox Ledger Has Two Rows Per Request Id: the relay
  inbox drain appends an `applying` row when it starts processing a request, then an outcome row when it
  finishes ... any reader of the inbox ledger must take the LATEST row for an id, never the first match."
- A worktree session reaches the state directory through a symlink, and the inbox readers refuse to follow
  one: always read through `canonical_root` (the existing function), as `execute` does today.
- The daemon applies within one poll tick of about five seconds, so a real wait ends long before the 90
  second budget the doctrine uses (`--wait 90`, under the Bash tool's default 120 s timeout); never sleep
  longer than `POLL_INTERVAL` in one step.
- `read_ledger` and `append_ledger` open the state directory without following symlinks: tests must pass a
  real directory, not a symlink.

## The one check

None for you (do not run cargo). The orchestrator runs `cargo test --lib commands::request` once after all
six workers of this stage return.

## Report format

Reply with: the files changed and their line counts; the new functions; the assumptions you made about
`LedgerRecord` field names and the note text; anything in this brief that the code contradicts.
