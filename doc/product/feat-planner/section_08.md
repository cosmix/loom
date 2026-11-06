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
with what the turn changed. The head's sub-line says in a few words what the session is doing
and what it needs; an idle chat has no words there. Before the first message the panel shows a
greeting and the composer, and no session exists. Only the browser holding authoring has a
composer; every other browser sees the same conversation live with no input box. Nothing in the
panel is stored on the server beyond section 07's model: drafts, panel width, and scroll
position live in the browser.

## Layout

```text
┌ Write section 02 ─────────────────────────── ✎ ┐
│ Working · section_02.md                        │  head: title, state words · place
├────────────────────────────────────────────────┤
│  turns, oldest first                           │  transcript
│  …                                             │  (no session: the greeting)
├────────────────────────────────────────────────┤
│ (What is still undecided?)                     │  context prompts, as pills
│ Queued (1)                                     │  queue
│ ┌────────────────────────────────────────────┐ │
│ │ ▤ About section_02                       × │ │  context row
│ │ ▤ "Port the server…"  section_02.md  ×     │ │  reference rows
│ │ Ask about section_02, or say what to change│ │  composer
│ │ ⌀                              Stop   Send │ │
│ └────────────────────────────────────────────┘ │
└────────────────────────────────────────────────┘
```

- **Width.** 21.25rem by default, resizable by dragging its left edge between 20rem and 40rem. The
  width is kept in the browser's local storage. (My call.)
- **Always shown.** At 1280px and wider the panel is a fixed column beside the reading column on
  every route that has a chat panel (not Home and not `/new`), so the reading column keeps about
  40rem beside the 18rem spine. The panel cannot be collapsed. (My call: a reader who wants to
  discuss a passage should never have to find the panel first.)
- **Narrow screens.** Below 1280px the panel leaves the layout and becomes a sheet over the
  reading column, opened from the header's Chat button, which carries the state glyph and the
  permission badge. The button shows on every route that has a chat panel; at 1280px and wider it
  toggles the side panel instead (pressed while the panel is open). Selecting "Add to chat" in the reading column (sections 05, 06) adds the
  chip without opening the sheet; the button shows the attachment count. (My call: on a phone
  the reader collects passages first and writes once.)
- **Surface.** The panel sits on `--card` with a `--hairline` left border, beside the reading
  column on `--background`. It is one continuous surface: hierarchy comes from type, space and a
  single hairline. A user turn is a tinted block aligned to the right, tool activity is a quiet
  run of lines that folds, assistant text has no box, and the composer is one round box with a
  round Send and a round Stop. A permission decision is the one item allowed real emphasis, and
  even that does not nest boxes. No coloured spine marks anything in it
  (`conventions/web-dashboard-typography.md`, "Stage thread chips"). Only the transcript
  scrolls: the panel head and the composer stay in place. The pane draws one live sign per
  working thing and none when nothing works, and never the roundel (the header notification
  pill's sign, section 02). The starting and wrap-up cards draw a ring spinner. A working turn
  ends with a typing bubble, the user's bubble mirrored to the left. In a pressure run each
  running step turns its own arrow. No sign is an avatar, so no message carries one.
- **Header.** The title sits over a sub-line. The title is the session's brief label ("Write
  section 08 — Chat interface"), "Chat" until it loads, and "New chat" while no session exists.
  The sub-line names the state in a few words, then the place the chat concerns (the feature or
  plan it belongs to, else the document being read): Starting, Working, "Needs your answer",
  Wrapping up, Ended or Stopped. An idle chat and an empty panel show no state words. The head
  has no menu, no run line and no relative time. **New chat** is the head's round, icon-only
  button, drawn only while a chat is live and this browser holds authoring; Close sits beside it
  in the sheet. Another browser sees "Read-only · Dana is authoring" in place of the composer
  (below), and the header's presence list shows an away holder and when authoring frees (section
  02). The context row (below) is the first attachment row of the composer, not part of the head.
- **No offers.** The panel draws no offer to start a step. Offers sit in the feature's spine and
  on the pages that own them (section 01), so no screen shows one offer twice. The panel offers
  New chat (while a chat is live) and the context prompts only.

On `/new`, an active kickoff conversation puts the transcript and composer in the main column,
with **Taking shape** in place of the chat dock. The transcript uses the selected reading face;
Taking shape uses Inter and lists the size, name, problem, settled decisions, open questions and,
for a feature, proposed sections. On narrow screens the panel follows the transcript.

`/new` always opens the empty form: one field, with the kind of work as a three-way switch on
its top edge (Feature, Standalone plan, Follow-up). Follow-up opens a picker of the features that
have a metaplan, done or not, each with its state and plan count; the picked feature's open
findings and open questions from its run reviews appear as suggestions under the field. Start
for a follow-up starts Plan follow-ups for that feature with the typed text and opens the
feature's metaplan page with the chat. While a kickoff is open the form also says "<name or
Untitled> is still open", with Continue and Set it aside. A kickoff persists as a draft under the
runtime directory (`kickoffs/<session id>.json`): the user's typed texts (at most 40 of 4 KiB),
the agent's last shape, the start's attachments, the user's rename and the confirmed target. The
draft is written at start and on every shape change, send, rename and confirmation, and it
survives the session and a server restart. Home lists drafts under "Unfinished", titled by name,
else "Untitled · <date>". A draft is live only while its session is the running chat; after a
restart every draft reads as ended.

- **Rename.** The Name row in Taking shape is editable until the work is confirmed. The user's
  name wins over the agent's in every later view, and the agent is told the new name in one user
  message. Before the agent has proposed a shape there is nothing to rename, and once the work is
  confirmed the name is fixed, because the write grant names it.
- **Set aside.** Ends the open kickoff and keeps its draft.
- **Continue.** `/new?session=<id>` opens a draft. A live draft shows its conversation. An ended
  draft shows its problem, decisions, questions and the user's messages, with "Continue this
  draft", which starts a new kickoff carrying the effective name, the size, the whole shape and
  the attachments that still resolve; one that does not is named in the opening as no longer
  available. The old draft is deleted only after the new session started and its draft is
  written; a refused or failed start leaves it as it was.
- **Discard.** Deletes an ended draft; a live one is set aside first.
- **Landed.** A draft whose confirmed document exists (a plan under any lifecycle prefix, or in
  `archive/`) is deleted, never listed. Ended drafts untouched for 30 days are removed when the
  server starts.

## Message rendering

Each item in section 07's model renders as follows. Chat chrome (state words, tool
rows, footers, the composer) is 14px, a user's message 15px in Inter, and the assistant's prose
16px in the reading face, all with a 1.5 line height, between concept mode's 17, 19 (default) or
21px reading text and the 13px floor that no transcript or panel text goes below, except a
chip's source and a file's size at 12px. Monospace only for paths,
commands, and identifiers (`conventions/web-dashboard-typography.md`). (My call.)

| Item         | Rendering                                                                                                                                                         |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `user`       | A tinted block aligned to the right, a quoted passage above the words inside it; attachment chips under it                                                         |
| `text`       | Full-width Markdown in the reading face (section 06's renderer), no box                                                                                           |
| `thinking`   | Nothing: the chat shows the model's words and its work, not its timing                                                                                            |
| `tool`       | A tool row (below)                                                                                                                                                |
| `permission` | A permission decision (below)                                                                                                                                     |
| `notice`     | A centred muted line with a small glyph; API errors in `--tone-blocked`; an auto-mode block (below) reads as "the session was stopped here", not as an error card |

**Markdown.** Assistant text uses the renderer file mode uses (section 06), with its rules: raw
HTML never rendered, HTML comments dropped, tables in the ledger style and scrolling inside their
own box, code blocks with a language label and a copy control. Links follow section 06 too: a
`doc/` path or relative link opens file mode in the reading column, an inline code span that is
exactly a document path becomes a link, a path outside `doc/` stays text, and `http(s)` links
open in a new tab. A chat reply never navigates away from the app.

**Block by block.** Text appears one content block at a time, as Claude Code writes each block to
the transcript (section 07, "Streaming granularity"). A block appears whole, without a typing
animation. While the session works, the head says "Working" and the working turn ends with a typing bubble:
the user's bubble mirrored to the left, holding three dots that rise in turn and rest under
reduced motion, its words "Working" for screen readers. It says the model is busy and nothing
more: the reply's text still arrives a block at a time, never typed out, because the bridge has
no finer granularity. (Operator's call.)

**Hover controls.** Each user message and each assistant text block shows on hover a copy
control (copies the Markdown source) and its time. Items are not editable after sending.

**Opening messages and server messages.** The first user message of a session started from an
offer is section 09's opening message. It renders as a user message labelled with the offer's
name ("Write section 08 — Chat interface"), drawn as one quiet centred line with that label and a
"Show" control, since its audience is the session; the prompt is not in the page until Show. The
closing message the server delivers before a session ends (section 07, "Interrupt and end") is
never shown. Its turn is drawn as the ending (below). (My call.)

**The ending.** The closing turn is not drawn as a turn. While its write runs it is a card with
a ring spinner, "Writing down what we decided", and a tick for each document the write
has reached. When it is done the card reads "Here's what this chat left behind": the documents
changed with their line counts, the decisions (Decided), and the open questions (Still open),
read from the `loom-closing` block the closing message asks for (section 09), with a link to
the changes when a document changed. A chat whose closing write never ran or was cut off shows
the same card from its turns, listing the documents it changed, and only when one changed. A
chat the user sent no message to has no closing write and no card.

**Long conversations.** The 20 most recent turns render in full. Older turns render as one line
each ("Turn 3 · 2 files changed") that expands in place. (My call: a long
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
| pending      | the word "Pending"; the working turn's typing bubble shows the work, not the row |
| done         | nothing beyond the row; an edit shows its `+a −b` counts                        |
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

**Subagents.** An `Agent` row carries the count of the subagent's own tool calls. Its nested rows (section 07: items with `subagent` set) render under
it, indented one step, collapsed by default. Expanding the row also shows the subagent's final
result. The summary rebuild a refinement turn runs (section 04) is one such row, labelled by its
description.

## Session states

The head's sub-line names the state in words, and a glyph never stands alone: colour is never the
only signal. The states are section 07's, with one more before the first message.

| State      | Head sub-line                                      | Panel                                                                        | Composer (authoring browser)                                  |
| ---------- | -------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------- |
| none       | no words                                           | The greeting (below)                                                         | Enabled with the context prompts; the first message starts a chat |
| Starting   | "Starting"                                         | A card with a ring spinner: "Starting the session", the name, model, effort  | Enabled; messages queue                                       |
| Idle       | no words                                           | The conversation                                                             | Enabled; Send delivers                                        |
| Working    | "Working"                                          | The typing bubble on the newest turn; the running tool's own row             | Enabled; Send queues; Stop beside Send                        |
| Permission | "Needs your answer", with a `--tone-warning` mark  | The permission card, pinned (below)                                          | Enabled; Send queues; Stop beside Send                        |
| Ending     | "Wrapping up"                                      | The ending card (above)                                                      | Replaced by "The composer comes back with your next chat."    |
| Ended      | "Ended"                                            | The conversation, then what the chat left behind                             | Replaced by "This chat has ended." and **Start a new chat**   |
| Failed     | "Stopped"                                          | The failure notice (below)                                                   | Replaced by "This chat has ended." and **Start a new chat**   |

The Idle row has no words in the head: the composer is the cue that the chat waits for the user.
A screen reader hears the state through a polite live region in the head, and a permission
request, an ended chat and a failed one through the chat announcer. The header's activity pill
keeps its own row for an idle chat, "Waiting for you" (section 02). The sub-line carries no timer,
tool summary or subagent count; the running tool's row says what is happening.

**Empty state.** With no session the panel shows a greeting in place of the transcript:
"Good morning. What are we deciding?" (morning before 12:00, afternoon before 18:00, otherwise
evening, by the browser's clock), then "I'll read the documents beside you and write down what we
settle. Your first message starts the session." Below it the context prompts (below) show as
pills above the composer. The panel carries no offer from section 01. New chat starts nothing: it
clears the concluded chat from the panel, and the composer takes focus. The user's first message,
with its attachments, starts the chat.

**Header glyphs.** The sheet-opening Chat button uses `ChatGlyph`: a turning arrow while the
chat starts (an operation that finishes), the green pulsing status dot for Working and Ending
(a state being shown), a ringing `BellRing` in `--tone-warning` while permission is needed, `!`
for Failed, `✓` for Ended, and a resting `MessageSquare` for Idle and no chat. Pending
permissions take priority over the session's state. The panel's own signs are the cards' ring
spinner and the working turn's typing bubble, so the head's sub-line draws no glyph of its own
for those states. The glyphs freeze under `prefers-reduced-motion`.

- **Failed** says which kind of failure: "The session could not start" with the spawn error, or
  "The session stopped unexpectedly" with the time of its last transcript activity. Both add:
  "The documents keep what the session recorded as it went. A new chat starts fresh." (section
  07, "Crashes and kills").
- **Ended** keeps the conversation readable until the next chat starts in this server (section
  07, "Reattaching and resume"). **Start a new chat** clears it from the panel at once. A
  message sent from the composer after a clicked prompt starts the next chat without clearing it:
  the ended conversation stays above the new one's starting state, under a divider with the end
  time ("Chat ended at 14:32"), and leaves the panel when the new session writes its first item.

The browser tab's title names the place and carries the state when it needs the user. On
`/f/<feature>/...` it reads `<slug> · loom plan`, elsewhere `<project> · loom plan`. A `(!)`
prefix shows while a permission decision waits, `(✓)` when a turn ended while the tab was hidden;
the prefix clears when the tab is shown. The holding browser can also get a desktop notification for both, through the
dashboard's opt-in notifications (`web/src/state/notifications.ts`), off by default. (My call.)

## Permission prompts

A permission request (section 07, "Permission prompts") renders as a card in the transcript at
the point it arose, and the same card is pinned above the composer while it is pending, so the
holder sees it wherever the transcript is scrolled.

In auto mode (section 07) a card appears only for a call that matches an `ask` rule in the
session's settings: `WebFetch`, and the `loom knowledge` and `loom plan summary` commands that
Kickoff and Team do not pre-allow (section 12). A call that Claude Code's classifier blocks shows
as a notice in the transcript, `Auto mode blocked Bash: [Irreversible Local Destruction]` (the
tool and the classifier's reason; `Auto mode blocked an action` when the payload names no tool).
The notice has no buttons, and its tone reads as the session having been stopped at that point,
not as an error.

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

While the session works or waits on a permission, a **Stop** button sits in the composer's action
row immediately before Send, at Send's size; it is absent otherwise. Send stays enabled, because a
message sent then is queued. Stop sends
section 07's interrupt. The transcript then shows "Stopped by Dana" as a notice, and any
permission card collapses as denied by the interrupt. Stop has no keyboard shortcut. (My call:
`Escape` already folds concept mode runs, and an accidental interrupt costs a turn.)

Interrupting moves every queued message back into the composer, joined with blank lines, instead
of delivering them at the Idle the interrupt causes. (My call: a user who stops a turn usually
means to redirect it, and a queued message written before the stop may no longer say what they
want.)

## Attachments

Selecting text in the reading column and choosing "Add to chat" (sections 05, 06), or pressing
`a` on a focused sentence (section 05), adds an attachment row inside the composer box, above the field. **Discuss**
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

- The attachments are one column of one-line rows: a glyph, the quote cut with an ellipsis in
  balanced quote marks, its source on the same line, and `×`. The context row ("Now viewing")
  and the reference rows share this anatomy. The area is capped at 7.5rem and scrolls inside the
  box, so the field stays in view with any number of attachments.
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

## Context: what the reader is viewing

The panel knows which document the reader is on. Each page sets a **viewed document**: a kind
(vision, product overview, section, metaplan, engineering overview, plan, pressure, run, file, or
none), a short name for the chip ("section_05", "Product overview", "metaplan"), and the
repository-relative source path to attach. The pressure and run pages have no path. The paths:

| Kind                 | Source path           |
| -------------------- | --------------------- |
| vision               | the vision            |
| product overview     | the vision            |
| section              | the section file      |
| metaplan             | that metaplan         |
| engineering overview | the first metaplan    |
| plan                 | the plan file         |
| file                 | the file's address    |

1. **Context line.** A quiet line in the panel head reads "About section_05", with the
   accessible name "Viewing section_05" and a "Dismiss context" button. It follows navigation
   immediately, whether or not a chat runs; while one does, it reads "Now viewing section_05".
   Dismissing clears the context for that document until the reader opens another.
2. **Composer placeholder.** "Ask about section_05, or say what to change"; with no context, "Ask
   about this feature".
3. **Prompts.** While no chat runs and the draft is empty, the composer area shows the prompts
   for the kind as a row of suggestion pills above the box, in a list named "Suggested prompts".
   A click fills the composer, focuses it and sends nothing, so no click spends tokens.

   | Kind                 | Prompts                                                                                                       |
   | -------------------- | ------------------------------------------------------------------------------------------------------------- |
   | vision               | Which sections does this imply? · What is still undecided?                                                    |
   | product overview     | What are the main decisions here? · What is still undecided?                                                  |
   | section              | What does this section decide? · What does this conflict with? · Change this section so that                  |
   | metaplan             | Is the plan order right? · Which plan carries the most risk?                                                  |
   | engineering overview | What are the main engineering decisions? · What could go wrong?                                               |
   | plan                 | What could go wrong with this plan? · Does this cover what its sections ask for? · Change this plan so that   |
   | pressure             | Which plan should be tested first?                                                                            |
   | run                  | What did the build change compared to the spec?                                                               |
   | file, none           | none                                                                                                          |

4. **Sending.** When the viewed document has a path that differs from the last one attached in
   this session, the message carries a source attachment for it, built the way **Discuss** builds
   one (above), ahead of the text. A passage the reader selected and added stays first. If the
   document cannot be fetched, the message goes without the attachment and the panel shows
   nothing alarming. The attachment already carries the path, so the server needs no new field:
   the first attachment's path picks plan or spec guidance for a chat started by a message
   (section 09).

## What a turn changed

Each turn ends with a footer under its last item.

```text
Changed 3 files
  product/feat-planner/section_02.md +12 −4    Diff
  plans/PLAN-plan-server.md        +3 −1     Diff
  product/feat-planner/section_09.md new       Diff
Rebuilt summaries
  section_02                       left for the background rebuild
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
summary page (section 05). While the turn runs the footer shows "N summaries rebuilding" and each
row reads "Rebuilding". When the turn ends each row reads "left for the background rebuild" for a
summary that was still stale at the turn's end (section 04, "Who rebuilds what").

**Cost.** The chat shows no token counts: no duration line in the footer, no running total, no
count on a collapsed turn or on a rebuilt-summary row. Where cost is still visible: Settings >
Summaries shows the rebuild token totals and the last rebuild (section 04, "Cost"); the footer
shows the Claude and Codex limits; a pressure test's header, its steps and its end card keep
their token totals (section 11). (My call: cost is an anti-goal concern, so it shows where the
spending is steered, not in the conversation.)

## Starting a chat

A chat starts in one of two ways, all from the browser that will hold authoring:

1. **An offer** (section 01) on the page that owns it: a missing document's page, a queue row on the
   product or engineering page, the Pressure test or Run page, an empty state's offer (section
   05), or a missing entry in file mode's tree (section 06). The panel offers none.
2. **Sending a message** when no chat runs: a new refinement session whose opening message is
   the composer's text and attachments, the viewed document included (below), with the brief of
   a message start (section 09 owns it). (My call: a reviewer who selected a passage and typed
   a question should not need a separate start step.)

**New chat** is not a way to start. It starts no session: it clears the concluded chat from the
panel and focuses the composer, and the user's first message then starts the chat as above. While
a chat is live the head's New chat button ends it first, after the confirmation below.

Starting shows the Starting state with the session's name
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

The panel has no End chat control. A chat ends when the user starts a new chat, opens another
offer, or leaves the planner (the holder's grace period, below). Starting a new chat or opening
an offer while a chat is live asks first (see "Starting a chat"). The session then shows Ending
until it reaches Ended (section 07). The closing write runs only when the user sent at least one
message (section 07); a chat without one ends at once, with no Ending card.

Closing the tab or the browser needs no action. A note under the composer says so the first time
a browser holds authoring: "Closing this tab keeps the session for 10 minutes. After that it
writes its notes into the documents and ends." The app sets no `beforeunload` prompt. (My call:
the grace period and the closing write make leaving safe, and an unload prompt on every reload
would teach users to dismiss it.)

While the holding browser is away, every other browser sees in the header's presence list that Dana is
away and that authoring "frees at 14:32" (section 02), from section 07's `away_until`. When the holder
returns within the grace period the list returns to normal. When it runs out, the session goes through Ending, and
any browser can start the next chat.

## Pressure runs

A pressure run (section 11) occupies the panel the way an authoring session does. Starting one
takes authoring like starting a chat (section 02).

- **Run header.** Under the head, a header names the plan,
  the round, and the step: "Pressure test · `PLAN-foo.md` · round 2 of 3 · /address", with the
  run's elapsed time and its token total. The head's sub-line shows the running step's state as
  for any session.
- **Claude steps.** Each Claude step renders as a run of turns, as any session's do, under a
  divider naming the step and its model. Its footer lists the plan edits the step made, with
  Diff links (above, "What a turn changed"), and its tokens, teammates included (section 11).
- **Codex step.** The Codex step is one status item: the model actually running, the elapsed
  time, and on finish either "Review written" with a link that
  opens the `codex-<plan file>` report in file mode, or "No review". On a failure the item shows
  the log's tail as monospace text. Codex tokens are not shown. While the step runs, its row in
  the step timeline turns an arrow, and so does the Claude `/pressure` row beside it when both
  run at once; a finished step shows a check and a step not yet started a dashed circle. The
  card's tile (a magnifier) and the run header's flask are still glyphs.
- **Composer.** The holder keeps the composer, and messages go to the foreground Claude step as
  in a chat. During the Codex step there is no session to deliver to, so Send is disabled with
  "Codex is reviewing; send when the next Claude step starts". (My call.)
- **Stop.** Stop is shown for the whole run and stops the whole run (section 11, "Stopping,
  failure, and server stops"), after confirming: "Stop the pressure test? The plan keeps the
  edits made so far, and no record is written." (My call: unlike a chat interrupt, stopping a run
  discards its progress.)
- **The end.** The header's last line says how the run ended: "Pressure test recorded", "Stopped
  by Dana", or "Step 3 failed" with the reason section 11 gives. The run's turns stay readable
  like an ended chat until the next chat or run starts.

Read-only browsers watch the same panel without the composer and without Stop.

## Read-only browsers

A browser that does not hold authoring (section 02, "Several people, one author") sees the
panel exactly as the holder does, live, with these differences:

- no composer while another browser holds authoring: in its place a muted line, "Read-only ·
  Dana is authoring". When nobody holds authoring there is no read-only browser: every browser
  gets the composer, and the first to send takes authoring (section 02);
- no Allow or Deny on permission cards, no Stop, no New chat (the Brief link still opens);
- queued messages as a count only;
- no "Add to chat" in the reading column (section 05); in its place "Send to author", whose
  suggestions reach the holder's panel as cards above the composer (section 10, "Read-only
  viewers").

**Presence.** The header's presence list (section 02) names the holder and lists the other
browsers by their browser names; the chat panel carries no presence line and never shows another browser typing. (My call: it would need a new
message lane for little gain.)

When `loom plan` runs in its read-only mode (section 02, "Missing prerequisites": `tmux` or
`claude` missing, Claude Code logged out, or the sandbox's prerequisites missing), every browser
is read-only. The panel shows a `HazardPanel` in its warning tone naming the missing piece and
how to install, log in, or fix it, and no offers.

## Share

Every authenticated browser has a **Share** control beside the header's presence list (section
02). It opens a popover that asks the server for the tokenized link (section 12, "Sharing
access") and shows it in monospace with a Copy button. The popover shows the URL only, with no
tunnel instructions; on a remote bind it adds the plain-HTTP warning in `--tone-warning`.

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
  announced once. Tool rows are not announced; the head holds a visually hidden polite live region that
  announces state changes, and says nothing for an idle chat.
- A new permission request is announced assertively once, naming the tool, and moves focus to
  the pinned card only when focus was already inside the chat panel.
- Every glyph has a text label; the state is always in words.
- The panel is one landmark (`aside`, labelled "Chat"); `Tab` from the reading column reaches it
  (section 05), and `c` focuses the composer.
- Animations follow `prefers-reduced-motion` as the dashboard's do: the spinners, the arrow, the
  status dot and the typing bubble's dots rest, and new items appear without motion.

## Out of scope for this section

- Session lifetime, transcript reading, hook events, delivery, the queue, and the model's fields:
  section 07.
- What the session does with an attached passage, the delivered attachment format, restoring
  what a turn changed, and what changed since a reviewer last looked: section 10.
- The briefs, opening and closing messages, and models per step: section 09.
- Diff views and the file-mode side of "Add to chat": section 06; concept mode's side: section
  05.
- Cost measurement, and the summary panel: section 04.
- The header's presence list and browser naming: section 02.
- Which paths sessions may write and the access checks on chat commands: section 12.
- Running a pressure run, its steps, and how it stops: section 11; the share link's access
  rules: section 12.
