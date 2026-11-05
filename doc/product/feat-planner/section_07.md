# 07 — Session Bridge

Part of the [Loom Plan product vision](index.md).

This section defines how the `loom plan` server starts and owns Claude Code sessions in tmux,
reads their transcripts into chat messages, delivers the user's input, learns the states the
transcript does not carry, answers permission requests, and ends, recovers, and reattaches
sessions. It ends with the model the server hands to the chat interface (section 08). The
vision's Decisions ("Bridge", "Sessions start cold") fix the frame.

## Session kinds

| Kind          | What it is                                                       | How many                     | Who drives it                         |
| ------------- | ---------------------------------------------------------------- | ---------------------------- | ------------------------------------- |
| Authoring     | The chat session that writes documents under `doc/`              | At most one per project      | The browser holding authoring         |
| Summary       | A background session that rebuilds summaries (section 04)        | One at a time; later work batched | The server; no browser                |
| Refinement    | Subagents the authoring session spawns to rebuild summaries      | Inside the authoring session | The authoring session                 |
| Pressure      | A Claude step of a pressure run (section 11)                     | One run per project, one step at a time | The server's pressure driver; the holder answers its prompts |

Every authoring session also carries a scope: Open, Kickoff or Team. Kickoff and draft-all
keep the Authoring kind. The scope lives in `scope.json` in the session's runtime directory;
the sandbox can neither read nor write it.

A refinement subagent is no separate session. The authoring session spawns them in two waves
(section 04: one `loom-plan:summary` agent per document summary, then one `loom-plan:overview`
agent) through Claude Code's own subagent tool, so the bridge sees them as tool activity and
through the `SubagentStart`/`SubagentStop` hook events. Each one's transcript is the authoring session's
`<session-uuid>/subagents/agent-<id>.jsonl` (`loom/src/provider_data/resolve.rs:7`), which the
bridge reads the same way as the main transcript.

Section 04 decides which summaries a summary session rebuilds and when; work that arrives while
one runs forms its next batch. One summary session runs at a time, so a rebuild after a
`git pull` does not compete with the authoring session for the user's subscription limits.

## Prerequisites

At start the server checks for `tmux` and `claude` on `PATH` and runs the existing login probe,
`claude auth status --json`, through `stage_auth_status` (`loom/src/claude/auth.rs:87`), which
probes under the same minimal agent environment the launch script sets (below). Any failure
puts the server in the read-only mode of section 02, and the app names the missing piece: tmux
not installed, Claude Code not installed, or Claude Code not logged in. (My call: a logged-out CLI would start a
session that sits on a login screen nobody can see.)

## Starting a session

Every session gets an id, a fresh UUID that is also its Claude Code session id. Its runtime
files live in the section 02 runtime directory:

```text
~/.loom/plan/<project-key>/sessions/<session-id>/
  session.json    registry record: kind, tmux socket, process identity, transcript path, state
  scope.json      authoring scope: Open, Kickoff or Team; inaccessible to the sandbox
  launch.sh       the launch script tmux runs (mode 0700)
  settings.json   hook registrations and permission rules (mode 0600)
  brief.md        the appended system prompt (section 09)
  plugin/         the per-session `loom-plan` plugin (section 09)
  events.jsonl    hook event spool
  answers/        permission decisions written by the server
  done/           pressure sessions only: the completion marker's directory (section 12)
  codex.log       pressure runs only: the Codex step's output (section 11)
```

For a pressure run, `session.json` also records the Codex process's `ProcessIdentity`, so a
later server can find and terminate it (section 11).

The launch reuses loom's tmux lane at the level below stages. The stage-facing
`TmuxBackend::spawn_session` takes a `Stage`, a `Session`, and `.loom/work`, none of which a
planning session has, so the planning server calls the lane's free functions directly:

- `spawn_in_tmux` (`loom/src/orchestrator/terminal/tmux/mod.rs:243`) starts the server and keeps
  its silent-failure guard (stderr with exit 0 is failure, then a `has-session` probe);
- `PRESENTATION_OPTIONS` (`tmux/mod.rs:94`) is applied as for stages, so the user's
  `~/.tmux.conf` cannot turn on mouse capture;
- `teardown_socket` (`tmux/mod.rs:309`) ends it;
- the socket directory rule is `loom_socket_dir` (`tmux/socket.rs:22`);
- liveness and every signal go through `ProcessIdentity` (`loom/src/process/identity.rs:11`),
  never `tmux has-session`, for the reason `TmuxBackend::is_session_alive` gives
  (`tmux/backend.rs:159`).

`PRESENTATION_OPTIONS` and `teardown_socket` are private to the tmux module today, and the
dashboard's `bind_first_available_port` (`commands/status/web/listener.rs:33`), which section 02's
port search reuses, is `pub(super)`; widening their visibility is part of this work.

One tmux server per session, as for stages, so a wedged server takes down one session.

`launch.sh` sets the environment and `exec`s Claude Code, so the pane's process is `claude`
itself. Before the `exec`, it starts the owner watch in the background (below, "Crashes and
kills"). The environment is loom's minimal agent environment
(`agent_session_environment_from`, `loom/src/process/environment.rs:143`) plus two variables,
`LOOM_PLAN_STATE` (the runtime directory) and `LOOM_PLAN_SESSION` (the session id). It never
sets `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, or `LOOM_WORK_DIR`, so loom's global hooks treat a
planning session as an ordinary user session. The working directory is the project root.

Before the launch the server records the project root as trusted in `$HOME/.claude.json`
(`projects[<root>].hasTrustDialogAccepted`, `fs/permissions/trust.rs::trust_project`), as loom
does for stage worktrees. Claude Code runs no hook, `SessionStart` included, while its
folder-trust dialog is open, and neither a `--settings` key nor a trusted parent directory
skips that dialog: without the entry a session in an untrusted root stays in `Starting`. A
failure to write the entry fails the start.

```text
claude --session-id <session-id> --name "<display name>"
       --settings <session dir>/settings.json --setting-sources user,project
       --model <model> --effort <effort> --permission-mode <mode>
       --plugin-dir <session dir>/plugin --strict-mcp-config
       --append-system-prompt-file <session dir>/brief.md
       ["<opening message>"]      pressure sessions only
```

The brief, the plugin and the opening message come from section 09, with section 11's
differences for pressure sessions; the model and effort from section 09's table (section 04 for
the summary levels, the `[pressure]` keys for pressure steps); `--strict-mcp-config` from
section 12; the permission mode and the write rules in
`settings.json` from section 12. The mode is `auto` for authoring and pressure sessions and
`default` (Manual) for summary sessions; the same value is the `defaultMode` in `settings.json`. The launch line carries no prompt, because a command line is
readable by every local user through `/proc/<pid>/cmdline` and an opening message can quote
documents. Once the session's first `SessionStart` arrives, the server delivers the opening
message as the session's first turn, through the same path as every chat message ("Delivering
input"): sanitised, held to the 100 KB cap, and pasted. The one exception is a pressure session's
opening, which holds only a slash command and a plan path and travels on the launch line, as
`loom pressure` passes it (`loom/src/commands/pressure/spawn.rs:38`).

After `spawn_in_tmux` returns, the server reads the pane's process id
(`display-message -p '#{pane_pid}'`), records its `ProcessIdentity` with start time in
`session.json`, and only then reports the session started. A spawn that fails after the tmux
server may exist tears it down before reporting the error (`mistakes/tmux-backend.md`).

### Kickoff

A kickoff starts from `/new` with an optional first message and attachments, and no target
file. The authoring session emits a fenced `loom-kickoff` block with the taking-shape state:
size, name, problem, decisions, open questions and sections, with entries marked `settled`,
`wording` or `open`. The server parses the block and sends a `kickoff` message to the browser.

Only the holder may use "Confirm name". It arms a one-time grant to create exactly one target
that did not exist at session start: `doc/product/<name>/index.md` for a feature, or
`doc/plans/PLAN-<slug>.md` plus its `doc/plans/briefs/<slug>/` bundle for a standalone plan.
Names follow the feature-name rule, with at most 64 characters. A second confirmation is
refused. The session keeps Kickoff scope for the rest of its life: file-tool writes stay within
the granted target and, for a standalone plan, its brief bundle. `loom knowledge` and
`loom plan summary` commands other than read-only `status` still ask the holder (section 12).

### Draft-all team

A draft-all team is one authoring session leading up to five in-process teammates, launched
with `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` added to the launch environment. It writes only
the missing documents its run was started for and their brief bundles. The bridge reads
teammate transcripts for tokens and each teammate's assigned documents, and sends them as
`draft_team` messages. No teammate message is rendered in the chat.

## Naming

| Name                 | Form                                         | Example                        |
| -------------------- | -------------------------------------------- | ------------------------------ |
| tmux socket          | `loom-plan-<first 12 hex digits of the id>`  | `loom-plan-3f9a0c41b7e2`       |
| tmux session         | `plan-<kind>`                                | `plan-authoring`               |
| Claude display name  | `loom plan: <step or task>`                  | `loom plan: section 07`        |

The `loom-plan-` prefix keeps socket paths far below the 104-byte `sun_path` limit and lets
`list_loom_sockets` (`tmux/socket.rs:54`) skip planning sockets the way it skips `loom-view-`,
so `loom init` and `loom clean` never report them as unattributed stage sockets. The terminal
line for a session start names the socket, so a person can watch a session with
`tmux -L <socket> attach -r`. (My call: read-only attach, since keystrokes typed into the pane
would bypass the bridge.)

## Hooks and the event spool

`settings.json` registers one command for every hook event the bridge reads: the running `loom`
binary's absolute path with a hidden subcommand, `loom plan hook <event>`. The command reads the
hook's JSON from stdin, checks that `hook_event_name` matches the event it was registered for
and exits silently otherwise (the lesson of
`doc/loom/knowledge/mistakes/spurious-waiting-for-input.md`), and appends one JSON line to
`events.jsonl` under an exclusive file lock. Each line carries the receive time, the event name,
and the hook payload's `session_id`, `transcript_path`, and the event-specific fields.

| Event               | Used for                                                                   |
| ------------------- | -------------------------------------------------------------------------- |
| `SessionStart`      | Session is up; the transcript path; `source` tells startup from compaction |
| `UserPromptSubmit`  | Confirms a delivered message; the session is working                       |
| `PermissionRequest` | A permission decision is needed (below)                                    |
| `PermissionDenied`  | Auto mode's classifier blocked a tool call; shown as a notice (below)      |
| `Notification`      | `idle_prompt` backs up the idle state; other types are logged only         |
| `Stop`              | The main agent's turn ended                                                |
| `StopFailure`       | The main agent's turn ended on an API error; handled as `Stop`             |
| `SubagentStart`     | A subagent started (count of running subagents)                            |
| `SubagentStop`      | A subagent ended; never changes the session's own state                    |
| `PreCompact`        | Compaction started; shown as a notice                                      |
| `SessionEnd`        | Claude Code is exiting                                                     |

`PreToolUse` is registered too, for section 12's path guard (`loom plan hook PreToolUse`,
"The planning hook"). It writes nothing to the spool; it answers allow or deny for each file-tool
call and fails closed.

The server tails each session's spool. Hook commands never fail the session: an error writing
the spool exits 0 with no output, except for `PermissionRequest`, which answers deny (below).
`PermissionDenied` prints nothing, so Claude Code never retries the call. A hidden `OwnerWatch`
argument of the same subcommand is no hook: Claude never calls it and it writes no spool line
("Crashes and kills").
Hook commands run in Claude Code's own process, outside the Bash tool's sandbox, so they write
the session directory while the session's own tools cannot (section 12, "What a session may
write").

The spool is runtime state. It is deleted with the session directory when the session ends.

## The transcript

The transcript is `~/.claude/projects/<slug>/<session-id>.jsonl`, where `<slug>` is the project
root with every `/` and `.` replaced by `-` (`project_slug`,
`loom/src/provider_data/resolve.rs:101`). `--session-id` fixes the file name in advance. The
server still takes the authoritative path from each `SessionStart` event's `transcript_path`,
and follows it if it ever changes, so a change in Claude Code's layout breaks nothing silently.

The server tails the file, reads complete lines only (a partial last line waits for its
newline), and keys every record by its `uuid`, so rereading never duplicates an item. Record
types it does not know are skipped without error; the transcript carries many bookkeeping
records (`attachment`, `file-history-snapshot`, `mode`, `cost-state`, `ai-title` and others).

| Record                                              | Becomes                                                              |
| --------------------------------------------------- | -------------------------------------------------------------------- |
| `user`, string content, not `isMeta`                | A user message                                                       |
| `user`, `isMeta` or command wrapper tags            | Nothing                                                              |
| `user`, text `[Request interrupted by user…]`       | An interrupt notice; the turn ends                                   |
| `user`, `tool_result` block                         | Completes the tool call with the same `tool_use_id`                  |
| `assistant`, `text` block                           | Assistant text, Markdown                                             |
| `assistant`, `thinking` block                       | A "thinking" marker with its duration                                |
| `assistant`, `tool_use` block                       | A tool call, pending until its result arrives                        |
| `system`, `turn_duration`                           | The turn's elapsed time                                              |
| `system`, compaction boundary or API error          | A notice                                                             |

Claude Code writes one `assistant` record per content block, each with the same `message.id`
and an `apiBlockIndex`; the bridge groups them into one assistant message. Thinking blocks in
current transcripts carry a signature and no text, so the chat shows that the session thought
and for how long, never what. Each assistant message's `usage` is read with the existing
parser (`loom/src/provider_data/claude.rs:13`) so a turn can show its token count.

Tool results for file edits carry `filePath`, `originalFile`, and `structuredPatch`. The bridge
uses `filePath` to build each turn's list of changed files, and `originalFile` and
`structuredPatch` only for each tool row's `+a −b` counts. A file's text before the turn, and
its `before_hash`, come from the pin section 06 takes when the turn starts ("Turn diffs"), which
section 08 shows as the turn's diff. Files a turn changed through `Bash` appear through the file
watcher (section 04) as "changed during this turn" without a tool attribution.

**Streaming granularity.** Text reaches the chat one content block at a time, as Claude Code
writes each completed block to the transcript. Word-by-word streaming is not a goal; it would
need stream-JSON or pane scraping, both ruled out. While a block is being written the chat shows the session as
working.

## Session state

A **turn** runs from the `UserPromptSubmit` of a delivered message (or of the opening message) to
the next main-agent `Stop` or `StopFailure`.

| State         | Entered on                                                                                    |
| ------------- | --------------------------------------------------------------------------------------------- |
| Starting      | Spawn                                                                                         |
| Idle          | `SessionStart` (startup), main-agent `Stop` or `StopFailure`, interrupt record, `idle_prompt` |
| Working       | `UserPromptSubmit`; a transcript record newer than the last idle event                        |
| Permission    | `PermissionRequest`, until the server answers it                                              |
| Ending        | The server asked for the final write                                                          |
| Ended         | `SessionEnd`, or the process identity check finds the process gone                            |
| Failed        | The spawn failed, or the process died outside Ending                                          |

Two rules keep the state honest. A hook event never moves the state alone if the transcript
contradicts it: an assistant or tool record written after the last `Stop` means the session is
working, whatever the spool says. And only the main agent's events move the state;
`SubagentStop` and subagent transcript records never do. The process identity check runs every
two seconds. (My call, both from `mistakes/spurious-waiting-for-input.md`.)

## Delivering input

A chat message goes into the pane in three tmux commands on the session's socket:
`load-buffer -b loom-input -` with the text on stdin, `paste-buffer -p -d -b loom-input -t
plan-<kind>` (bracketed paste, so newlines do not submit), then `send-keys Enter`. Never
`load-buffer -w`, which reaches the clipboard and crashes tmux 3.6a with a client attached
(`mistakes/tmux-backend.md`).

The server delivers only while the session is Idle. A message sent while the session works is
queued in the server, shown in the chat as queued, and delivered at the next Idle; queued
messages go in one at a time. An interrupt (below) is the exception: it discards the queue. Typing into the pane mid-turn would race Claude Code's own
input handling, which the bridge cannot observe without reading the pane.

A delivery counts once a `UserPromptSubmit` event arrives within 10 seconds. Without one the
chat marks the message undelivered and offers to resend it. The delivered text, the user's
words with the attachment block section 10 appends, is capped at 100 KB, measured after the
server builds it. The WebSocket command that carries a message is bounded separately by section
12's 256 KiB inbound limit, which leaves room for the attachments' structured fields and JSON
escaping. (My call.)

A queued message can be withdrawn by the holder until its delivery starts; it leaves the queue
and the chat.

Chat messages that start with `/` are refused with a note, except `/compact`. (My call: `/clear`
and `/exit` would change the session id or end it outside the bridge, and other commands open
terminal dialogs the browser cannot see.)

Planning sessions run without the question tool: `settings.json` denies `AskUserQuestion`, and
section 09's guidance has sessions ask in plain text, which the chat shows as ordinary assistant
text. (My call: that tool renders a picker only in the pane.)

## Permission prompts

Permission decisions go through the `PermissionRequest` hook. The bridge never answers a
dialog by keystrokes.

1. Claude Code runs `loom plan hook PermissionRequest` before it would show its own dialog.
   The command writes the request (tool name, tool input, a request id) to the spool and waits,
   polling `answers/<request-id>.json` every 200 ms.
2. The server sets the session to Permission and the chat shows the request to every browser.
   Only the browser holding authoring can answer: **Allow** or **Deny**, with an optional note
   that reaches the session as the deny message.
3. The server writes the answer file; the hook prints the decision as Claude Code expects and
   exits.

```json
{"hookSpecificOutput": {"hookEventName": "PermissionRequest",
  "decision": {"behavior": "deny", "message": "Not in doc/; ask before touching code."}}}
```

There is no "always allow". Standing rules belong to section 12, which sets them per session
kind.

Authoring and pressure sessions run in auto mode, so a request reaches this flow only for a call
that matches an `ask` rule in the session's settings (`WebFetch`, and the `loom knowledge` and
`loom plan summary` commands that Kickoff and Team do not pre-allow; section 12). Everything else
that no allow rule covers is decided by Claude Code's classifier. When the classifier blocks a
call, Claude Code runs `loom plan hook PermissionDenied`, the spool carries the payload's
`tool_name` and `reason`, and the chat shows a notice item, `Auto mode blocked Bash:
[Irreversible Local Destruction]`. Without a reason the text is `Auto mode blocked Bash`, and
without a tool name `Auto mode blocked an action`. The notice is text only, with no Allow or
Deny: the session has already been told, and the holder can answer by messaging it.

The hook is registered with a one-hour timeout. The answer is deny, with a message saying why,
when the request is unanswered after one hour, the session ends, the server is stopping, or the
hook cannot reach its directory, so a session never waits on a dialog in a pane nobody watches.
The chat shows that reason on the permission item.
Summary sessions have no one to ask and no classifier: they run in `default` mode, and a request
is denied at once and logged, since a summary session needing a permission means its rules
(section 12) are wrong.

## Interrupt and end

**Interrupt.** The authoring browser's stop button sends `send-keys Escape` to the pane, which
interrupts Claude Code's turn. The transcript records the interruption and the session goes
Idle. A pending permission request is denied first. The interrupt discards the queue and returns
the queued texts to the holder's composer (section 08); nothing is delivered at the Idle the
interrupt causes. Stop does nothing while the session is idle, and it sends at most one Escape
per turn: a second Escape at an idle prompt opens Claude Code's rewind picker.

**End.** A session ends with a final write when:

- the user starts a new chat or opens another offer from the authoring browser;
- the authoring browser has been gone for the grace period;
- `loom plan` gets its first Ctrl-C or SIGTERM.

The server waits for any running turn to reach Idle, returns any queued messages to the composer
of the tab that asked, accepts no new message, and, when the end comes from the grace period or a
server stop, denies every pending and new permission request so the final write never waits on an
answer nobody can give, delivers the closing message (wording from
section 09: record the remaining open questions and anything pending into the documents), waits
for that turn's `Stop` with no subagent running, then tears the session down. The final write
is bounded at 10 minutes, after which the session is torn down as it stands. (My call: decisions
are recorded as they are made, so the bound loses at most the open-questions pass.)

A session whose user sent no message is torn down at once, without the closing message: there is
nothing to write, so no model turn is spent on it.

**Grace period: 10 minutes** from the moment the holder's last WebSocket closes. (My call: long
enough for a reload, a network switch, or a short laptop sleep with the server on another
machine; short enough that a person who walked away does not hold authoring for an afternoon.)
Read-only browsers see that the holder is away and when authoring frees up.

**Stopping `loom plan`.** On the first Ctrl-C the authoring session gets the final write,
the running summary session finishes its current task within the same bound, and queued summary
sessions are dropped. On the second Ctrl-C the server tears down every tmux server it started
and exits. (My call: tearing down is fast, and leaving sessions running after the server has
gone would leave processes nobody owns.)

Summary sessions end the same way without a closing message: the server tears one down at its
first main-agent `Stop` with no subagent running.

## Reattaching and resume

Sessions start cold: a new `loom plan` never resumes an earlier conversation, and the bridge
never uses `claude --resume`. "Resume" in this product means a browser reattaching to a session
that is still running in the same server:

- A reload or reconnect from the holding browser (identified by its browser id, section 12)
  within the grace period gets the full chat model back, then live updates, and keeps
  authoring.
- A read-only browser that connects at any time gets the same model, read-only.
- After the authoring session ends, its conversation stays visible read-only until the next
  chat starts in this server. A server restart shows none of it.

The model is rebuilt from the transcript, the source of truth; the server keeps it in memory
only as a cache.

## Crashes and kills

- **The session process dies** outside Ending: the session goes Failed, the chat says so with
  the last transcript activity, authoring is released only if the browser that started the chat
  still holds it, and never while a pressure run holds it, and the documents keep what the session
  recorded as it went. Nothing restarts automatically; the user starts a new chat, cold. A
  failed summary session is reported to section 04's engine, which decides on a retry.
- **The `loom plan` server dies** (a crash or `kill -9`): its sessions keep running in their own
  tmux servers. The next `loom plan` on the project reads `sessions/`, and for each record whose
  process identity verifies as alive it delivers the closing message, waits within the same
  10-minute bound, and tears the session down, before accepting any new chat. A record whose
  identity cannot be verified is left alone and its socket name printed; a destructive sweep
  reads missing evidence as "do not destroy" (`mistakes/tmux-backend.md`). (My call.) A
  pressure session gets no closing message: it is torn down at once, and the run is handled as
  section 11, "Stopping, failure, and server stops", describes.
- **The server never returns.** Every session, summary sessions included, starts with an **owner
  watch**: its launch script runs `loom plan hook OwnerWatch <session dir>` in the background,
  outside Claude Code's sandbox, and the watch moves to a session of its own, so the signals that
  end the pane do not end it. Every 15 seconds the watch reads the project's server lock record
  and checks that the process it names is alive: the pid exists and its start time matches the
  record's, so a reused pid reads as no owner. The read takes no lock, so the watch can never
  make a starting server refuse. When no live owner has been seen for 15 minutes, the watch
  tears the session's tmux server down; it does the same when the session's own process is gone
  and no owner is alive, which clears the tmux server that `remain-on-exit` leaves behind. Each
  sighting of a live owner restarts the 15 minutes, so a server restarted within them keeps the
  session and recovers it as above. The watch ends when the session does. It delivers no closing
  message: with no server nobody reads it, and sessions record decisions as they are made.

## What the chat receives

The server exposes each session to the browser as one model, sent whole on connect and as
deltas after. Section 08 owns how it is rendered.

```json
{
  "session": {"id": "3f9a…", "kind": "authoring", "state": "working", "holder": "Dana",
              "away_until": null, "queued": 1},
  "turns": [{
    "id": "t4", "duration_ms": 41200,
    "tokens": {"input": 1840, "cache_write": 5200, "cache_read": 48000, "output": 920},
    "items": [
      {"id": "u-…", "type": "user", "text": "…", "attachments": [{
        "kind": "source", "path": "doc/plans/PLAN-x.md", "anchor": "lifetime", "lines": [40, 44],
        "stage": null, "summary_sentence": null, "rests_on": [], "quote": "…",
        "changed_since_selected": false, "locked": null}]},
      {"id": "a-…", "type": "thinking", "duration_ms": 3100},
      {"id": "a-…", "type": "text", "markdown": "…"},
      {"id": "a-…", "type": "tool", "name": "Edit", "summary": "…", "status": "done", "subagent": null},
      {"id": "p-…", "type": "permission", "tool": "Bash", "input": "…", "status": "denied",
       "deny_reason": "holder", "note": "Not in doc/; ask before touching code."},
      {"id": "n-…", "type": "notice", "text": "Interrupted"}
    ],
    "changed_files": [{"path": "…", "before_hash": "…", "by_tool": true}],
    "summaries_rebuilding": ["…"]
  }]
}
```

A delta adds an item, replaces an item by id (a tool call completing, a permission answered),
or changes the session record. `tokens` holds the turn's four counts from the transcripts'
`usage` (fresh input, cache writes, cache reads, output). A permission item's `deny_reason` is
`holder`, `timeout`, `session_ended`, `server_stopping`, `hook_error`, `interrupt`, or
`summary_session`; `note` is the holder's deny note, or null. Subagent activity appears as
tool items with `subagent` set to the subagent's id, nested under the tool call that spawned it. `summaries_rebuilding` comes from
section 04's engine. Attachments are formatted into the delivered text by section 10. The bridge
parses each user record's `<loom-attachments>` block back into the structured fields above
(section 10's header fields: kind, address and anchor, line range, `#stage:<id>`, summary
sentence id, `rests-on` source anchors, quoted text, `changed-since-selected`, `locked`), so a
reload rebuilds the chips from the transcript alone.

The server also sends these messages:

| Message      | What it carries                                                                  |
| ------------ | -------------------------------------------------------------------------------- |
| `kickoff`    | Taking-shape state parsed from the session's fenced `loom-kickoff` block           |
| `draft_team` | Teammate tokens and assigned documents read from transcripts; no teammate chat    |

Only the holding browser's connection may send the commands the bridge accepts: start a chat,
send a message, withdraw a queued message, answer a permission request, interrupt, end the
chat, confirm a kickoff name. The server rejects them from any other connection, except that while nobody holds
authoring any browser may start a chat or send a message, and takes authoring by doing so
(section 02).

## Out of scope for this section

- Which summaries a summary session rebuilds, when, on which model, and the rebuild budget:
  section 04.
- Rendering of messages, tool activity, states, diffs, and the presence list: section 08.
- The briefs, opening messages, closing message wording, and per-step skills: section 09.
- How a selected passage becomes part of a message: section 10.
- Permission mode, sandboxing, writable paths, and standing permission rules: section 12.
- Server lifetime, the lock, and read-only mode: section 02.
- Browser naming: section 02; browser id: section 12.
