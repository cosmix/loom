# Planning Session Bridge
>
> Bridge

The bridge owns Claude Code sessions for `loom plan`: it starts each in its own tmux server, reads the
hook spool and the transcript, runs the state machine, delivers chat input, answers permission
requests and ends or recovers sessions. Contract: `doc/product/section_07.md`. Code:
`loom/src/plan_session/` (bridge, state, spool, transcript, input, attachments, launch) and
`loom/src/plan_server/chat/` (dispatch, guidance, the hub wiring).

## Module map

| File | Owns |
| --- | --- |
| `launch.rs`, `launch/script.rs` | `prepare` (session directory, sandbox files, trust entry) and `launch_script`: a 0700 launch script that `env -i` execs `claude` with `LOOM_PLAN_STATE` and `LOOM_PLAN_SESSION` pinned after the passed env |
| `tmux_io.rs`, `tmux_host.rs` | `pane_pid` (`#{pane_pid}:#{pane_dead}:#{pane_dead_status}`, fails on a dead pane), paste and key delivery; `TmuxHost` implements `SessionHost` over the lane's free functions in `orchestrator/terminal/tmux/` |
| `spool.rs`, `hook_cli.rs`, `permission.rs` | the ten `SPOOL_EVENTS` registrations, the locked append, the blocking `PermissionRequest` hook |
| `state.rs` | `StateMachine` over `Input::{Spool, Record, ...}` |
| `transcript/` | the record mapper: `mod.rs` (dedup, routing), `records.rs` (classification), `content.rs`, `turns.rs`, `tools.rs`, `usage.rs` |
| `input.rs`, `attachments/` | `check_send`, the 100 KB cap, the `<loom-attachments>` block (`format`, `parse`, `resolve`) |
| `bridge/` | `Bridge` and `SessionHost` (`mod.rs`, `host.rs`), start path (`start.rs`), per-session state (`session.rs`), the driver tick (`driver.rs`, `registry.rs`), delivery and interrupt (`flow.rs`, `turns.rs`), cards (`requests.rs`), browser commands (`commands.rs`), end and teardown (`ending.rs`, `stopping.rs`), restart recovery (`recovery.rs`), test fakes (`fake_host.rs`, `harness.rs`) |
| `plan_server/chat/` | `dispatch.rs` (start, send, interrupt, end from the browser), `guidance.rs` (`GuidanceSlot`), `mod.rs` (`install`, holder fill) |

## Naming and the session directory

Socket `loom-plan-<first 12 hex of the session id>`, tmux session `plan-<kind>`. `list_loom_sockets`
skips the `loom-plan-` prefix, so `loom init` and `loom clean` never report planning sockets. A person
watches with `tmux -L <socket> attach -r` (read-only; keystrokes typed into the pane would bypass the
bridge). The session directory holds `session.json` (the record: id, socket, pid `Option<u32>`, process
identity; `None` is the provisional record written before the pane exists), the launch script, `settings.json`,
`guard.json`, `events.jsonl` (the spool), `answers/<request-id>.json`, `done` and `marker` for pressure
runs and `codex.log`. The launch line carries no positional prompt: `/proc/<pid>/cmdline` is world
readable and an argv word would skip sanitising and the 100 KB check. The opening message is pasted
through the normal delivery path after the first `SessionStart { startup }`.

Claude Code 2.1.312 shows an "Is this a project you trust?" dialog in an untrusted directory and runs no
hook, `SessionStart` included, until it is accepted. Only `$HOME/.claude.json`
`projects[<exact canonical cwd>].hasTrustDialogAccepted = true` skips it; the same key in a `--settings`
file or a trusted parent directory does not. `launch::prepare` calls `fs::permissions::trust::trust_project`
(locked, atomic, keeps the file's mode and symlink) before every spawn, and the launch script pins `HOME` to the
bridge's home. Any path that spawns an interactive `claude` in a directory loom chose writes this entry first.
The capsule settings key `hasTrustDialogAccepted` in `session_settings/contents.rs` has no effect on this
version ([concerns](../concerns/planning-session-sandbox-limits.md#planning-session-bridge-follow-ups)).

## Spool, hooks and the event-name check

`loom plan hook <event> --session-dir <dir>` handles ten events. `PermissionRequest` blocks; `SessionStart`,
`UserPromptSubmit`, `Notification`, `Stop`, `StopFailure`, `SubagentStart`, `SubagentStop`, `PreCompact` and
`SessionEnd` append one line to `events.jsonl` and exit 0 with no output. The payload's `hook_event_name`
must equal the argument; a mismatch is not spooled. Payloads over 1 MiB are refused, strings over 64 KiB are
cut with `…[truncated]`, and the append goes through `fs::safe_fs::safe_append` under a lock.
`spool::try_read_payload` is the one payload reader (the guard calls it for its deny texts) and `spool::Tail`
the one tail reader (reset detection plus `read_lines`) for spool and transcript. The bridge dedups spool
lines on `(received_at, event, request_id, payload hash)`; a line offset is not stable across a reset onto a
rewritten file.

Hook payload fields confirmed on Claude Code 2.1.312: `StopFailure` carries `session_id`, `transcript_path`,
`cwd`, `prompt_id`, `effort`, `hook_event_name`, `error` (for example `server_error`) and
`last_assistant_message`, and a turn whose API call failed (after retries; `CLAUDE_CODE_MAX_RETRIES=0` makes
it immediate) ends with `StopFailure`, never `Stop`. `UserPromptSubmit` fires the moment a message is
submitted, also while a turn runs: Claude Code writes a `queue-operation` enqueue record and runs the
message as its own turn afterwards. `StopFailure` is treated as `Stop` everywhere (state machine, driver,
card closing).

## Permission requests

The hook writes the request to the spool (`auto_denied` inside `payload` when it denies on its own:
`summary_session` for Summary kinds, `hook_error` when `guard.json` fails to load; the helper is
`SpoolEvent::auto_denied`) and polls `answers/<request-id>.json` every 200 ms. Three clocks nest:
the server denies unanswered requests with `timeout` at `permission_deny_after` = 3590 s, the hook gives up
at `HOOK_WAIT` = 3595 s, and the hook is registered with `timeout_secs` 3600 for Claude Code. The server
answer always lands first. `DenyReason` (`model/mod.rs`) is a published wire enum: `holder`, `timeout`,
`session_ended`, `server_stopping`, `hook_error`, `interrupt`, `summary_session`. A request whose
`tool_input` the card cannot show whole (a `TRUNCATED` marker in any string, or pretty input over
`INPUT_CHARS`) is denied `hook_error` on arrival because Allow would approve the uncut call. Answer
request ids are validated (no path separators). A mismatched `hook_event_name` makes the hook print nothing,
and Claude Code then falls back to its own pane dialog.

## State machine and its two honesty rules

States: `Starting`, `Idle`, `Working`, `Permission`, `Ending`, `Ended`, `Failed` (`SessionState`); `Activity`
(`Idle`, `Working`, `Permission`) is tracked underneath `Ending`. `StateMachine::new` starts with activity
`Working` under `Starting`, so an end requested before `SessionStart { startup }` never looks idle.

1. A hook event never moves the state alone against the transcript: a main `user` or `assistant` record
   newer than the last idle event means `Working`. Only user and assistant records are fed as
   `Record { main: true }`: Claude Code writes `stop_hook_summary` and `turn_duration` system records after
   the Stop hook ran, so feeding them flips every Stop back to `Working`.
2. Only the main agent's events move the state. `SubagentStart`/`SubagentStop` change the running-subagent
   count and never the state.

The pending permission set clears only on a main Stop or StopFailure, an interrupt record, or per-id
removals; a main transcript record never clears it (another tool's result is a main record). The driver
applies the records and spool lines of one tick in time order, records first on a tie
(`Batch::in_time_order`, `bridge/registry.rs`); applying records first or spool first as blocks lets an
interrupt record reset the subagent count before the earlier `SubagentStart` lines count. The driver drains
the transcript before applying a Stop, so a late record of the closing turn stays in that turn. Process
identity is checked every `identity_interval` (2 s) through `process::{ProcessIdentity, IdentityStatus}`; a
death outside `Ending` is `Failed` (`FailureKind::Died`), a failed spawn `FailureKind::Spawn`.

## Delivery

Delivery happens only while `Idle`, one message at a time, from a server-side queue of at most
`MAX_QUEUED` = 20. Three tmux commands on the session's socket: `load-buffer -b loom-input -` with the text on
stdin (a writer thread, since 100 KB exceeds a pipe buffer), `paste-buffer -p -d` (bracketed paste, so newlines
do not submit), `send-keys Enter`. Never `load-buffer -w` (clipboard; crashes tmux 3.6a with a client
attached). A delivery counts when `UserPromptSubmit` arrives within `delivery_timeout` = 10 s; otherwise the
item is marked undelivered (the last `UNDELIVERED_KEPT` = 8 stay resendable). A failed paste of the opening
fails the live session at once through `Bridge::fail_live`. The delivered text, with the attachment block,
is capped at `MAX_DELIVERED_BYTES` = 100 000. `check_send` refuses text whose first non-whitespace
character, after skipping a leading U+FEFF, is `/`, except `/compact`. Attachments: at most 10, quotes cut
at 2,000 characters on a line boundary with `[…N more characters; read the source]` where N is the quote's
characters minus the shown lines joined by newlines.

## Interrupt, end, teardown

Interrupt is accepted only while `Working`: it denies the open permission request (`interrupt`), presses
Escape once per turn (a second Escape at an idle prompt opens the rewind picker), and drains the queue,
returning the texts as `ChatFrame::QueueReturned { texts }` without the attachment block; a message not yet
pasted when `end` is called is returned too. An interrupt record ends the turn itself and a later Stop is
ignored; nothing is delivered at the Idle the interrupt causes.

The final write (authoring end): wait for `Idle`, return the queue, refuse new messages, deny pending and new
requests when the end comes from the grace period or a server stop, paste the closing message, wait for its
Stop with no subagent running, then tear down, bounded by `final_write_bound` (600 s installed; `END_SLACK`
5 s past it). The holder's grace period is 600 s (`plan_server::authoring::GRACE`). Pressure sessions end at
their completion marker with no closing message and keep recorded children and `codex.log`. Summary sessions
end at their first main Stop with no subagent. `Bridge::abort` (second Ctrl-C) tears live sessions down,
gives a `Launching` session a `Teardown` end plan, and waits up to `LAUNCH_WAIT` 30 s for spawns in flight.
A failed teardown keeps `session.json` and the session in `Failed` (`TEARDOWN_FAILED` prefix); the retry runs
every `identity_interval`, by socket alone when the record has no pid.

## Recovery and its do-not-destroy rule

`recovery.rs` scans `sessions/` on start. It leaves alone, and prints the socket name of, any record whose
id differs from its directory name, whose socket is not `loom-plan-<12 hex of that id>`, whose JSON is
unreadable, or whose process identity is `Unverifiable`: missing evidence reads as "do not destroy" (a planted
record must never steer teardown at another session's server;
[tmux sweep mistake](../mistakes/tmux-backend.md)). A provisional (pid-less) record is torn down by socket.
A verified-alive session gets `deny_unanswered` answers, `deny_new`, the closing message and the same bound.
Recovery replays the spool to rebuild the permission cards.

## Chat model

`PlanMessage::Chat(Box<ChatFrame>)` with `ChatFrame::{Snapshot, Delta, Result, QueueReturned}`. Deltas
(`ChatDelta`): `Session`, `TurnAdded`, `TurnUpdated`, `ItemAdded`, `ItemReplaced`, `ItemRemoved`, `Queue`,
`Cleared`. `Item` variants: `User` (attachments, `Delivery`, `Origin`), `Thinking`, `Text`, `Tool`
(`tool_use_id`, `subagent`, `parent`), and permission and notice items. The model is rebuilt from the
transcript; the bridge keeps it as a cache. The mapper reads the path from `SessionStart`, accepting only
(`driver::checked_transcript`) an absolute path of plain components named `<session id>.jsonl`, opened
no-follow as a regular file; subagent transcripts come from a directory that must not be a symlink. It reads
complete lines, skips unknown records and any repeated `uuid`, groups assistant blocks by `message.id`
(usage counted once per id, `provider_data/claude.rs`), routes a record to the turn already holding its
`message.id` (a tool result to its call's turn), else the open turn, else an implicit turn that `begin_turn`
adopts. `changed_by_tool` returns a path relative to the project root only when it lies inside with plain
components; other `filePath` values stay as recorded. `SessionView.holder` and `away_until` are filled by
`plan_server::chat` (`with_holder`) on snapshots and `Session` deltas, not by the bridge. Delta delivery to a
new tab can repeat an item already in the snapshot ([concerns](../concerns/planning-session-sandbox-limits.md#planning-session-bridge-follow-ups)).

## Guidance slot and dispatch

`GuidanceSlot` (`RwLock<Arc<dyn GuidanceProvider>>`, `get`/`set`) in `plan_server/chat/guidance.rs` supplies
launch files and the closing message. Chat installation sets `ChecklistGuidance` (it refuses an offer id the checklist
does not currently offer, then defers to `FixedGuidance`, kept separate because `FixedGuidance` unit tests pin
acceptance of unoffered ids); `plan_server::guidance::install` then `set`s the shared `PlanGuidance`, which renders the
plugin and brief ([Planning Session Guidance](planning-guidance.md)).
`dispatch` validates the text before `take_authoring`, records the starter inside the guidance closure (the
session directory name is the session id), serialises starts with `ChatState::starting` (a plain send to a
running chat skips it), and `give_back` frees authoring a refused start took unless a chat is reserved or
runs. A direct `Bridge::start_session` caller must hold `starting` or a lease from before it reserves.

## Verification

`LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns a tmux-test skip into a failure. Inside the stage sandbox (no unix
sockets) `launch_starts_claude_in_its_own_server_and_identifies_it`,
`a_launch_whose_claude_exits_at_once_is_torn_down_and_reported` and `a_long_paste_reaches_the_pane_and_teardown_removes_the_socket`
print SKIP and count as passed. A host run at `7adae040` passed 462 `plan_session::` and 76
`orchestrator::terminal::tmux::` lib tests with none skipped. The operator end-to-end
(`loom/tests/plan_session_bridge_e2e`) is **unverified and failed, never passed**: on Claude Code 2.1.312 and
tmux 3.6 the first run stalled in `Starting` (the trust dialog, fixed), the second never drained the queue
(`StopFailure` unregistered, fixed), the third (`a7d644c2`, after trust, `StopFailure`, pane diagnostics and
per-round turn waits) timed out at 120 s waiting for the opening's delivery with state `Idle`; the cause is
not diagnosed. Interrupt Stop/StopFailure, `/compact` event order, `$TMPDIR` and the pasted-slash probe were
never reached. pty probes (outside tmux) showed a paste at the first `SessionStart` is submitted within
0.1 s with the bridge's full launch flags, so the stall is not the paste itself. The integration-verify stage
was completed by the operator with `--no-verify`; direct unsandboxed host test runs are not repeated. The
open diagnosis is post-merge work.

## Rebuilding Ids on a Turn

`Bridge::set_rebuilding` (`bridge/rebuilding.rs`) stores a turn's rebuilt summary ids through `transcript::Mapper::set_summaries_rebuilding(turn_id, ids)` and publishes the returned `TurnUpdated` deltas. The mapper owns the `turns` field, so a delta alone would show the ids once and lose them on the next snapshot or `TurnUpdated` for that turn. Attribution calls it as the turn's summaries are named (`name_on_turn`).
