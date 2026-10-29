# 10 — Review and Refinement Loop

Part of the [Loom Plan product vision](index.md).

This section defines the loop that connects reviewing and authoring: a reader doubts a passage,
takes it into a chat, the session edits the documents, and the reader sees what changed. It
settles how attached passages are written into the message the session receives, how a reviewer
undoes a turn's edit, what a refinement may do once plans have run, how a product change points
at the plans that implement it, what a read-only browser can contribute, where open questions
show, and how a reviewer sees what changed since they last looked. Step 6 of the workflow is the
user reading; the app records no approval (vision, Decisions, "Approval").

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
   and every rebuilt summary with its cost.
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

There is no approve button, no sign-off record, and no "reviewed" flag on any document. Step 6 is
the user reading; when they are satisfied, starting a run is their act (section 11). A team that
wants a record of who agreed uses its usual means, a pull request review of the committed `doc/`
tree, which the plain-Markdown rule (vision, principle 1) makes possible.

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
| The metaplan's account of a plan          | Edits the metaplan. Metaplans are never locked; a new round may go in a new metaplan (section 01)                            |

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
  matter at step 6 as much as a section's.) The count links to the heading in file mode.
- **Reading.** The heading is an ordinary heading section: summaries may cite it, and it opens in
  concept mode like any passage.
- **Answering.** An open question is attached like any passage. A session that settles one moves
  the decision into the body of the document and removes the item (section 09's guidance), so the
  count falls and the turn diff shows both edits.

## What changed since you last looked

The app stores no state, so "since you last looked" is kept by each browser for itself, and git
supplies the old text.

**The checkpoint.** Each browser keeps in local storage, per project, a checkpoint: the commit
`HEAD` pointed to and the content hash (section 04) of every document file mode shows, taken when
the checkpoint was set. The first visit sets it silently. After that it moves only when the
reviewer presses **Mark all seen**; opening one file's "since you looked" diff marks that file
seen alone, by updating its hash in the checkpoint.

**What the reviewer sees.** A header control, "Changed since you looked · 4", opens a list of the
documents whose current hash differs from the checkpoint's, plus documents added and removed since.
Each row has a diff, a fourth kind beside section 06's three: before is the document's text at the
checkpoint commit, after is the current file. Concept mode marks the key chip (section 05) of
every changed document with a small dot labelled "changed since you looked", and file mode's tree
marks the entry the same way.

**Comparing with a ref.** The same list can compare against any git ref the reviewer types, such
as `main` or a tag, using that ref's text and hashes instead of the checkpoint's. A reviewer
checking a branch before it merges compares with `main`. The ref is not stored. (My call.)

**Limits**, stated where the list is shown:

- A document that had uncommitted edits when the checkpoint was set diffs against its committed
  text, so the diff can include edits the reviewer already saw; its row says so.
- A checkpoint commit no longer in the repository (after a rebase or a reset) cannot supply old
  text; those rows list the file as changed, with only the uncommitted and ref diffs offered.
- Local storage belongs to the page's origin, which includes the port. A server that bound a
  different port (section 02, "Port") starts with no checkpoint.
- The checkpoint belongs to the browser; another browser or a cleared storage starts fresh.

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
