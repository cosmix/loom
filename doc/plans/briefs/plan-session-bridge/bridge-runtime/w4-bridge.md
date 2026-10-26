# W4 (bridge-runtime): the Bridge

Read `doc/plans/briefs/plan-session-bridge/common.md` first ("Bridge API" is your public surface;
"State machine", "Permission flow", "The chat model" are the rules you drive). You run in wave 1,
alone. Everything you write is testable with a fake `SessionHost`; no tmux, no server.

## Files you own (write only these)

- `loom/src/plan_session/bridge/mod.rs` (`Bridge`, `BridgeConfig`, `BridgeError`, `BridgeEvent`,
  `EndOutcome`, `EndMode`, `SendOutcome`, `StartRequest`, `LaunchGuidance`, `GuidanceFn`,
  `TurnPins`, `RecoveryReport`, the public methods)
- `loom/src/plan_session/bridge/host.rs` (`SessionHost`, `LaunchSpec`, `Spawned`)
- `loom/src/plan_session/bridge/session.rs` (one live session: record, `StateMachine`, `Mapper`,
  `SpoolReader`, transcript readers, queue, pending permissions, timers)
- `loom/src/plan_session/bridge/driver.rs` (the tick)
- `loom/src/plan_session/bridge/commands.rs` (send, withdraw, permission, interrupt, end)
- `loom/src/plan_session/bridge/ending.rs` (final write, summary and pressure ends, teardown, recovery)
- `loom/src/plan_session/bridge/registry.rs` (`session.json`)
- `loom/src/plan_session/bridge/fake_host.rs` (`#[cfg(test)]` fake), `loom/src/plan_session/bridge/tests.rs`,
  `tests_queue.rs`, `tests_permission.rs`, `tests_end.rs`, `tests_recovery.rs`

## Types you define (beyond common.md)

```rust
pub struct LaunchSpec {
    pub id: String, pub kind: SessionKind, pub dir: SessionDir, pub socket: String, pub tmux_session: String,
    pub guidance: LaunchGuidance, pub extra_env: Vec<(String, String)>, pub opening_argv: Option<String>,
    pub project_root: PathBuf, pub runtime: RuntimePaths, pub loom_bin: PathBuf, pub claude_bin: PathBuf, pub home: PathBuf,
}
pub struct Spawned { pub identity: ProcessIdentity }
pub enum EndMode { FinalWrite, Teardown }
pub enum SendOutcome { Delivering { item_id: String }, Queued { queue_id: String } }
pub enum BridgeError { NoSession, NotRunning, Busy(&'static str) /* e.g. "recovery in progress", "a chat is running" */, Input(InputRefusal), Refused(String), Io(String) }
// RecoveryReport and RecoveredSession are in common.md "Bridge API" (each recovered session carries its kind and child records).
// registry.rs
#[derive(Serialize, Deserialize)]
pub struct SessionRecord { pub schema: u32 /* 1 */, pub id: String, pub kind: SessionKind, pub socket: String,
    pub tmux_session: String, pub pid: u32, pub start_time: Option<u64>, pub transcript_path: Option<PathBuf>,
    pub started_at: String, pub children: Vec<ChildRecord> }
#[derive(Serialize, Deserialize)]
pub struct ChildRecord { pub label: String, pub pid: u32, pub start_time: Option<u64> }
pub fn write_record(dir: &SessionDir, record: &SessionRecord) -> anyhow::Result<()>; // safe_fs, 0600, whole-file replace under flock
pub fn read_record(dir: &SessionDir) -> anyhow::Result<SessionRecord>;              // bounded no-follow
```

## Behaviour

Concurrency: one `Mutex<Inner>`. Commands only mutate state and return; every host call
(spawn, deliver, interrupt, teardown) runs on the driver thread or in `abort`, never under the
lock. Listeners (`subscribe`, `subscribe_chat`) are called after the lock is released, with the
deltas and events collected during the step, in order. A listener must not be able to deadlock
the bridge by calling back into it.

`start_session`:
1. Refused (`Busy`) while recovery runs, while stopping, and for an `Authoring` or `Pressure` kind
   while another authoring or pressure session is not Ended/Failed (one chat at a time, one
   pressure run at a time); a `Summary` while another summary session runs.
2. Mint the id (uuid v4), socket `loom-plan-<12 hex>`, tmux session `plan-<label>`; create
   `runtime.sessions/<id>/` and `answers/` 0700 through `safe_fs` (the `sessions/` dir too).
3. Call `guidance(&dir)`; an error removes the directory and is returned (`Refused`).
4. Insert the session in Starting, publish `Session` + `Started`, and leave the spawn to the
   driver (host.spawn), which on success writes the record (pid, start time) and only then
   reports the session started; on failure the state goes Failed with `Failure { kind: spawn }`
   and the message, and the directory is kept until the next chat clears the ended one.
   `mapper.expect_opening(req.opening_origin)`.

Driver tick (every `config.tick`), per session:
- spool: `SpoolReader::read_new`; each event through `StateMachine::apply` and the reactions:
  `SessionStart` records `transcript_path` (follow a change); `UserPromptSubmit` ->
  `mapper.begin_turn`, `pins.begin_turn("<id>/<turn>")`, a pending delivery confirmed (its user
  item `delivery: delivered`), `TurnStarted`; main `Stop` -> `mapper.end_turn`,
  `pins.end_turn`, merge with `mapper.changed_by_tool` into `changed_files` (by_tool true for
  paths the tool set has, before_hash from the pin), `TurnUpdated`, `TurnEnded`; `PermissionRequest`
  (not `auto_denied`) -> a `permission` item `p-<request id>`, pending, `expires_at = received_at + permission_deny_after`;
  `SubagentStart`/`SubagentStop` -> count, and `SubagentStop` publishes `SubagentStopped { id, agent_id }`; `PreCompact` -> notice; `SessionEnd` -> Ended.
- transcripts: main and every subagent file under `<transcript dir>/<id>/subagents/`, each with
  its own reader offset, lines to `mapper.ingest_main` / `ingest_subagent` (agent id and the
  `toolUseId` from `agent-<id>.meta.json`, read bounded), then
  `StateMachine::apply(Record { at, main })` for each main record.
- identity every `identity_interval`: `host.identity(...)`; `Dead` -> `ProcessGone`.
- permission expiry -> `write_answer(.., deny, reason timeout)`, item `denied`/`timeout`.
- delivery: when Idle and nothing is being delivered, pop the queue head and `host.deliver`; after
  `delivery_timeout` without `UserPromptSubmit` the item becomes `undelivered`. One at a time.
- pressure: `dir.marker()` exists -> teardown, `Ended { outcome: Marker }`.
- summary: first main `Stop` with `subagents_running() == 0` -> teardown, `Ended { Closed }`.

`transcripts(id)` returns the main transcript path from the last `SessionStart` and every
`subagents/agent-<id>.jsonl` found beside it; `turn_running()` is true while the chat session is
Starting, Working or Permission. `StartRequest.offer` (the `Offer::id` a chat started from, or
None) is copied into `SessionView.offer`.

Commands (`send`, `withdraw`, `answer_permission`, `interrupt`, `end`) exactly as section 07:
`send` takes the already formatted text (the server formatted it with `attachments::format_message`
and checked it) plus the attachments for the item; it adds a `user` item `s-<n>` with
`delivery: sent` when the session is Idle and nothing is delivering (`Delivering`), else queues it
(`Queued`, a `Queue` delta; first line = the text's first line cut to 120 chars).
`withdraw` removes a queued message that is not being delivered. `answer_permission` writes the
answer file (allow, or deny with reason `holder` and the note as message), updates the item, and
applies `PermissionAnswered`. `interrupt` (only while Working or Permission; see "Edge rules"): deny every pending permission (reason `interrupt`),
clear the queue returning its texts, record that the next Idle delivers nothing, ask the driver to
`host.interrupt`; a `notice` "Stopped" is added. `end(FinalWrite)`: `EndRequested`; the driver
returns the queue, waits for the activity to be Idle, delivers `(config.closing_message)()` (marking `mapper.expect_closing`), waits for
the next main `Stop` with no subagent running, then teardown; the whole bounded by
`final_write_bound` from the end request, after which it tears down as it stands. Pressure
sessions never get a closing message (`FinalWrite` acts as `Teardown`). `end(Teardown)`: deny
pending permissions (`session_ended`), teardown now.

Teardown: deny pending permissions first (`session_ended`, or `server_stopping` from `drain`/
`abort`), `host.teardown(socket)` (a failure keeps the record and directory: never destroy the
only handle; `mistakes/tmux-backend.md`), terminate verified children
(`process::identity::terminate_verified` on each `ChildRecord`), then remove the session
directory, set Ended, publish `Ended`. Exception: a Marker end keeps the children and the
directory until `release_session` ("Edge rules"); a failed `host.teardown` sets Failed instead
of Ended ("Edge rules"). The chat model of an ended authoring or pressure session
stays as `previous` in the next snapshot until a new chat starts (`Cleared` then).

Crash: `ProcessGone` outside Ending -> Failed (`Failure { kind: died, message: "The session stopped unexpectedly", }`
plus `last_activity` = the last main record time), pending permissions denied
(`session_ended`), `Ended { outcome: Failed(..) }`. Nothing restarts.

`recover()` (run once before the first chat; `start_session` is `Busy` until it returns): for each
`runtime.sessions/*/session.json` (read no-follow; an unreadable record is left alone and named):
`Unverifiable` identity -> left alone, its socket in `left_alone` with "process identity cannot be
verified" (missing evidence is "do not destroy"); `Dead` -> `host.teardown(socket)` if its socket
file exists, remove the directory, `torn_down`; `VerifiedAlive` -> pressure: teardown and
children terminated at once (`torn_down`); others: rebuild the session's state by reading its
spool and transcript from the start, then the `FinalWrite` flow bounded by `final_write_bound`
(`closed`). Recovery runs the sessions concurrently on the driver thread, not one after another. The report
is kept; `on_recovered(f)` runs `f` with it when recovery ends, or immediately if it has ended.

`drain()` (first Ctrl-C): `end(FinalWrite)` for authoring, a summary session finishes its current
task within the same bound (treated as `FinalWrite` without a closing message: teardown at its
next main Stop), pressure `Teardown`; return when all are Ended or the bound passed. `abort()`
(second Ctrl-C): teardown every session now, from the calling thread.

## Edge rules (fix round; each has a test below)

- Opening on argv (px-seams-R04): `LaunchGuidance.opening_on_argv` per common.md; the bridge
  validates the pattern before spawn and passes the opening in `LaunchSpec.opening_argv:
  Option<String>` (W5 appends it as the last shell-escaped word only when `Some`); default `None`.
- Opening message (px-security-04): the launch has no positional prompt. After the first
  `SessionStart { startup }` the driver delivers `guidance.opening_message` as the session's first
  turn through the normal path (`input::sanitize`, `input::check_delivered`, `host.deliver`), with
  `mapper.expect_opening(opening_origin)`; an opening that fails `check_delivered` fails the start
  (`Failure { kind: spawn }`).
- Turn end (px-state-03): an interrupt record that closes the open turn runs the same path as a
  main Stop (`mapper.end_turn`, `pins.end_turn`, `TurnUpdated`, `TurnEnded`). A Stop for a turn
  already closed is ignored. Both possibilities are handled: Claude Code may or may not run Stop
  after an interrupt.
- Self-started turns and late confirmations (px-state-08): `SelfStartedTurn` or a main record after
  the last idle event with no open turn opens one through the UserPromptSubmit path
  (`begin_turn`, `pins.begin_turn`, `TurnStarted`). A Stop with no `TurnStarted` for its turn
  emits no `TurnEnded`. After a delivery timeout the queue pauses until the next Stop or
  idle_prompt; a late `UserPromptSubmit` (or the matching user record) turns `undelivered` back
  into `delivered` (`ItemReplaced`).
- Interrupt (px-state-04): accepted only while the activity is Working or Permission (otherwise
  `BridgeError::NotRunning` and the queue stays); at most one `host.interrupt` per turn, no second
  Escape until an interrupt record or Stop arrives (a double Escape on an idle prompt opens Claude
  Code's rewind picker, which would swallow the next paste). The "next Idle delivers nothing" flag
  clears at that Idle, or 5 s after the press when no interrupt record arrives, and never
  suppresses the closing message. During Ending, interrupt is refused.
- Permissions (px-state-05, px-state-R03): several may be pending; the machine's set drives the
  state; the driver binds each request to the pending `tool_use` with the same tool name and an
  equal `tool_input` (the earliest unbound one) and applies `RequestResolved` only when THAT
  tool's result arrives. Every card the bridge closes for any reason (Stop, interrupt, resolved,
  teardown) is replaced `denied` / `session_ended` with note "the session no longer waits for
  this answer" and gets a deny answer file through `write_answer`. Expiry at 3590 s runs per
  request id whatever the set holds. An answer for a closed card is refused
  (`BridgeError::Refused`) and writes nothing more.
- Ending (px-state-06, -07): `EndRequested` returns the queue (`QueueReturned` to the requesting
  connection, through the returned texts of `end`) and the driver delivers nothing but the closing
  message while Ending; `send`/`withdraw` during Ending return `NotRunning` ("the chat is
  closing"). `end(FinalWrite)` takes an `EndCause::{Holder, GraceExpired, Draining, Caller}`:
  for `GraceExpired` and `Draining`, every pending request is denied at once (`holder` /
  `server_stopping`) and every new one on arrival while Ending; for `Holder` (the holder pressed
  End, or plan 11 closing the holder's chat before a run) and `Caller` (any other server-side
  caller: plan 10's summary stop, plan 11's run Stop) requests stay answerable. Call sites: plan 7
  dispatch `End` -> `Holder`, grace -> `GraceExpired`, `drain` -> `Draining`; plan 10
  `end(id, Teardown, Caller)`; plan 11 `end(id, FinalWrite, Holder)` for the open chat (its
  returned texts go to the holder as `QueueReturned`) and `end(id, Teardown, Caller)` for Stop.
  `end` therefore has the signature `end(&self, id, mode: EndMode, cause: EndCause) -> Result<Vec<String> /* returned queue */, BridgeError>`.
- Subagents (px-state-09): the count resets per common.md; the driver applies one synthetic
  `SubagentStop` for a subagent transcript whose last record is a final assistant message and that
  has not grown for 30 s.
- `/compact` (px-state-10): a delivered `/compact` applies `CompactDelivered`; its delivery is
  confirmed by `UserPromptSubmit`, a `compact_boundary` record or `SessionStart { compact }`.
- Provisional record (px-state-11): `write_record` with `pid: None` BEFORE `host.spawn`, completed
  after; a failed completion tears the session down at once. Recovery treats a provisional record
  whose socket exists as a teardown candidate (the socket name is ours and unique) and lists
  `sessions/*` directories with no record in `left_alone`.
- Recovery and permissions (px-state-12): before the final write of a recovered session, every
  spool `PermissionRequest` with no answer file gets a deny (`server_stopping`, message "the server
  restarted"), and new requests from recovered sessions are denied on arrival. Recovered sessions
  run their ends concurrently, so a new chat waits at most for the slowest, never the sum.
- Recovery events (px-state-13): after each recovered session's teardown the bridge emits
  `BridgeEvent::Ended` (`Closed` or `TornDown`) as well as filling the report.
- Failed teardown (px-state-18): sets `Failed` (`Failure { kind: died, message: "teardown failed: <err>" }`),
  keeps the record and directory, prints nothing itself (W6 prints the socket through
  `lifecycle::line` on the `Ended { Failed }` event), retries teardown each identity interval
  while the identity verifies alive, and does not block a new chat.
- Marker end (px-seams-01): `Ended { Marker }` tears down the tmux server but keeps verified
  children running and the session directory (with `codex.log`) until `release_session(id)`;
  children are still terminated on `Teardown`, `Failed`, `abort` and recovery.
- Spool replay (px-state-F01): `SpoolReader::read_new` re-reads from offset 0 after the file
  shrank or was replaced (`was_reset()`). The session keeps the set of applied event keys
  (`received_at`, `event`, `request_id`, plus the payload's `session_id`) and skips any event
  already applied, so a replaced or truncated `events.jsonl` never replays turns, permission
  cards or state changes. The set lives as long as the session.
- Token transcripts (px-seams-15): `add_token_transcript(id, path)` registers an extra reader the
  driver tails; each complete line goes to `mapper.ingest_tokens_only` and its deltas publish.

## Tests (fake host; no tmux)

`fake_host.rs` records every call and lets a test script the process (alive/dead) and append
spool and transcript lines into the session directory and a temp transcript dir (the transcript
path comes from a `SessionStart` spool line the test writes). Cover: start -> Starting -> Idle on
`SessionStart`; a send while Idle delivers once; a send while Working queues and is delivered at
the next Idle, one at a time; undelivered after the (short) timeout; withdraw; interrupt returns
queued texts, denies a pending permission with `interrupt`, delivers nothing at the next Idle;
permission allow/deny/timeout write the right answer files; end with final write delivers the
closing message only after Idle and tears down after the following Stop with no subagent running
(a `SubagentStart` without a matching stop delays it), and tears down at the bound otherwise; a
summary session ends at its first Stop; a pressure session ends when the marker appears and never
gets a closing message; a process death outside Ending -> Failed; recovery's three cases
(unverifiable left alone with its socket named; dead cleaned; alive closed), and `start_session`
refused while recovery runs; a second authoring or pressure start refused
while one runs, a second summary start refused while a summary session runs, an authoring start
allowed beside a running summary session; the record is on disk (pid and start time) before the
`Started` event and before the state leaves Starting; the socket is `loom-plan-` plus 12 hex
digits and the tmux session `plan-<kind>`; a `SessionStart` with a new `transcript_path` is
followed (records from the new file map, the old file is no longer read); a Bash-changed file
(pins report it, the mapper's tool set does not) appears in `changed_files` with `by_tool: false`
and its pin's before hash; `withdraw` of the message being delivered is refused; `drain` denies a
pending permission with `server_stopping`; `abort` tears every session down from the calling
thread; an ended chat is `previous` in the snapshot until the next authoring start, which
publishes `Cleared`; recovery tears down a live pressure session at once (no closing message) and
terminates its verified child record; listeners receive deltas in order and may call `chat_snapshot` from
inside a callback without deadlocking.

Fix-round tests (one each): the opening message is delivered by `host.deliver` after
`SessionStart` and never appears in `LaunchSpec`; an interrupt record with no Stop emits
`TurnEnded` and calls `pins.end_turn` once, and a later Stop changes nothing; a task-notification
turn emits `TurnStarted` and `begin_turn`, and a send in that window queues; `UserPromptSubmit` at
timeout + 1 s turns the item delivered and the queue paused until Stop; interrupt while Idle makes
no host call and sets no flag; two interrupts while Working make one press; interrupt during Ending
is refused; two concurrent permission requests keep Permission until both are answered; an answer
after Stop is refused; expiry removes the id; a request pending at grace expiry and one arriving
during a drain's closing turn are both denied at once and the closing message goes out; `end`
with a queued message returns it and delivers only the closing text; send during Ending is
refused; an interrupt with two subagents running resets the count so the final write tears down
at the next Stop; a stalled finished subagent is stopped after 30 s; `/compact` confirmed by
`SessionStart { compact }` with no UserPromptSubmit goes Idle; a provisional record with a live
socket is torn down by recovery and a record-less directory is in `left_alone`; a recovered session
with an unanswered request gets the deny file and the closing message; recovery emits `Ended` for
each session it closes or tears down; a failed teardown leaves the state `Failed` and a new chat
can start; a marker end with a live recorded child leaves the child alive and `codex.log` readable
until `release_session`, which then terminates it and removes the directory; an
`add_token_transcript` line adds to the turn's tokens; an `events.jsonl` replaced with a copy of
its own earlier lines plus one new Stop applies only the Stop (no second turn, no second
permission card, one `TurnEnded`).

## Proof

`cargo test --lib plan_session::bridge::` and `cargo clippy --all-targets -- -D warnings` from
`loom/`. Report anything surprising (and `loom memory note` it).
