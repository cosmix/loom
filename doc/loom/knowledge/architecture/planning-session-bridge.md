# Planning Session Bridge
>
>

The bridge owns Claude Code sessions for `loom plan`: it starts each in its own tmux server, reads the
hook spool and the transcript, runs the state machine, delivers chat input, answers permission
requests and ends or recovers sessions. Contract: `doc/product/feat-planner/section_07.md`. Code:
`loom/src/plan_session/` (bridge, state, spool, transcript, input, attachments, launch) and
`loom/src/plan_server/chat/` (dispatch, guidance, the hub wiring).

## Module map

| File | Owns |
| --- | --- |
| `launch.rs`, `launch/script.rs` | `prepare` (session directory, sandbox files, trust entry) and `launch_script`: a 0700 launch script that `env -i` execs `claude` with `LOOM_PLAN_STATE` and `LOOM_PLAN_SESSION` pinned after the passed env |
| `tmux_io.rs`, `tmux_host.rs` | `pane_pid` (`#{pane_pid}:#{pane_dead}:#{pane_dead_status}`, fails on a dead pane), paste and key delivery; `TmuxHost` implements `SessionHost` over the lane's free functions in `orchestrator/terminal/tmux/` |
| `spool.rs`, `hook_cli.rs`, `permission.rs` | the eleven `SPOOL_EVENTS` registrations, the locked append, the blocking `PermissionRequest` hook |
| `web_fetch.rs` | the `WebFetch` fetch policy (`FetchMode`, `FetchPolicy`, `FetchPolicyCell`), host parsing and allowlist validation |
| `state.rs` | `StateMachine` over `Input::{Spool, Record, ...}` |
| `transcript/` | the record mapper: `mod.rs` (dedup, routing), `records.rs` (classification), `content.rs`, `turns.rs`, `tools.rs`, `usage.rs` |
| `input.rs`, `attachments/` | `check_send`, the 100 KB cap, the `<loom-attachments>` block (`format`, `parse`, `resolve`) |
| `bridge/` | `Bridge` and `SessionHost` (`mod.rs`, `host.rs`), start path (`start.rs`), per-session state (`session.rs`), the driver tick (`driver.rs`, `registry.rs`), delivery and interrupt (`flow.rs`, `turns.rs`), cards (`requests.rs`), fetch rules and grant (`fetches.rs`), browser commands (`commands.rs`), end and teardown (`ending.rs`, `stopping.rs`), restart recovery (`recovery.rs`), the background-task table (`background.rs`, `background/`), the slash catalogue cell and class check (`catalogue.rs`) and a local delivery's barrier and settle (`local.rs`), test fakes (`fake_host.rs`, `harness.rs`) |
| `commands/` | the session's command table, the native table and the side process that reads the rest ([slash commands](plan-server.md#slash-commands-and-skills-in-the-planner-chat)) |
| `plan_server/chat/` | `dispatch.rs` (start, send, interrupt, end from the browser), `fetch_cmd.rs` (the card's fetch choices, Ask again), `guidance.rs` (`GuidanceSlot`), `mod.rs` (`install`, holder fill) |
| `plan_server/web_fetch/` | `web-fetch.json` in the runtime directory (temp file and rename), `GET`/`POST /api/web-fetch` (viewer read, holder write), the cell the bridge reads |

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

`loom plan hook <event> --session-dir <dir>` (`hook_cli.rs`) handles eleven events. `PermissionRequest` blocks;
`SessionStart`, `UserPromptSubmit`, `Notification`, `Stop`, `StopFailure`, `SubagentStart`, `SubagentStop`,
`PreCompact`, `SessionEnd` and `PermissionDenied` append one line to `events.jsonl` and exit 0 with no output;
`OwnerWatch` runs the owner watch (see Interrupt, end, teardown). In auto mode the classifier's refusals reach
the chat as `PermissionDenied`: the driver (`bridge/driver.rs`) posts an `Info` notice built by
`bridge/denied.rs::denied_notice` from the payload's tool and reason, sanitized through `input::sanitize`; a
missing or non-string tool gives the generic notice. A resumed driver replays the spool with an empty applied
set, so these notices repeat after recovery, as `PreCompact` notices do. The payload's `hook_event_name`
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
`session_ended`, `server_stopping`, `hook_error`, `interrupt`, `summary_session`, `settings`. A request whose
`tool_input` the card cannot show whole (a `TRUNCATED` marker in any string, or pretty input over
`INPUT_CHARS`) is denied `hook_error` on arrival because Allow would approve the uncut call. Answer
request ids are validated (no path separators). A mismatched `hook_event_name` makes the hook print nothing,
and Claude Code then falls back to its own pane dialog.

**Fetch rules (`web_fetch.rs`, `bridge/fetches.rs`).** A `WebFetch` request carries a `FetchView` on its card:
the parsed URL (punycode, cut at 2048 characters), `host`, `listable` (the host passes
`validate_domain`, which gates "Always allow <host>"), the answering rule and `matched` (the allowlist entry
that covered it, which the transcript line names). `Session::arrival_rule` runs after the recovered answer, the
cut-input deny and an ending's `deny_new`, then reads `BridgeConfig::fetch_policy` (a shared
`FetchPolicyCell` the server's `plan_server/web_fetch/` replaces on each Settings save) with the session's
`Flow::fetch_grant`: Never denies (`settings`), Always and a listed host allow, the grant allows, else the
card. Always and the grant allow only a non-local host name: an IP literal, `localhost`, `*.localhost`, a
single label or an unreadable URL goes to the card, because the fetch runs in the unsandboxed Claude Code
process. The policy file carries a `revision`; a Settings `POST` with a stale one gets 409 `stale_policy`,
and `validated` keeps the serialized policy under half of `MAX_BODY_BYTES` so Settings can always save it. A rule-answered request is added as an already closed item, never pending, so no browser pings.
The grant is set only by `Bridge::grant_fetches` (the holder's "Allow this session", through
`plan_server/chat/fetch_cmd.rs` and `on_chat`), refused on a non-`WebFetch` card, cleared by
`revoke_fetches`, and lives in `Flow`, so a new or recovered session starts without it. "Always allow"
lists `Bridge::fetch_host` (the server-parsed host, never the browser's), moves Ask to Listed, then
`answer_fetches` closes every open fetch card the rules now cover. Host matching uses the `url` crate:
exact host or a `.`-suffix, punycode, IP hosts never listed.

## State machine and its two honesty rules

States: `Starting`, `Idle`, `Working`, `Permission`, `Ending`, `Ended`, `Failed` (`SessionState`); `Activity`
(`Idle`, `Working`, `Permission`) is tracked underneath `Ending`. `StateMachine::new` starts with activity
`Working` under `Starting`, so an end requested before `SessionStart { startup }` never looks idle.

1. A hook event never moves the state alone against the transcript: a main `user` or `assistant` record
   newer than the last idle event means `Working`. Only user and assistant records are fed as
   `Record { main: true }`: Claude Code writes `stop_hook_summary` and `turn_duration` system records after
   the Stop hook ran, so feeding them flips every Stop back to `Working`.
2. Only the main agent's events move the state. `SubagentStart`/`SubagentStop` feed the background-task
   table and never the state. Claude Code fires a new `SubagentStart` for the same `agent_id` each time it
   wakes an agent (a background coordinator woken by its children's notifications); the table reopens the
   entry, ends it only on evidence and sends `SubagentStopped` once per id. The close at the closing Stop
   needs `BackgroundTasks::blocking() == 0` ([the table](#background-tasks-in-the-planner-chat), [mistake](../mistakes/spurious-waiting-for-input.md#a-re-woken-subagent-kept-the-subagent-count-up-and-the-chat-stayed-ending)).

The pending permission set clears only on a main Stop or StopFailure, an interrupt record, or per-id
removals; a main transcript record never clears it (another tool's result is a main record). The driver
applies the records and spool lines of one tick in time order, records first on a tie
(`Batch::in_time_order`, `bridge/registry.rs`); the table's times and ends are order-robust, so a replay
that reads the spool before the transcript builds the same table. The driver drains
the transcript before applying a Stop, so a late record of the closing turn stays in that turn. Process
identity is checked every `identity_interval` (2 s) through `process::{ProcessIdentity, IdentityStatus}`; a
death outside `Ending` is `Failed` (`FailureKind::Died`), a failed spawn `FailureKind::Spawn`.

## Delivery

Delivery happens only while `Idle`, one message at a time, from a server-side queue of at most
`MAX_QUEUED` = 20. Three tmux commands on the session's socket: `load-buffer -b loom-input -` with the text on
stdin (a writer thread, since 100 KB exceeds a pipe buffer), `paste-buffer -p -d` (bracketed paste, so newlines
do not submit), then `send-keys Enter` `ENTER_AFTER_PASTE` = 150 ms later from a short-lived `loom-paste-enter` thread
(`tmux_io.rs` `paste_text`, `enter_after_paste`): the paste stays synchronous so a failed paste still fails the opening,
and the driver thread never sleeps, since one tick serves every session. A lost Enter is only logged. Never `load-buffer -w`
(clipboard; crashes tmux 3.6a with a client attached). A delivery counts when `UserPromptSubmit` arrives within
`delivery_timeout` = 10 s; otherwise the item is marked undelivered (the last `UNDELIVERED_KEPT` = 8 stay resendable).
A failed paste of the opening fails the live session at once through `Bridge::fail_live`.

**Background turns do not confirm a different message.** Every Agent call in a planning session runs in the background, and a finished subagent sends an `<agent-message>` hand-back and a `<task-notification>`, each opening a turn of its own (queued until Stop when the session is mid-turn; a mid-turn hand-back arrives as a `queued_command` attachment and still fires `UserPromptSubmit`). `confirm_prompt` (`bridge/flow.rs`) treats a prompt led by one of `transcript::BACKGROUND_TAGS` (`<task-notification>`, `<agent-message`; the same list the mapper uses to recognise these prompts) as a background prompt: it confirms the pasted delivery only when its sanitized text equals the delivery's `match_text` (a message the user wrote starting with the same tag), and otherwise only late-confirms by text (`late_confirm`). Claude Code can start such a turn between the bridge's idle and the paste, and the pasted message then waits behind it unsubmitted; it confirms by its own prompt, and when that comes after `delivery_timeout` the item shows Undelivered with Resend in the meantime ([the mistake](../mistakes/planner-ui-repair.md#a-question-was-reported-unanswered-after-a-background-turn)). Test: `tests_background_prompt.rs`.

**An unsubmitted opening.** `bridge/opening.rs` covers an opening whose Enter was swallowed: `enter_again` presses Enter
again (never pastes again; `SessionHost::press_enter`, a default no-op that `TmuxHost` and `fake_host.rs` override) every
2 s, at most twice, inside `delivery_timeout`, and only while the session is live and `Idle` (Enter on a permission prompt
would answer it; a chat message never gets one, as its box may hold half-typed text). `unsubmitted_opening` fails a Summary
session whose opening still has no `UserPromptSubmit` at `delivery_timeout` through `Bridge::fail_live` with "opening not
submitted within <timeout>", so the scheduler requeues its ids. An opening whose `UserPromptSubmit` and transcript match
were both lost therefore keeps delivering until the session next goes `Idle`, then fails once. `Delivering.opening` is
`Option<u8>` (`Some(n)` = the opening, `n` re-presses; `session.rs`).

**The pasted-content wrapper.** Claude Code expands each pasted chunk of a prompt, in the transcript and in the
`UserPromptSubmit` hook, as two newlines, an `<pasted_content id="X">` line, the chunk, a `</pasted_content id="X">` line
and a newline; a short single-line prompt is recorded bare, and a paste that arrives split leaves its tail outside the
wrapper. `attachments/pasted.rs::unwrap_pasted` removes every complete expansion wherever it sits (the id is 1 to 32
ASCII alphanumerics, `_` or `-`) and returns a message with no complete wrapper unchanged. `parse_message` is
`unwrap_pasted` then `parse_unwrapped`. The transcript mapper (`transcript/mod.rs`) unwraps before `cap_chars`, because
unwrapping after the 100 000-character cap would lose the close tag of a maximum-size delivery; the hook path
(`bridge/flow.rs` prompt hook) calls `parse_message`. Every comparison of a sent message with what Claude Code recorded
goes through `input.rs::match_key` on both sides (sanitise, remove wrappers, collapse whitespace): the mapper's pairing
of a record with its `s-` item (`transcript/opening.rs`), `RecordMark::HumanUser`, `Delivering.match_text` and
`confirm_prompt`. A record that pairs with one sent item shows the sent item's own text and attachments. Known limits:
two sent messages that differ only in whitespace share a key; a CRLF-terminated tag line is not unwrapped
([the mistake](../mistakes/planner-ui-repair.md#claude-code-wrapped-delivered-prompts-and-every-text-match-broke-silently-2026-11-03)).

**Slash messages.** `check_send(text, &CommandTable)` lets a message through whose first token the session's command table allows, as one line of at most 300 characters with no attachments; the table, the side process that fills it, the re-check just before the paste, and the confirmation, barrier and settle per command class are in [Slash commands and skills](plan-server.md#slash-commands-and-skills-in-the-planner-chat). A local command (`/compact`, `/context`, a fork skill) holds the delivering slot until it settles, so the next queued item waits.

The delivered text, with the attachment block, is capped at `MAX_DELIVERED_BYTES` = 100 000. Attachments: at most 10, quotes
cut at 2,000 characters on a line boundary with `[…N more characters; read the source]` where N is the quote's characters
minus the shown lines joined by newlines.

## Interrupt, end, teardown

Interrupt is accepted only while `Working`: it denies the open permission request (`interrupt`), presses
Escape once per turn (a second Escape at an idle prompt opens the rewind picker), and drains the queue,
returning the texts as `ChatFrame::QueueReturned { texts }` without the attachment block; a message not yet
pasted when `end` is called is returned too. An interrupt record ends the turn itself and a later Stop is
ignored; nothing is delivered at the Idle the interrupt causes.

The final write (authoring end): wait for `Idle`, return the queue, refuse new messages, deny pending and new
requests when the end comes from the grace period or a server stop, paste the closing message, wait for its
Stop with no blocking background agent (`BackgroundTasks::blocking() == 0`), then tear down, bounded by `final_write_bound` (600 s installed; `END_SLACK`
5 s past it). The holder's grace period is 600 s (`plan_server::authoring::GRACE`). Pressure sessions end at
their completion marker with no closing message and keep recorded children and `codex.log`. Summary sessions
end at their first main Stop with no blocking background agent. `Bridge::abort` (second Ctrl-C) tears live sessions down,
gives a `Launching` session a `Teardown` end plan, and waits up to `LAUNCH_WAIT` 30 s for spawns in flight.
A failed teardown keeps `session.json` and the session in `Failed` (`TEARDOWN_FAILED` prefix); the retry runs
every `identity_interval`, by socket alone when the record has no pid.

**Agent end** (`bridge/done_fence.rs` `Session::agent_end`, run at a main Stop): an authoring chat whose turn's last assistant text ends in a valid `loom-done` fence (`transcript/done_fence.rs`, `Mapper::turn_done`: the fence is the final block, at most 64 KiB, JSON `{decisions, questions}` of at most 50 strings of 500 characters; a user's or quoted fence, or one followed by more text, is no fence, and an invalid one only logs) ends with `EndMode::FinalWrite`, `EndCause::Agent` and `Closing::Skip`: no closing message is pasted, because the planner already wrote its notes, but the close gate and `final_write_bound` stay (`Teardown` would bypass the background gate). A queued or delivering message cancels the end. Validation reads the full text; the display item is capped at 64K characters, so `transcript/done_fence.rs` keeps the final fence in the capped markdown and the web (`chat/closing.ts` `endedByAgent`, `chat/kickoff-fence.ts`) sees every fence the server acts on. The web infers the cause from the transcript, and `STATE_NAME.ending` ("Writing final notes") is imprecise for a chat the agent ended ([follow-ups](../concerns/planner-ui-repair-followups.md)). The close gate is a state check, `check_close` (`bridge/driver.rs`), called from `on_idle` and every tick, because background work drains after the main Stop: it sets `close_ready = Closed` once the closing Stop was seen, activity is idle, nothing is opening or delivering and no background agent blocks. After a finished closing turn, self-started background records (a peer hand-back writes a `SelfStartedTurn` mark) count as table evidence and do not reopen activity; real prompts and assistant records still do ([the mistake](../mistakes/planner-review-fixes.md#a-background-hand-back-reopened-activity-after-the-closing-stop)). The contracts `a-done-turn-ends-the-chat-without-a-closing-write`, `a-queued-message-cancels-the-agent-end`, `an-agent-end-waits-for-running-background-agents` and `a-user-or-quoted-fence-never-ends-the-chat` pin it (`EndCause::Agent` is named once as a build pin and the end is asserted as `EndOutcome::Closed`, which a `Teardown` end, `TornDown`, fails).

**Owner watch** (`owner_watch.rs`, started by `launch/script.rs` as the `OwnerWatch` hook): a session whose
server was SIGKILLed has no one to end it, and `remain-on-exit` keeps its tmux server after claude dies, so the
watch polls every `OWNER_POLL` (15 s) and tears the tmux server down once the server lock holder has been
absent for `OWNER_GRACE` (15 min); every sighting of an owner resets the grace, and it returns when the
session's socket is gone. The tmux socket is the session's liveness, independent of the Claude process
identity. `read_holder_pid` reads the lock without taking the singleton lock or following a symlink.
`hook_cli::owner_watch` calls `setsid` before watching, because a plain `&` job of the launch script's
non-interactive `sh` dies with the pane leader; it does it inside the `watch_target` branch, not on the first
line, so an in-process test of `run(OwnerWatch)` does not detach the test binary from its terminal session. A process that leads its own group cannot `setsid` (EPERM), so the regression test (`tests/plan_owner_watch_session.rs`) places the hook in a group another process leads with `Command::process_group`.
`watch_target` validates the session record, the guard policy and the socket name as recovery does. Known
limits ([follow-ups](../concerns/planner-ui-repair-followups.md#planner-redesign-follow-ups)): the watch starts
once and is not restarted, a failed kill-server gets one attempt, and teardown does not call
`terminate_children`.

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
`orchestrator::terminal::tmux::` lib tests with none skipped.

**Conformance probes.** What the bridge assumes about a real Claude Code is recorded from Claude Code 2.1.321
(`p1-status` ran on 2.1.322) in `loom/tests/fixtures/plan_session/probe-notes.md`, with the transcripts, spool
files and raw notes it cites under `loom/tests/fixtures/plan_session/`: P1 for background tasks (launch
results, notification carriers and status values, `TaskStop`, `background_tasks` on `Stop`, interrupt, resume,
compact) and P2 for slash commands and skills (per-paste matrix, record shapes per class, timing, the side
process, row shape and built-in rows). The harness is `loom/tests/plan_session_bridge_e2e/probes.rs` and
`probes/`: one `#[ignore]` test per probe, driving a plan session through the bridge over tmux in a throwaway
project under a copied HOME. No stage runs them (they need a logged-in `claude` and the network). The operator
reruns them on the host, outside any sandbox, after each Claude Code upgrade:

```bash
cd loom && cargo test --test plan_session_bridge_e2e probes:: -- --ignored --test-threads=1
```

A rerun that contradicts a decision in the planner amends the plan first; the rerun of
`p2_side_process_initialize_and_commands_changed` shows the rows a newer native command adds, and
`p2_side_process_max_turns_and_loom_ui` checks `loom-ui` on a host that has it. The fixtures replay offline
through `bridge/fixture_replay.rs` and the bridge contract tests, so a stage can check the table against real
record shapes without Claude Code. What the probes did not observe is listed under "Not observed" in the notes
and in [the follow-ups](../concerns/planning-session-sandbox-limits.md#planner-commands-native-table-parity).

**Operator end-to-end.** The older round-based end-to-end (`loom/tests/plan_session_bridge_e2e`, rounds
outside `probes/`) last ran on Claude Code 2.1.312 and tmux 3.6: the first run stalled in `Starting` (the trust
dialog, fixed), the second never drained the queue (`StopFailure` unregistered, fixed), the third (`a7d644c2`)
timed out at 120 s waiting for the opening's delivery with state `Idle`; the cause was not diagnosed. Its
interrupt Stop/StopFailure, `/compact` event order and `$TMPDIR` rounds were never reached. The 2.1.321 probes
delivered their pastes and queued items through the same bridge, so the stall has not reproduced there, but no
full-round run has passed since. pty probes (outside tmux) showed a paste at the first `SessionStart` is
submitted within 0.1 s with the bridge's full launch flags.

## Rebuilding Ids on a Turn

Attribution (`plan_server/summaries/attribution.rs`) tracks the chat session's open turn and the summaries that session claims during it. `Bridge::set_rebuilding` (`bridge/rebuilding.rs`) stores those ids on the turn through `transcript::Mapper::set_summaries_rebuilding(turn_id, ids)` and publishes the returned `TurnUpdated` deltas. The mapper owns the `turns` field, so a delta alone would show the ids once and lose them on the next snapshot or `TurnUpdated` for that turn. Attribution calls it as the turn's claimed summaries are named (`name_on_turn`). Only the correction rebuild a chat makes itself (`loom plan summary brief --note` and `seal`, product spec section 10 "When the summary is wrong") claims summaries inside a turn: an authoring turn rebuilds no other summary, so most turns carry no ids. The footer's rows read "Rebuilding" while the turn is open and "left for the background rebuild" once it ends (`chat/turn-footer-rebuilt.tsx`).

## Planner Kickoff, Draft-All Teams and the Feature Web Model

Authoring sessions that start a feature or draft many documents at once, the session scope that confines them, and how the browser models features. The document side (features, ids, offers) is in [planning-document-model.md](planning-document-model.md).

### Session scope

Kickoff and draft-all runs keep `SessionKind::Authoring`; the limit on what they may write is a per-session `scope.json` in the session directory (`plan_session/scope.rs`: `SCOPE_FILE`, `SessionScope::{Open, Kickoff, Team}`, `load`, `write`). The chat guidance closure writes it before launch; the sandbox builder and the PreToolUse guard (`plan_session/guard/mod.rs`, which re-reads it on every authoring doc write) read it. The file lives under the runtime root, which a session can neither read nor write (only drafts, claims and the lock are writable).

A new `SessionKind` variant or new fields on `StartRequest`, `LaunchSpec`, `SandboxInputs` and `GuardPolicy` would have broken every exhaustive match and about 25 struct literals outside the stage; a session-dir file needs neither and keeps `kind == Authoring` checks correct.

- **Kickoff.** `SessionScope::Kickoff { existing, grant }` allows no write until the holder confirms one grant: a size (`feature` or `plan`) and a name (`is_feature_name`). `arm_kickoff` refuses an invalid name, a target that exists in the scope's `existing` list or in a fresh snapshot of the project (`scope_snapshot.rs`), a live session already on it, and a second confirmation. The grant allows exactly the target document and, for a plan, paths inside its brief bundle. The holder's `confirm` runs in `plan_server/chat/kickoff_cmd.rs` (holder-only, refused read-only, serialised by `ChatState::starting`).
- **Team.** `SessionScope::team(documents, bundles)` allows only the listed documents (1 to 64, plain relative paths, no duplicates) and paths inside the listed brief bundles, and turns peer tools on (`peer_tools`). The documents come from `planning::guidance::team::draft_run` for the offers `draft-all-sections` and `draft-all-plans` (`MAX_TEAMMATES` 5 caps the teammates; the offer needs a feature). `chat/team.rs::extra_env` sets the agent-teams environment variable for those two offers only.
- **Known limit.** The scope binds file tools through the PreToolUse guard. A Bash redirect under `doc/` that the holder approves is bounded by layer 1 (the locked set built at spawn, which cannot follow a mid-session grant), not by the scope.

### State on the wire

- Kickoff state is parsed in the chat layer from the last fenced `loom-kickoff` block in an assistant text item (`chat/kickoff.rs::parse_shape`: body at most 16 KiB, strings 500 characters, lists 40 items) and sent as `PlanMessage::Kickoff`.
- Teammate transcripts are split out of subagent ingestion in `Feed::read_agents` (agent meta `taskKind` `in_process_teammate`); their lines feed token usage and a `TeamTracker` (`plan_session/bridge/team.rs`), surfaced as `BridgeEvent::Team` and `PlanMessage::DraftTeam`. `ChatDelta` and `ChatSnapshot` are unchanged, so the web schemas gained two message arms only.
- The server has no file-move route: `Offer::MoveIntoFeature` answers `StartError::FileMove`, so the home shelf's "Move into a feature" is built as a chat start whose opening carries the move instructions.

### Browser model

- `projectAtom` holds `{ checklists, standalone, moves }` from the server's `hello` and `checklist` messages (`web/src/plan/api/socket.ts`); `checklistAtom` stays a writable derived atom (the active feature's checklist, else the sole feature's) so single-checklist consumers and tests keep one seam.
- Routes: `/` home (feature cards, standalone shelf), `/f/:feature` the feature's step list and draft-team panel, `/new` the kickoff landing; concept routes are in [plan-server.md](plan-server.md#browser-routes-page-state-and-view-state).
- Set-only server atoms (`kickoffAtom`, `draftTeamAtom`, `treeAtom`) are never cleared by the server. A consumer scopes them by session id or feature and knows who fills them (`useFileSocket` fills `treeAtom`); a component that reads one unscoped shows another session's state.
- The Settings reading face and size are per-project preferences through `projectPref` (`plan/chat/prefs.ts`, exported for reuse); see [the reading face](../conventions/web-dashboard-typography.md#planner-reading-face).

## Background tasks in the planner chat

The bridge keeps one `BackgroundTasks` table per planning session (`loom/src/plan_session/bridge/background.rs`, with `background/entry.rs` for the entry and its ranked fields and `background/inputs.rs` for the evidence). It lists every background agent and shell the session launched, from launch to the evidence that ended it. `Idle` keeps its meaning (the main agent waits for input); background work is a separate model, drawn beside it. The table is never persisted: recovery re-reads the spool and the transcripts from offset 0 and rebuilds the same table. Facts about Claude Code 2.1.321 behind it: `loom/tests/fixtures/plan_session/probe-notes.md` ("P1. Background tasks").

### Entries and states

- `BackgroundTask`: `id` (agent id or shell task id), `kind` (`Agent`/`Shell`), `label`, `agent_type`, `tool_use_id`, `started_at`, `last_activity_at`, `state`, and for agents `steps`, last tool and a report preview. `steps` counts assistant entries (one per content block), so the column says "steps".
- `TaskState`: `Running`, `ToolWait`, `Stalled` are unended; `Done`, `Failed`, `Stopped` are ended (`is_ended`). On the wire `Stalled` is `stalled`, the UI word is "stalled?".
- Each field keeps the value of its highest-ranked source (`entry.rs` `Rank`): the launching call's description, else the meta's, else the `Stop` list's. `started_at` is the minimum over launch evidence and `last_activity_at` the maximum over evidence naming the task (a `Stop` list never counts, adoption only sets `started_at`), so a replay reads the spool before or after the transcript and gets the same table. A transcript file's timestamps can run backwards (a queue-operation enqueue is written before the records it precedes); `Batch::in_time_order` keys each line by a non-decreasing floor, so a fixture-replay cut-off uses the floored time.
- Not tracked: teammates (a teammate's meta removes its id for good; `DraftTeamPanel` shows them) and a `type: "subagent"` `Stop` entry with an unknown id.

### Launch evidence

Each of these creates an entry or fills the fields it lacks: an `Agent`/`Task` `tool_result` with `toolUseResult.status == "async_launched"` (every Agent call launches this way); a `SubagentStart`; a `<forked-skill-launch>` system record; a `Bash` `tool_result` with `toolUseResult.backgroundTaskId`; and a `Stop` list entry of `type: "shell"` with an unknown id, which adopts a shell launched inside a subagent as a session-level shell entry (no `tool_use_id`, label from the list). Subagent transcripts are not parsed for shell launches. A `SubagentStart` stamped after an agent's end reopens it (Claude Code re-wakes one agent id many times).

### A task ends only on evidence

- **The `Stop` list.** Each main `Stop` payload carries `background_tasks`, the running tasks of the session; an unended entry that started before that list's `received_at` and is absent from it ends `Done`. The rule holds in either read order. A `SubagentStop` list lags (the stopping agent still lists itself) and is never read. Bridge contracts give every `Stop` a `background_tasks` list; a `Stop` without the key is never relied on to mean an empty list.
- **The notification** gives the outcome: `<status>` `completed` is `Done`, `failed` is `Failed`, anything else `Stopped`, read from the status element and never the summary wording (`transcript/notification.rs`). It still opens the main agent's turn as `SelfStarted`; the table call sits beside that path. It sets the outcome of an entry the list already ended (`Done` becomes `Failed`) and sends no second `SubagentStopped`. An unknown id adds an ended entry. The interim agent variant ("is waiting on its own background work") ends the agent like `finished`.
- **`TaskStop`** `tool_result` (`toolUseResult.task_id`) ends a known entry `Stopped`; a stopped shell sends no notification, so this result and the next `Stop` list are its only end evidence.
- **Agent evidence, known ids only:** the hand-back record, the `tool_result` with `toolEndsTurn`, meta `finished: true` (read once by `AgentFeed::classify`, so it counts after a restart) and a `SubagentStop` whose `agent_id` is in the table; a `SubagentStop` for an unknown id (an internal fork, `agent_type: ""`) is ignored. A list-only (weak) end yields to `TaskStop`, agent evidence or a notification, and a reopen needs an agent assistant record or a `SubagentStart` stamped after `ended_at` (the killed agent writes user records after its `TaskStop`).
- **Stall.** An agent with none of this evidence turns `Stalled` when its transcript has been quiet past `STALL_BUDGET` (600 s, `bridge/session.rs`); it stays in the table and later evidence ends it. Shells never stall. `BackgroundTasks::reevaluate(now, stall_budget)` runs on the driver tick, takes no debounce (the fold state is cached per agent at line time, so a tick calls no fold and allocates nothing until a stall) and is the only call that reads a clock.
- **`SubagentStopped`** is sent once per agent id, ever (the stopped-once set survives clears), so `Attribution::on_subagent_stopped` costs the transcript once.
- **Clearing.** One Escape proves nothing about any task. `agents_killed` clears every entry; `SessionStart` with source `startup`, `resume` or `clear` and `SessionEnd` clear the table; `compact` keeps it.

### Close gate and Restore

`check_close` (`bridge/driver.rs`, called from `on_idle` and the tick) closes a summary or ending session at a `Stop` only when `BackgroundTasks::blocking() == 0`: agent entries in `Running` or `ToolWait`; shell, `Stalled` and ended entries do not block. The contracts `a-summary-session-waits-for-a-launched-agent` and `closing_contract_tests.rs`/`tests_end.rs` fail when the check is dropped. Restore on the web is blocked while any entry is unended (adopted shells and `stalled?` included) because a background task can still write files.

### Transport

- **Bridge event.** `BridgeEvent::Background { id, tasks }` is sent when the table changes, coalesced to one per driver tick (a flag set by each change, flushed in the timers after `reevaluate`); `SubagentStopped` stays immediate. `SessionView` carries no running-subagent count; `ChatSnapshot`, `ChatDelta` and `EndedChat` carry the changed view.
- **Plan server.** `plan_server/chat/background/` (`mod.rs`, `groups.rs`, `state.rs`) registers its own bridge listener and hub `on_open` hook for the chat session only, runs the `planner-tokens` thread (see [token-flow-sources](token-flow-sources.md#planner-token-pass)) and joins the table with the token rows by agent id outside the bridge mutex: a flow row with no table entry is dropped, a table row with no flow row shows no token figures. `PlanMessage::Background { session, tasks }` (`wire/background.rs`, fixture `web/src/plan/api/fixtures/background.json`) carries `{model, total, tokens_per_min, active}` per task, broadcasts when the table changes or an active agent's total or rate changes (at most once per 5 s pass), is replayed by `snapshots()`, and on `Ended` or an emptied table sends an empty frame. Strings from transcripts, meta files and `Stop` lists go through `inline_safe` before they leave. The web side is in [planner-web-shell](planner-web-shell.md#background-tasks-and-the-command-list).

### Limits

Open items from review: [the follow-ups](../concerns/planning-session-sandbox-limits.md#planner-background-table-open-items). The contracts that pin this table: `bridge/background_contract_tests.rs`, `plan_server/chat/background/contract_tests.rs`, `tokens/planner_contract_tests.rs` and the web `background.contracts.test.tsx`.
