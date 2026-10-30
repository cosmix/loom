# 08 — Chat Interface

Part of the [Loom Plan product vision](index.md).

This section defines the chat panel: where it sits, how it renders the session model section 07
hands it (user messages, assistant text, thinking markers, tool activity, notices), how each
session state looks, permission prompts, queued messages and interrupts, attachments in the
composer and in sent messages, what a read-only browser sees, the per-turn list of changed files
and rebuilt summaries with their cost, how a session started from an offer looks, ending a chat,
how a pressure run shows, the Share control, and the error states. Section 07 owns what the server knows; this section owns what the browser
shows and which commands it sends. What a passage attached to a message means to the session is
section 10's.

## The model in one paragraph

The chat panel docks on the right of every page except `/new` (vision, "The screen"). It shows one
conversation, the project's authoring session (section 07), as a list of turns; each turn is the
user's message, then the session's reply rendered one transcript block at a time, then a footer
with what the turn changed and cost. A state line at the top of the panel always says what the
session is doing and what it needs. Only the browser holding authoring has a composer; every
other browser sees the same conversation live with no input box. Nothing in the panel is stored
on the server beyond section 07's model: drafts, panel width, and scroll position live in the
browser.

## Layout

```text
┌ Chat ─────────────────────────────────────── ⋯ ┐
│ ◐ Working · 41s · Edit section_02.md           │  state line
│ Dana authoring · 2 watching · 18.4k tokens     │  session line
├────────────────────────────────────────────────┤
│  turns, oldest first                           │  transcript
│  …                                             │
├────────────────────────────────────────────────┤
│ Queued (1)                                     │  queue
│ [section_02.md › Port · lines 54–60] [×]       │  attachment chips
│ ┌────────────────────────────────────────────┐ │
│ │ Message…                                   │ │  composer
│ └────────────────────────────────────────────┘ │
│                                  Stop   Send   │
└────────────────────────────────────────────────┘
```

- **Width.** 26rem by default, resizable by dragging its left edge between 20rem and 40rem. The
  width is kept in the browser's local storage. (My call.)
- **Collapsing.** A control in the panel header collapses it to a 2.5rem rail that keeps the
  state glyph and a badge when a permission decision is waiting. The reading column recentres in
  the freed width. Collapsed or not is kept per browser. (My call.)
- **Narrow screens.** Below the `lg` breakpoint the panel leaves the layout and becomes a sheet
  over the reading column, opened from a header button that carries the state glyph and the
  permission badge. Selecting "Add to chat" in the reading column (sections 05, 06) adds the
  chip without opening the sheet; the button shows the attachment count. (My call: on a phone
  the reader collects passages first and writes once.)
- **Surface.** The panel sits on `--card` with a `--hairline` left border, beside the reading
  column on `--background`. No coloured spine marks anything in it
  (`conventions/web-dashboard-typography.md`, "Stage thread chips").
- **Header.** The title is "Chat", with Collapse in the dock and Close in the sheet. A live
  chat adds **New chat**; an ended or failed chat offers it in the composer area. The menu
  appears when the browser can end the chat. Below the state line, a chat's session line names
  the authoring browser, the watcher count and the running token total; another browser sees
  "Read-only · Dana is authoring". An away holder shows when authoring frees. With no chat,
  there is no session line; the checklist's offer belongs below "No chat".

On `/new`, an active kickoff conversation puts the transcript and composer in the main column,
with **Taking shape** in place of the chat dock. The transcript uses the selected reading face;
Taking shape uses Inter and lists the size, name, problem, settled decisions, open questions and,
for a feature, proposed sections. On narrow screens the panel follows the transcript.

## Message rendering

Each item in section 07's model renders as follows. Type is Inter at 14px with a 1.5 line height
for message text, between the dashboard's 12-13px chrome and concept mode's 17, 19 (default) or 21px reading text;
state words, tool rows, and footers use the 12-13px chrome size. Monospace only for paths,
commands, and identifiers (`conventions/web-dashboard-typography.md`). (My call.)

| Item         | Rendering                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------- |
| `user`       | Right-aligned block on `--muted`, rounded, at most 85% of the panel width; attachment chips under it |
| `text`       | Full-width Markdown, no bubble                                                                      |
| `thinking`   | One muted line, "Thought for 3s"; consecutive markers in one message merge into one line            |
| `tool`       | A tool row (below)                                                                                  |
| `permission` | A permission card (below)                                                                           |
| `notice`     | A centred muted line with a small glyph; API errors in `--tone-blocked`                             |

**Markdown.** Assistant text uses the renderer file mode uses (section 06), with its rules: raw
HTML never rendered, HTML comments dropped, tables in the ledger style and scrolling inside their
own box, code blocks with a language label and a copy control. Links follow section 06 too: a
`doc/` path or relative link opens file mode in the reading column, an inline code span that is
exactly a document path becomes a link, a path outside `doc/` stays text, and `http(s)` links
open in a new tab. A chat reply never navigates away from the app.

**Block by block.** Text appears one content block at a time, as Claude Code writes each block to
the transcript (section 07, "Streaming granularity"). A block appears whole, without a typing
animation. While the session works and the newest block has not arrived, the state line says so;
the transcript shows no placeholder bubble. (My call: a fake typing effect would claim a
granularity the bridge does not have.)

**Hover controls.** Each user message and each assistant text block shows on hover a copy
control (copies the Markdown source) and its time. Items are not editable after sending.

**Opening messages and server messages.** The first user message of a session started from an
offer is section 09's opening message. It renders as a user message labelled with the offer's
name ("Write section 08 — Chat interface"), collapsed to its first line with a "Show" control,
since its audience is the session. The closing message the server delivers
before a session ends (section 07, "Interrupt and end") renders the same way, labelled "Closing
message from loom plan". (My call.)

**Long conversations.** The 20 most recent turns render in full. Older turns render as one line
each ("Turn 3 · 2 files changed · 4.1k tokens") that expands in place. (My call: a long
refinement session would otherwise put thousands of rows in the page.)

**Scrolling.** The transcript follows new items while it is scrolled to the bottom. When the
reader has scrolled up, new items do not move it; a "New activity" pill at the bottom edge
scrolls down when clicked.

## Tool activity

A tool call is one row: a glyph, the tool name, a one-line summary, and its status. The summary
is section 07's `summary` field, which the bridge forms from the tool input:

| Tool                     | Summary                                                    |
| ------------------------ | ---------------------------------------------------------- |
| `Read`                   | the path, with the line range when one was given           |
| `Edit`, `Write`          | the path, and `+a −b` lines once the result arrives        |
| `Bash`                   | the command's first line                                   |
| `Grep`, `Glob`           | the pattern and the path searched                          |
| `Agent`                  | the subagent's description                                 |
| `WebFetch`, `WebSearch`  | the URL or the query                                       |
| anything else            | the tool name and its first input field                    |

| Status       | Shows                                                                           |
| ------------ | ------------------------------------------------------------------------------- |
| pending      | a small `BusyRoundel` (`web/src/aurora-ui/feedback/BusyRoundel.tsx:57`)         |
| done         | a muted check                                                                   |
| error        | `--tone-blocked` cross and the error's first line under the row, without expanding |
| denied       | "Denied", with the deny note if one was given                                   |
| not finished | muted "Not finished" for a call the turn's interrupt cut off                    |

**Expanding.** Clicking a row expands it to the tool's input and result as monospace text, the
result capped at 40 lines with a "Show all" control. An `Edit` or `Write` row expands to the
change as a unified diff built from the result's `structuredPatch` (section 07, "The
transcript"), added lines in `--tone-completed`, removed lines in `--tone-blocked`, as plain
text without loading CodeMirror. The full diff with context is the turn's diff in file mode
(below). (My call.)

**Grouping.** While a turn runs, every tool row shows as it arrives. When the turn ends, three or
more consecutive tool rows with no text between them fold into one row, "Used 6 tools · edited 2
files", that expands to the rows. A row in error or denied stays outside the fold. (My call: the
reader of a finished turn wants the text and the changed files; the tool trail is there on
demand.)

**Subagents.** An `Agent` row carries the count of the subagent's own tool calls and a
`BusyRoundel` while it runs. Its nested rows (section 07: items with `subagent` set) render under
it, indented one step, collapsed by default. Expanding the row also shows the subagent's final
result. The summary rebuild a refinement turn runs (section 04) is one such row, labelled by its
description.

## Session states

The state line at the top of the panel names the state in words, with a glyph; colour is never
the only signal. The states are section 07's.

| State      | State line                                                                            | Composer (authoring browser)                 |
| ---------- | ------------------------------------------------------------------------------------- | -------------------------------------------- |
| none       | "No chat"; the primary offer and "New chat" occupy separate rows below it             | Enabled; sending starts a chat               |
| Starting   | `BusyRoundel` turning, "Starting session", the session's name and model               | Enabled; messages queue                      |
| Idle       | Roundel at rest, "Waiting for you"                                                    | Enabled; Send delivers                       |
| Working    | Roundel turning, "Working · 41s", the newest tool row's summary, "2 subagents" if any | Enabled; Send queues; Stop shown             |
| Permission | `--tone-warning` glyph, "Needs a decision: Bash"; the card is pinned (below)          | Enabled; Send queues; Stop shown             |
| Ending     | Roundel turning, "Writing final notes into the documents"; the 10-minute bound        | Disabled, with the reason                    |
| Ended      | Muted stamp, "Chat ended · 3m ago"                                                    | Replaced by "New chat" and the primary offer |
| Failed     | `HazardPanel` (`web/src/aurora-ui/feedback/HazardPanel.tsx:116`) in its error tone    | Replaced by "New chat"                       |

**Empty state.** "No chat" stands beside its resting glyph. The primary offer from section 01,
when available, and **New chat** each get a full row beneath it, so an offer's title is distinct
from the state label. The composer stays available to the authoring browser; sending its text
and attachments starts a chat.

**Header glyphs.** The collapsed rail and the sheet-opening button use `ChatGlyph`: a turning
`LoaderCircle` for Starting, a writing `PenLine` for Working and Ending, and a ringing `BellRing`
in `--tone-warning` while permission is needed. Pending permissions take priority over the
session's state. Idle and no chat use a resting roundel; Failed shows `!`, Ended `✓`. The session
line uses the drafting glyph for Starting, Working and Ending, and a breathing status LED for
"Waiting for you". These activity glyphs freeze under `prefers-reduced-motion`.

The roundel and the stamp reuse the dashboard's patterns: `WorkRoundel` and `ActivityRoundel`
(`web/src/components/activity-roundel.tsx:11`, `:37`) for the turning disc, the terminal well's
`Stamp` (`web/src/components/terminal/terminal-notice.tsx:57`) for an ended state with its age.
Elapsed times use `formatElapsed` (`web/src/lib/format.ts:42`).

- **Working** counts from the turn's first `UserPromptSubmit`. The summary of the newest tool row
  tells a viewer what is happening without scrolling the transcript.
- **Failed** says which kind of failure: "The session could not start" with the spawn error, or
  "The session stopped unexpectedly" with the time of its last transcript activity. Both add:
  "The documents keep what the session recorded as it went. A new chat starts fresh." (section
  07, "Crashes and kills").
- **Ended** keeps the conversation readable until the next chat starts in this server (section
  07, "Reattaching and resume"). The ended conversation stays in place above the new one's
  starting state, under a divider with the end time, and leaves the panel when the new session
  writes its first item.

The browser tab's title carries the state when it needs the user: `(!)` while a permission
decision waits, `(✓)` when a turn ended while the tab was hidden; the prefix clears when the tab
is shown. The holding browser can also get a desktop notification for both, through the
dashboard's opt-in notifications (`web/src/state/notifications.ts`), off by default. (My call.)

## Permission prompts

A permission request (section 07, "Permission prompts") renders as a card in the transcript at
the point it arose, and the same card is pinned above the composer while it is pending, so the
holder sees it wherever the transcript is scrolled.

The card shows the tool name, its input, and the time left before the automatic deny:

- `Bash`: the command, in monospace, in full;
- `Edit`, `Write`: the path and the change as a unified diff, as in an expanded tool row;
- any other tool: its input as indented JSON.

The holding browser sees two buttons, **Allow** and **Deny**. Deny opens a one-line note field
with "Deny" and "Deny with note" actions; the note reaches the session as the deny message
(section 07). There is no "always allow" (section 07). Keyboard: with the pinned card focused,
`y` allows and `n` opens the deny note; the keys do nothing while the composer has focus. (My
call.)

Read-only browsers see the same card without buttons: "Waiting for Dana to decide".

After the answer the card collapses to one line:

| Outcome                                         | Line                                         |
| ----------------------------------------------- | -------------------------------------------- |
| `allowed`                                       | "Allowed: Bash"                              |
| `denied`, `holder`                              | "Denied: Bash", and the note                 |
| `denied`, `timeout`                             | "Denied automatically after one hour: Bash"  |
| `denied`, `interrupt`                           | "Denied when the turn was stopped: Bash"     |
| `denied`, `session_ended` or `server_stopping`  | "Denied: the session was ending"             |
| `denied`, `hook_error`                          | "Denied: the permission hook failed: Bash"   |

The line comes from section 07's `permission` item: its `status` (`pending`, `allowed`,
`denied`), its `deny_reason`, and its `note`, the holder's deny note. The `summary_session`
reason never shows here: summary sessions have no chat.

## Composing and sending

The composer is a plain textarea that grows to eight lines, then scrolls. `Enter` sends;
`Shift+Enter` inserts a newline. `c` focuses the composer from anywhere outside an input, and
joins the `?` help dialog (`web/src/routes/shell.tsx:26-31`) beside section 05's keys. (My call.)

**Drafts.** The unsent text and its attachments are kept in the browser's local storage, so a
reload, a dropped connection, or a crashed tab keeps them. (My call, matching section 06's editor
drafts.)

**What sending does.** Send issues section 07's "send a message" command with the text and the
attachments' structured form. The message appears in the transcript as soon as the server
accepts it, marked by its delivery state:

| Delivery state | Shows                                                          |
| -------------- | -------------------------------------------------------------- |
| queued         | In the queue strip above the composer, not yet in the transcript |
| sent           | In the transcript with a muted "Sending" until `UserPromptSubmit` |
| delivered      | In the transcript, no marker                                   |
| undelivered    | "Not delivered" in `--tone-warning` with a "Resend" action     |

**Limits.** The composer counts the message with its formatted attachments (section 10) against
section 07's 100 KB cap and disables Send above it, saying by how much. A message starting with
`/` other than `/compact` disables Send with the note "Slash commands are not available here;
`/compact` is." The server refuses both as well (section 07).

### Queued messages

A message sent while the session is Starting, Working, or waiting on a permission decision is
queued in the server and shown in a strip above the composer: "Queued (2)", each with its first
line. The server delivers them one at a time at the next Idle (section 07). Before delivery the
holder can remove a queued message or take it back into the composer to edit, which removes it
from the queue through section 07's "withdraw a queued message" command. Read-only browsers see
the count only.

### Interrupt

While the session works or waits on a permission, a **Stop** button sits beside Send. It sends
section 07's interrupt. The transcript then shows "Stopped by Dana" as a notice, and any
permission card collapses as denied by the interrupt. Stop has no keyboard shortcut. (My call:
`Escape` already folds concept mode runs, and an accidental interrupt costs a turn.)

Interrupting moves every queued message back into the composer, joined with blank lines, instead
of delivering them at the Idle the interrupt causes. (My call: a user who stops a turn usually
means to redirect it, and a queued message written before the stop may no longer say what they
want.)

## Attachments

Selecting text in the reading column and choosing "Add to chat" (sections 05, 06), or pressing
`a` on a focused sentence (section 05), adds an attachment chip above the composer. **Discuss**
in a document header attaches the whole document, opens the panel or sheet and focuses the
composer. Its quote previews the first 2,000 characters of the document. Sending starts a chat
if none runs. The chip labels the document and any passage, line range or stage:

| Origin                      | Chip label                                     |
| --------------------------- | ---------------------------------------------- |
| Document header, Discuss    | `section_02.md`                                |
| Source text, rendered view  | `section_02.md › Port`                         |
| Source text, editor or diff | `section_02.md › Port · lines 54–60`           |
| A plan's Stages panel       | `PLAN-foo.md › stage integration-verify`       |
| A summary sentence          | `Summary · section_02` (or `Product overview`) |

- Hovering or focusing a chip shows the selected text in full. Clicking it opens the passage in
  the reading column, in the mode it came from. `×` removes it.
- A chip whose document has changed since the selection was made shows "changed" in
  `--tone-warning`; it still sends, with that fact stated (section 10).
- A chip from a locked document (vision, "Locked plans"; the knowledge base) shows a lock glyph,
  so the holder knows the session cannot edit it.
- At most 10 attachments per message; the eleventh "Add to chat" says so. (My call.)

A sent user message shows the same chips under its text. A chip on a sent message also says
whether the turn changed that passage's section, with a link to that file's turn diff at the
heading (section 10).

The browser holds the attachments' structured form (path, anchor, selected text, line range,
stage id, summary sentence id, source anchors) as sections 05 and 06 define it. How they are
written into the delivered text, and read back from the transcript, is section 10's.

## What a turn changed

Each turn ends with a footer under its last item.

```text
41s · 2.8k tokens
Changed 3 files
  product/feat-planner/section_02.md +12 −4    Diff
  plans/PLAN-plan-server.md        +3 −1     Diff
  product/feat-planner/section_09.md new       Diff
Rebuilt summaries
  section_02                       3.1k tokens
  Product overview                 6.4k tokens
```

**Changed files.** One file row per entry in section 07's `changed_files`: the path relative to
`doc/` as monospace text, `+a −b` line counts or "new" / "deleted", and a **Diff** link that opens
file mode's "What a turn changed" diff for that file and turn (section 06, "Diffs"). While the turn runs the
list builds as edit results arrive, headed "Changing" instead of "Changed", and the Diff links
are disabled with the tooltip "Available when the turn ends", since section 06 builds the turn
diff at the turn's end. A file the turn changed through `Bash` instead of an edit tool
(`by_tool: false`) is listed the same way with the note "changed during this turn".

A file outside `doc/` that an edit tool changed is listed with a `--tone-warning` glyph and no
Diff link, since file mode serves only `doc/` (section 06); the note says "outside doc/".
Sessions can write outside `doc/` only for a pressure run's `planning-omissions.md` (section
12).

Under a changed product section, the row lists the plans whose `Implements:` line names it, with
their state; section 10 says what that list is for.

A turn that changed no file shows no list.

**Rebuilt summaries.** Section 07's `summaries_rebuilding`, one row per summary with a link to its
summary page (section 05). While the rebuild subagent runs the rows read "Rebuilding" with a
`BusyRoundel`. When the turn ends each row shows its tokens (section 04, "Cost"), or "failed" with
the last `seal` error on hover, or "left for the background rebuild" for a summary that was still
stale at the turn's end (section 04, "Who rebuilds what"). Overviews built in one batch show one
figure for the batch.

**Cost.** The footer's first line is the turn's duration (`system` `turn_duration`) and its tokens:
fresh tokens (input, cache writes, and output, the `fresh` sum the dashboard uses,
`web/src/lib/tokens.ts:11`) formatted with `compactTokens` (`web/src/lib/tokens.ts:35`), with a
tooltip giving the four parts: fresh input, cache writes, cache reads, output. The figure covers
the main agent and its subagents, the summary rebuild included; the rebuilt-summaries rows break
that part out. The session line shows the session's running total the same way, and, when the
quota poller (section 04) has a figure, the time left in the five-hour window from
`quotaLeftText` (`web/src/lib/tokens.ts:57`). (My call: cost is an anti-goal concern, so it shows
where the spending happens.)

## Starting a chat

A chat starts in one of three ways, all from the browser that will hold authoring:

1. **An offer** (section 01): the checklist's primary or secondary offer, an empty state's offer
   (section 05), or a missing entry in file mode's tree (section 06).
2. **New chat**: a refinement session with no step-specific brief (section 09 owns its brief).
3. **Sending a message** when no chat runs: a new refinement session whose opening message is
   the composer's text and attachments. (My call: a reviewer who selected a passage and typed a
   question should not need a separate start step.)

Starting opens the panel if it is collapsed and shows the Starting state with the session's name
(section 07's display name, `loom plan: section 08`), its model and effort (section 09), and a
**Brief** link that opens the brief the session was started with in a read-only dialog, so the
user can see what the session was told. (My call.) The opening message appears as the first user
message, labelled with the offer.

When a chat is already running and the holder accepts another offer, the app asks: "End the
current chat first? It will write its open questions into the documents before it stops." On
confirmation the current session goes through Ending, and the new one starts when it has ended.
There is one authoring session at a time (vision, Decisions). (My call.)

When two browsers start a chat at the same moment, the server grants the first; the second sees
"Dana started a chat first" and becomes read-only.

## Ending a chat

The panel header's menu has **End chat**, for the holding browser. It confirms first: "End this
chat? The session will write its open questions and anything pending into the documents, then
stop." The session then shows Ending until it reaches Ended (section 07).

Closing the tab or the browser needs no action. A note under the composer says so the first time
a browser holds authoring: "Closing this tab keeps the session for 10 minutes. After that it
writes its notes into the documents and ends." The app sets no `beforeunload` prompt. (My call:
the grace period and the closing write make leaving safe, and an unload prompt on every reload
would teach users to dismiss it.)

While the holding browser is away, every other browser sees in the session line: "Dana is away ·
authoring frees at 14:32", from section 07's `away_until`. When the holder returns within the
grace period the line returns to normal. When it runs out, the session goes through Ending, and
any browser can start the next chat.

## Pressure runs

A pressure run (section 11) occupies the panel the way an authoring session does. Starting one
takes authoring like starting a chat (section 02).

- **Run header.** Under the state line, in place of the session line, a header names the plan,
  the round, and the step: "Pressure test · `PLAN-foo.md` · round 2 of 3 · /address", with the
  run's elapsed time and its token total. The state line shows the running step's state as for
  any session.
- **Claude steps.** Each Claude step renders as a run of turns, as any session's do, under a
  divider naming the step and its model. Its footer lists the plan edits the step made, with
  Diff links (above, "What a turn changed"), and its tokens, teammates included (section 11).
- **Codex step.** The Codex step is one status item: the model actually running, the elapsed
  time with a `BusyRoundel` while it runs, and on finish either "Review written" with a link that
  opens the `codex-<plan file>` report in file mode, or "No review". On a failure the item shows
  the log's tail as monospace text. Codex tokens are not shown.
- **Composer.** The holder keeps the composer, and messages go to the foreground Claude step as
  in a chat. During the Codex step there is no session to deliver to, so Send is disabled with
  "Codex is reviewing; send when the next Claude step starts". (My call.)
- **Stop.** Stop is shown for the whole run and stops the whole run (section 11, "Stopping,
  failure, and server stops"), after confirming: "Stop the pressure test? The plan keeps the
  edits made so far, and no record is written." (My call: unlike a chat interrupt, stopping a run
  discards its progress.) There is no End chat while a run lasts.
- **The end.** The header's last line says how the run ended: "Pressure test recorded", "Stopped
  by Dana", or "Step 3 failed" with the reason section 11 gives. The run's turns stay readable
  like an ended chat until the next chat or run starts.

Read-only browsers watch the same panel without the composer and without Stop.

## Read-only browsers

A browser that does not hold authoring (section 02, "Several people, one author") sees the
panel exactly as the holder does, live, with these differences:

- no composer while another browser holds authoring: in its place a muted line, "Read-only ·
  Dana is authoring". When nobody holds authoring there is no read-only browser: every browser
  gets the composer, "New chat", and the primary offer, and the first to use one takes authoring
  (section 02);
- no Allow or Deny on permission cards, no Stop, no End chat (the Brief link still opens);
- queued messages as a count only;
- no "Add to chat" in the reading column (section 05); in its place "Send to author", whose
  suggestions reach the holder's panel as cards above the composer (section 10, "Read-only
  viewers").

**Presence.** The session line names the holder and counts the other browsers: "Dana authoring · 2
watching". Hovering the count lists them by their browser names, as the header's presence list
does (section 02). The chat panel shows no typing indicator. (My call: it would need a new
message lane for little gain.)

When `loom plan` runs in its read-only mode (section 02, "Missing prerequisites": `tmux` or
`claude` missing, Claude Code logged out, or the sandbox's prerequisites missing), every browser
is read-only. The panel shows a `HazardPanel` in its warning tone naming the missing piece and
how to install, log in, or fix it, and no offers.

## Share

Every authenticated browser has a **Share** control beside the header's presence list (section
02). It opens a popover that asks the server for the tokenized link (section 12, "Sharing
access") and shows it in monospace with a Copy button, under the line section 12 sets for the
bind:

- on a loopback bind, "Works on this machine. From another machine, tunnel first:" and the
  `ssh -L <port>:127.0.0.1:<port> <hostname>` command, with its own Copy button;
- on a remote bind, the link as bound and the plain-HTTP warning in `--tone-warning`.

The popover holds the link only while open. (My call: a link left on screen is a link in a
screenshot.)

## Connection and server errors

The panel talks to the server over the app's WebSocket. Its failures look like this:

| Condition                                 | Shows                                                                                                                                  |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Socket closed, server still reachable     | A banner over the transcript, "Connection lost · reconnecting", retrying after 1, 2, 5, then every 10 seconds; on reconnect the full model replaces the panel's (section 07) |
| Disconnected while holding authoring      | The banner adds: "If this browser stays disconnected for 10 minutes, the session writes its notes and ends."                            |
| Server restarted (the cookie is refused)  | The whole app shows the access screen of section 12 ("One token, every data route"), which leads with "loom plan restarted. Open the URL it printed in the terminal." when this page was connected before; the chat panel shows nothing of its own |
| Server stopping (first Ctrl-C)            | "loom plan is stopping. The session is writing its notes." Composer disabled                                                           |
| A command refused                         | An inline line under the composer with the server's reason, for example "Another browser holds authoring"                              |

During any of these the composer keeps its draft, and the transcript stays readable as last
received, dimmed, with "Last update 3m ago". (My call.)

## Accessibility

- The transcript is `role="log"` with `aria-live="polite"`; each new assistant text block is
  announced once. Tool rows are not announced; the state line is a polite live region that
  announces state changes, at most once every five seconds while Working.
- A new permission request is announced assertively once, naming the tool, and moves focus to
  the pinned card only when focus was already inside the chat panel.
- Every glyph has a text label; the state is always in words.
- The panel is one landmark (`aside`, labelled "Chat"); `Tab` from the reading column reaches it
  (section 05), and `c` focuses the composer.
- Animations follow `prefers-reduced-motion` as the dashboard's do: the roundel rests, and new
  items appear without motion.

## Out of scope for this section

- Session lifetime, transcript reading, hook events, delivery, the queue, and the model's fields:
  section 07.
- What the session does with an attached passage, the delivered attachment format, restoring
  what a turn changed, and what changed since a reviewer last looked: section 10.
- The briefs, opening and closing messages, and models per step: section 09.
- Diff views and the file-mode side of "Add to chat": section 06; concept mode's side: section
  05.
- Cost measurement, the summary panel, and quota meters: section 04.
- The header's presence list and browser naming: section 02.
- Which paths sessions may write and the access checks on chat commands: section 12.
- Running a pressure run, its steps, and how it stops: section 11; the share link's access
  rules: section 12.
