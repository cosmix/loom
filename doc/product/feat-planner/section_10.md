# 10 — Review and Refinement Loop

Part of the [Loom Plan product vision](index.md).

This section defines the loop that connects reviewing and authoring: a reader doubts a passage,
takes it into a chat, the session edits the documents, and the reader sees what changed. It
settles how attached passages are written into the message the session receives, how a reviewer
undoes a turn's edit, what a refinement may do once plans have run, how a product change points
at the plans that implement it, what a read-only browser can contribute, where open questions
show, how a reviewer sees what changed since they last looked, how a reader settles a document,
and how findings are triaged. Reviewing is the user reading, which has no step in the workflow;
approval is the settled mark (vision, Decisions, "Approval").

## The loop in one paragraph

The reviewer reads in concept mode or file mode until a claim looks wrong. They select it and
choose "Add to chat", write what they doubt, and send. The session receives the message with the
passage's text and its exact source, checks the documents, and either answers or edits them. When
the turn ends, the chat lists the files it changed with a diff for each and names the summaries
the turn rebuilt; the passage the reviewer attached says whether its section changed. The page
the reviewer is reading updates where it stands: an open source passage re-renders, summary
sentences built on the changed text turn dashed and are replaced when their rebuild arrives. If
the edit is wrong, the reviewer says so in the next message, or restores the file. Nothing in the
loop is stored except the documents.

## The steps

| Step | What the reviewer does                                  | What the app does                                                     | Owner      |
| ---- | ------------------------------------------------------- | --------------------------------------------------------------------- | ---------- |
| 1    | Opens a sentence down to its source, or reads a file    | Shows the expansion chain and the source passage                      | 05, 06     |
| 2    | Selects the doubtful text, "Add to chat"                | Adds a chip with the passage and its provenance                       | 05, 06, 08 |
| 3    | Writes the question or the correction, sends            | Formats the attachments into the delivered text (below)               | 08, this   |
| 4    | Reads the reply; answers permission prompts             | Shows blocks, tool rows, the state                                    | 07, 08     |
| 5    | Opens the turn's diffs                                  | Lists changed files, rebuilt summaries, cost                          | 06, 08     |
| 6    | Reads on                                                | Updates the view in place; flashes replaced sentences                 | 05, 06     |

Step 1 matters for summaries. A doubtful overview sentence can be wrong in two ways: the summary
misstates the document, or the document says the wrong thing. Opening the sentence to its source
tells the reviewer which, and the attachment carries both the sentence and the sources it rests
on, so the session can tell too.

## Taking a passage into chat

Any number of passages, from either mode and from several documents, can be attached before one
message is sent. Attaching never starts a session; sending does. When no chat is running, the
first message with its attachments starts a refinement session with that message as its opening
message (section 08, "Starting a chat"), and the sending browser takes authoring.

The reviewer does not need to say where the passage is; the attachment says it. They write what
they doubt in their own words: "this plan assumes the daemon is running; it should not".

### What the session receives

Section 07 delivers text into the pane, so attachments become text. The server appends one block
to the user's message, in a fixed form:

```text
This plan assumes the daemon is running; it should not.

<loom-attachments>
[1] source doc/plans/PLAN-plan-server.md#lifetime lines 40-44
> `loom plan` starts the daemon if it is not running and
> registers the planning server with it.
[2] summary doc/loom/summaries/plans/plan-server.md#s-3fa9c2d1 rests-on doc/plans/PLAN-plan-server.md#lifetime
> The planning server runs inside the loom daemon.
[3] source doc/plans/DONE-PLAN-token-flow.md#stage:integration-verify locked=done changed-since-selected
> Integration verify runs the dashboard against a live daemon.
</loom-attachments>
```

The header line of each entry is `[n] <kind> <address>` followed by any of these, space
separated:

| Field                     | Present when                                                                   |
| ------------------------- | ------------------------------------------------------------------------------ |
| `lines a-b`               | The selection was made in the editor or a diff (section 06)                    |
| `rests-on <addr>,<addr>`  | A summary sentence: the source sections it rests on (section 05)               |
| `locked=<reason>`         | The document is locked: `running`, `done`, `retired`, `knowledge`, `brief`     |
| `changed-since-selected`  | The document's hash changed between the selection and the send                 |

- `<kind>` is `source` for document text or `summary` for a summary sentence.
- `<address>` is the document path and anchor as section 03 defines anchors, including
  `#stage:<id>` for a Stages panel selection; a summary sentence is addressed by its summary file
  and its sentence id (section 04).
- The selected text follows, each line prefixed with `>`. Text longer than 2,000 characters is
  cut at a line boundary and ends with `> […N more characters; read the source]`. (My call: the
  session reads the file anyway; the quote only has to identify the passage.)
- A selection that spans levels in concept mode (section 05) becomes one entry per part.

The block goes at the end so the user's words come first in the transcript. The fixed form lets
the bridge read the block back out of the transcript's user record: the chat shows the user's
text with chips, and a reload rebuilds the chips from the transcript, as section 07 rebuilds the
whole model. A user message containing a literal `<loom-attachments>` line has it escaped by the
server before delivery. (My call.)

Section 09's guidance tells the session what the block means: read each addressed section before
answering; for a `summary` entry, compare the sentence with the sections it rests on and say
whether the summary or the document is wrong; for a `locked` entry, never edit the document
(below); for `changed-since-selected`, the reviewer saw older text.

### When the summary is wrong

When the session finds that a summary sentence misstates a document that is right, it leaves the
document alone and rebuilds that summary through `loom plan summary brief`
and `seal` (section 04) in the same turn, giving its rebuild subagent the doubted sentence and
what the source says instead through `brief --note`. The turn's footer then shows a rebuilt
summary and no changed document, which is what happened.

A reviewer who does not want a chat can also press rebuild on the summary (section 04, "Who
rebuilds what"); that rebuild gets no note, so it fixes a wrong sentence only by chance. The chat
route is the one that carries the reason.

## Seeing what a refinement changed

When the turn ends, the reviewer has four views of its effect, all already defined elsewhere:

1. **The turn's footer** (section 08): every changed file with its line counts and a Diff link,
   and every rebuilt summary with its cost. Under each changed document it lists the downstream
   impact: the related documents that now read revised, stub changed or upstream changed, and the
   plans covering a changed section, each with its settle state.
2. **The turn diff** (section 06, "Diffs"): source text before the turn and after it, per file,
   covering subagent edits and the summary rebuild.
3. **The chips on the sent message** (section 08): each attachment says "changed in this turn"
   with a link to its file's turn diff scrolled to the attached heading, or "unchanged". (My call:
   the reviewer's first question after a turn is whether the thing they raised was touched.)
4. **The reading column**: an open source passage re-renders and flashes as soon as its file
   changes; summary sentences built on it turn dashed, and their replacements flash once when the
   rebuild arrives (section 05, "Rebuilds while reading"; vision, Decisions, "Refinement"). A file
   open in file mode updates with a "changed just now" note (section 06).

A turn that answered without editing has no file list; the reply is the result.

### Undoing a turn's edit

The reviewer can reject an edit two ways.

- **In the chat.** "Put back the lifetime section; I was wrong." The session edits again, and the
  next turn's diff shows it. This is the route when the session should also change its
  understanding, and the only route for a file the turn created or deleted.
- **Restore.** The turn diff of a file the turn modified has a **Restore** action for the holding
  browser: it writes the file's text from before the turn. A turn footer with several modified
  files has **Restore all**, which asks first and lists the files. Undoing a wrong edit should
  cost no tokens and depend on no session.

Restore goes through file mode's save path (section 06, "What the server does on save") with the
file's hash at the end of the turn as the opened hash. If the file changed since the turn ended,
the restore is refused and section 06's merge view opens with the turn's starting text on the
right, so the reviewer chooses chunk by chunk. Restore is offered only while no turn is running,
so it never races a session's edit, and not for created or deleted files, since file mode never
creates or deletes (section 06). A plan restored to text with validation errors gets section 06's
"Save with errors" confirmation.

A restored file is an outside change: the summaries the turn rebuilt go stale again and the
background rebuild follows, under the rebuild budget (section 04). The next chat message tells the
session nothing about the restore unless the reviewer says so; the session reads the files as
they are.

Turn diffs live for the server's lifetime (section 06), so Restore does too. After a restart the
uncommitted diff and git remain.

## Approval

Approval is the settled mark of the next section: a reader who is satisfied with a document marks
it settled. There is no separate sign-off record and no "reviewed" flag. Starting a run is the
user's act (section 11). A team that wants a record of who agreed uses its usual means, a pull
request review of the committed `doc/` tree, which the plain-Markdown rule (vision, principle 1)
makes possible.

## Settling a document

The summary page and the source page of the vision, a product section, the metaplan or a plan
without a lifecycle prefix show the document's settle state beside its title: draft, revised,
stub changed, upstream changed or settled (section 03 defines the `Settled:` line and its
hashes). The mark is document-level; a plan is settled together with its brief bundle.

- **Mark settled** writes the `Settled:` line, through the save path, covering the document's
  text now, then has it committed so the settled version is a commit. Only settling actions write it;
  sessions never do (section 09, "Write limits"). Mark settled needs authoring, like every write.
- **Settle all specs** on the product overview and **Settle all plans** on the metaplan page settle
  each settleable, unsettled section or plan through the same save and commit queue. They appear
  only when a document of that role needs settling. Stubs, stale documents and documents with
  unresolved calls or open questions are reported as skipped; unresolved documents can be settled
  with a second confirming press. The response lists settled paths and skipped paths with reasons.
- **The settle commit.** It holds the document and, for a plan, its brief bundle, and nothing
  else: other staged or modified files stay as they were. Its message is
  `docs(<feature>): settle <path under doc/>` (`unsettle` for Unsettle, which commits the
  document alone), with no trailer. It is a plain `git commit` on the checked-out branch with
  the user's identity, hooks and signing, never `--no-verify`, whatever the project's hooks are:
  they may run for minutes, rewrite the files, refuse or commit themselves. The commit runs after
  the settle answers, on a worker that takes one settle at a time in the order asked, so a slow
  hook holds up only the settles queued behind it; how each ended reaches every browser, and git's
  and the hooks' own words reach only the browser that settled. When the paths already match HEAD
  there is no commit. A document, or a plan's brief, edited between the settle and its commit is
  not committed, and the reader is asked to settle again. Git's exit status does not decide
  whether the commit happened: HEAD is read before and after it, and the settle's commit is the
  one git added on top of the old HEAD with the settle's message, whatever log encoding the
  repository sets. Git ignores a `post-commit` hook's exit status, and one that runs past the
  deadline, prints past the output limit or commits on top of it still leaves the settle
  reporting its own commit; HEAD that did not move means nothing was committed. A hook that
  rewrites a settled file into the commit leaves the line describing the text before it: the
  commit is reported with "A commit hook changed the document after it was settled. Settle this
  version to record it.", and the document reads changed since it was settled. A commit that holds
  what was settled while the file changed beside it, by a hook or by the reader while the hooks
  ran, is reported with "The document changed after it was settled. Settle this version to record
  it.", since loom cannot tell those two apart. The line is never marked again on the reader's
  behalf, and the commit is never amended, since after a hook HEAD may not be the settle's. A file
  a hook adds beside the settled ones goes into the commit and its index entry is left as
  committed, so the next commit does not undo the hook's work; a file the reader had staged that a
  hook also committed keeps the reader's staged version, and the note names it. A commit past ten
  minutes is stopped (git first gets the chance to release its index lock, and a lock left behind
  is named). A merge, rebase, cherry-pick or revert in progress, or a project outside git, skips
  the commit; a refusing hook, a timeout or a git failure before any commit leaves the line
  written and unstages a new brief the settle had added, as does a failed commit after something
  else moved HEAD. When the server stops, the commit in flight runs to its end and the settles
  queued behind it end as not committed. While the commit runs the control says "Committing…"; a
  commit then shows "Committed <short sha>" for a moment, a commit that left something to do says
  "Settled and committed." and what is left, a line not committed says "Settled, but not
  committed:" (or "Unsettled, …") and why, and a reconnect while the commit runs says "The commit
  result was lost; check git log." until its end arrives, if it still does. A second press while
  the first press's commit still runs waits for both: the note follows the second, and the first
  one's failure still shows.
- **Unresolved points.** A document with `(My call.)` markers or open questions is not settled
  silently: the dialog lists them and the reader confirms, or backs out.
- **Unsettle** removes the line and commits that. A locked document (running, done, retired or
  archived) can be neither settled nor unsettled; it keeps the line it had.
- **Changes since settled.** A document that reads revised offers the comparison with the version
  that was settled: the newest commit whose text holds the current `Settled:` line. The view is
  the commit comparison of "Seeing what a refinement changed". The page asks for that commit as it
  shows the control. When the line was never committed (a settle whose commit was skipped or
  refused), See what changed stays disabled and says the settled version was never committed, so
  there is nothing to compare, and that settling again records it. A hook's rewrite that stayed
  out of the settle commit shows as the change since it; one that went into it is already in that
  commit, so the comparison shows no difference. This is separate from
  "What changed since you last looked", which is each browser's own record.
- **Stubs.** A section or plan shows its stub, the vision row or metaplan entry, above its body,
  linked to its place, with a note when the stub changed since the document was settled. A body
  whose scope moved past its stub is the session's duty to fix in the same turn (section 09);
  the app records only that the stub changed.

## Triaging findings

The Findings page (section 11, "After a run") lists the feature's ledger. Each open finding offers
three actions for the holding browser:

- **Plan this** starts the `Plan follow-ups` session, seeded with the finding's id, title and
  About nodes. The session writes the follow-up entry and sets the finding to `planned`.
- **Fold into entry** picks an entry whose plan has not started running (unwritten, or written
  and not yet run). The session starts seeded the same way, edits the entry to include the
  finding, and sets the finding to `folded`.
- **Dismiss** takes a reason, required, one line of at most 200 characters, and sets the finding
  to `dismissed: <reason>` without a session.

Finding titles, text and reasons are shown as text, never as markup.

## Refinement after plans ran

The returning author (vision, Users) comes back after some plans have run. A refinement then
works on the same documents with one rule: the locked set (running, done, and retired plans,
everything under `doc/plans/archive/`, run reviews `REVIEW-*.md`, and the brief bundles of
locked plans) is read-only to sessions as it is in the editor (vision, Decisions, "Locked
plans"; section 06). Section 12 enforces this for sessions; section 09's guidance tells
the session before it tries.

What a refinement may do instead of editing a locked plan:

| The change concerns                       | The refinement                                                                                                               |
| ----------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| What a product section says               | Edits the section. It is never locked.                                                                                       |
| Work a done plan built                    | Writes a new plan for the change, with its own slug and `Implements:` line, and an entry for it in a metaplan                 |
| Work a running plan is building           | The same: a new plan, to run after the running one. Correcting a running stage's verification is `loom stage amend`, a terminal operation outside this app |
| A retired plan the author wants back      | A new plan with a new slug; reusing the retired slug would be a stray copy (section 03, "Plans and their lifecycle")          |
| The metaplan's account of a plan          | Edits the metaplan. Metaplans are never locked; follow-ups append entries to `index.md` (section 01)                         |

The new plan names the plan it follows in its prose ("Follows `DONE-PLAN-token-flow.md`"), as a
cross-reference section 03 already resolves. (My call: no new convention, and a plain reader sees
the chain.) Run reviews (`REVIEW-*.md`, section 03) are read-only to sessions and attachable like
any document, so the author can take "what was built" into the chat that plans the follow-up.

## Refining across product and plans

Product sections and plans are linked only by plans' `Implements:` lines (vision, Decisions,
"Product-to-plan links"; section 03). The app uses those lines to show the reach of a product
change, and never edits a plan on its own.

**In the turn's footer.** Under each changed product section the footer (section 08) lists the
plans that implement it, grouped by state:

- **Written plans**: "may need updating", with a **Check these plans** action for the holding
  browser. It fills the composer with "Check these plans against the change to section_02." and
  attaches the changed section and each plan, as source entries. The user edits or sends it. (My
  call: the session should not widen a refinement to plans the user did not ask about, and the
  user should not have to find them.)
- **Running and done plans**: "built from earlier text", with a **Plan a follow-up** action that
  fills the composer the same way for a new plan (above).

**On the product section's summary page.** Section 05 lists "Implemented by:" with each plan's
state. For a done plan, this section adds a marker when the product section changed after the
plan was built: "Changed after PLAN-token-flow was built", with a link to the diff.

The build date is the `**Generated:**` date in the plan's run review (`REVIEW-<plan file
stem>.md`, written at `loom/src/fs/plan_review/mod.rs:81`), the only dated record of a finished
run in the files. The section's change date is its last commit
(`git log -1 --format=%cI -- <path>`), or now when the file has uncommitted changes. The diff
runs from the section's text at the last commit before the build date to the current file. Known
limits, stated in the marker's tooltip: `loom run` regenerates the review when it runs again on a
done plan (`loom/src/fs/plan_lifecycle.rs:194`), which moves the date later, and a done plan
without a review shows no marker. A running plan has no recorded start, so it gets the fixed note
"This plan is running; changes here do not reach it" instead of a marker. (My call: git and the
review are already in the repository, so this needs no new record.)

## Read-only viewers

A read-only browser writes nothing: no chat, no save, no restore, no rebuild approval (sections
02, 05, 06, 08). It can still hand a passage to the person authoring.

Selecting text in a read-only browser shows **Send to author** where the holder would see "Add to
chat". It opens a small form for a note of at most 1,000 characters and sends the passage, its
provenance, the note, and the sender's browser name to the server. The holder's chat panel shows
it as a suggestion card above the composer: "Sam suggests: section_02.md › Port", the note, and
two actions, **Attach** (adds the chip and puts the note in the composer, prefixed with "Sam
asks:") and **Dismiss**. Nothing reaches the session unless the holder sends it. Section 12
("Suggestions and restores") sets the route's access rules.

- Suggestions live in the server's memory, at most 20 pending, and are lost on restart; the
  sender sees "Sent to Dana", then "Attached" or "Dismissed".
- When nobody holds authoring, there is no one to send to: the control is hidden, and the
  read-only browser can start a chat itself (section 08).
- When the holder is away, suggestions wait and the sender sees "Dana is away".

Without this, a read-only reviewer's only channel is copying a link to the passage (section 06's
addresses) and sending it outside the app.

## Open questions in documents

Sessions write unresolved points under a heading named `Open questions` in the document they
affect (vision, Decisions, "Sessions start cold"; section 03). The reviewer meets them in three
places:

- **Counts.** Section 01 counts them beside each product section in the checklist. The same count
  shows on every document's summary page header and on its file mode tree entry, for any document
  with that heading: product sections, metaplans, and plans. (My call: a plan's open questions
  matter in a review as much as a section's.) The count links to the heading in file mode.
- **Reading.** The heading is an ordinary heading section: summaries may cite it, and it opens in
  concept mode like any passage.
- **Answering.** An open question is attached like any passage. A session that settles one moves
  the decision into the body of the document and removes the item (section 09's guidance), so the
  count falls and the turn diff shows both edits.

## What changed since you last looked

The app stores no state, so "since you last looked" is kept by each browser for itself: what the
reviewer saw of each document, and a commit to fall back on.

**The checkpoint.** Each browser keeps in local storage, per project, a checkpoint: the commit
`HEAD` pointed to and the content hash (section 04) of every document file mode shows. The first
visit sets it silently. **Mark all seen** moves the whole checkpoint to now. Opening one
document's diff marks that document seen alone: its hash in the checkpoint moves to the text the
diff showed.

**What was seen.** Beside the checkpoint the browser keeps the text it showed of each document,
paired with the hash it was read at: the text of the document's last diff, or its text when Mark
all seen was pressed (read for every document the list showed then; one that cannot be read keeps
no text). A stored text is a document's baseline while its hash is the checkpoint's. Each text is
capped at 256 KiB and all of them at 2 MiB, the least recently seen dropped first. A document with
no stored text (over the cap, unreadable, or never viewed) falls back to the checkpoint commit,
and its row says "Changes since commit <short sha>".

**What the reviewer sees.** The activity popover's row "Changed since you looked · 4" opens a
dialog that states what it compares: documents that changed since the reviewer last looked at
them in this browser. Its list holds the documents whose current hash differs from the
checkpoint's, plus documents added and removed since, each with its status as a glyph and a word.
Each row has a diff, a fourth kind beside section 06's three: before is what this browser saw of
the document (else its text at the checkpoint commit), after is the current file. While a diff is
open it follows the document, so a later edit shows from what was just seen; the open document
stays in the list, marked seen, until another is chosen. Concept mode marks the key chip (section
05) of every changed document with a small dot labelled "changed since you looked", and file
mode's tree marks the entry the same way.

**Comparing with a ref.** The same list can compare against any git ref the reviewer types, such
as `main` or a tag, using that ref's text and hashes instead of the checkpoint's. A reviewer
checking a branch before it merges compares with `main`. The ref is not stored, and a ref diff
marks nothing seen. (My call.)

**Limits**, stated in a short note at the dialog's foot:

- A document diffed from the checkpoint commit that had uncommitted edits when the checkpoint was
  set diffs against its committed text, so the diff can include edits the reviewer already saw.
- A checkpoint commit no longer in the repository (after a rebase or a reset) cannot supply old
  text; a document with a stored text still diffs from it, and the others offer only their
  uncommitted diff.
- Local storage belongs to the page's origin, which includes the port. A server that bound a
  different port (section 02, "Port") starts with no checkpoint.
- The record belongs to the browser; another browser or a cleared storage starts fresh.

## Out of scope for this section

- Selecting text and the attachment's structured form: sections 05 and 06.
- Rendering chips, turn footers, permission cards, and the composer: section 08.
- Turn boundaries, delivery, and the model's fields: section 07.
- Diff views, the save path, and the merge view: section 06.
- What the session's guidance says about attachments, locked plans, open questions, and
  follow-up plans: section 09.
- Summary rebuilds, the budget, and `brief` and `seal`: section 04.
- Pressure testing and starting a run after review: section 11.
- Enforcing locked paths for sessions, and access checks on the suggestion and restore routes:
  section 12.
