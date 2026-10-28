# chat-web W4: composer, queue, chips, permission and suggestion cards

Dispatch: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 600000 ms. The orchestrator partitions this territory into ordered file-sized forwards before dispatch to fit the 540 s deadline; workers never delegate. All test/proof commands below are for the orchestrator; workers run compile-only checks, no git, and never write under `.loom/`.

Working dir `web/`. You own `src/plan/chat/{composer.tsx, composer.test.tsx, composer-gate.ts,
stop-control.tsx, queue-strip.tsx, chips.tsx, chips.test.tsx, permission-card.tsx,
permission-card.test.tsx, suggestion-cards.tsx}`. W1's atoms, commands and formatter exist.
Frozen contracts: `src/plan/chat/chat.contracts.test.tsx`.

## composer.tsx

`<ComposerArea />` (W2 mounts it): queue strip, pinned permission card, suggestion cards (holder
only), chips, then the composer. A plain `textarea` named "Message" growing to eight lines;
`Enter` sends, `Shift+Enter` newline; `c` anywhere outside an input focuses it (W6 adds its row to
plan 2's help keys; you own only the key handler). Draft text and chips live in W1's `draftAtom`. Send: the size check
uses `deliveredBytes(text, attachments)` built on `formatMessage(` (call it here so the stage's
wiring check, which greps `formatMessage(` in this file, sees the composer count the block) against plan 7's cap, 100,000 bytes after `format_message` and
sanitising (`plan_session::input::check_delivered`), disabling Send and saying by how much; a
message starting with `/` other than `/compact` disables Send with "Slash commands are not
available here; `/compact` is."; `composerBlock(session)` from `composer-gate.ts` gives any
other reason (stopping server, Ending, read-only mode; plan 11 adds the Codex reason there).
Delivery states per section 08's table (queued in the strip; sent with muted "Sending"; delivered
plain; undelivered "Not delivered" in `--tone-warning` with Resend). A refused command shows the
server's reason in one line under the composer. Read-only browsers: no composer, the muted line
"Read-only · <holder> is authoring"; while nobody holds authoring every browser has the
composer. The first time this browser holds authoring, a note under the composer: "Closing this
tab keeps the session for 10 minutes. After that it writes its notes into the documents and
ends." (remember it was shown in `localStorage`). No `beforeunload`.

## stop-control.tsx, queue-strip.tsx

`<StopControl confirm?: string />`: shown while Working or Permission, sends interrupt (with a
confirm dialog only when `confirm` is set). After an interrupt the queued texts return into the
composer joined with blank lines, taken only from plan 7's `queue_returned` frame (W1's
`queueReturnedAtom`), appended after any draft text; the browser's queue copy is never used. Queue strip: "Queued (<n>)", each with its first line, remove and edit (edit
withdraws through W1's command and puts the text back); viewers see the count only.

## chips.tsx

Pending chips from plan 3's `pendingAttachmentsAtom` (kept in `draftAtom` across reloads): label per section 08's table
(`section_02.md › Port`, with `· lines 54–60` from the editor or a diff, `PLAN-foo.md › stage
<id>`, `Summary · section_02` or `Product overview`); hover or focus shows the selected text in
full; click opens the passage in its mode; `×` removes; "changed" in `--tone-warning` when the
document's current hash (W1's hashes) differs from the hash at selection; a lock glyph for a
locked document (knowledge, locked plans and their briefs). At most 10: the eleventh add shows
"At most 10 attachments per message" and is dropped. `<SentChips item turn />` for sent user
messages: the same chips read from the model's parsed attachments, plus "changed in this turn"
with a link to that file's turn diff at the heading, or "unchanged", from chat-server's
`turn-sections` route once the turn has ended.

## permission-card.tsx

`<PermissionCard item inline? />`: `role="region"` named "Permission: <tool>"; tool name, input
(Bash: the command in full, monospace; Edit/Write: path and unified diff via W3's
`patch-view.tsx`; other tools: indented JSON), time left before the one-hour automatic deny.
Holder: Allow, Deny (opens a one-line note with "Deny" and "Deny with note"); with the pinned card
focused `y` allows and `n` opens the note; nothing while the composer has focus. Viewers: no
buttons, "Waiting for <holder> to decide". The pinned copy sits above the composer while pending;
a new request is announced assertively once and takes focus only when focus was inside the
panel. Answered: one line per section 08's outcome table (allowed, holder with the note, timeout,
interrupt, session_ended or server_stopping, hook_error).

## suggestion-cards.tsx

Holder only: one card per pending suggestion, "<name> suggests: <chip label>", the note as text,
Attach (adds the chip built from the document as the server reads it, and puts "<name> asks:
<note>" into the composer) and Dismiss, each sending `suggestion_answer`.

## Attach from a suggestion

Attach builds the chip from the document as the server reads it now: fetch plan 3's `GET
/api/doc?path=` for the suggestion's path; when the suggested text still occurs in the
document, attach it with the document's current hash as `selected_hash`; otherwise attach the
section at the suggested anchor and mark the chip "changed". Attach issues no chat command.

## Required test cases (coverage audit; named tests)

`composer.test.tsx`: "viewer sees the read-only line, no textbox"; "everyone composes when
authoring is free"; "send with no chat issues start"; "send while starting queues"; "send while
idle"; "stop shown while working"; "stop issues interrupt"; "viewer has no stop"; "ending
disables with reason"; "ended replaces composer"; "stopping disables"; "enter sends, shift enter
newline"; "grows to eight rows"; "c focuses composer"; "delivery states" (queued, sent,
delivered, undelivered); "resend issues send again"; "queue strip lists first lines"; "remove
withdraws"; "edit withdraws and refills composer"; "viewer sees queue count only"; "escape in the
panel does not interrupt"; "refused command shows reason"; "first-hold note once"; "no
beforeunload listener"; "composerBlock reason disables send"; "stop confirm prop asks first".
`chips.test.tsx`: "labels per origin"; "hover full text, click opens, x removes"; "changed since
selection"; "lock glyph for locked and knowledge docs"; "chips from file and concept modes
accumulate"; "adding a chip sends nothing"; "spanning selection adds one chip per part"; "sent
chips from model attachments"; "sent chip changed in this turn links to diff at heading";
"unchanged". `permission-card.test.tsx`: "bash body"; "edit body diff"; "other tool json"; "time
left from expires_at"; "deny with note sends note"; "two buttons only"; "y and n keys"; "keys
ignored in composer"; "outcome lines" (table-driven over all six outcomes); "assertive
announcement and focus rule"; "input renders as text". Suggestion cards (in `chips.test.tsx`):
"attach fills chip and composer"; "attach re-reads the document and marks changed text"; "attach
sends no chat command"; "dismiss answers"; "note renders as text".

## Fix-round additions (pressure test px-browser-09, -10, -11, -12, -13, -14, -19, px-security-09, px-state-07)

- The draft clears only when the server accepts the send (the command's `result` ok). A rejected
  or unconfirmed send keeps the text; Resend is offered only for W1's unconfirmed sends that the
  next snapshot did not show as delivered.
- Losing authoring with text in the composer: the composer becomes the read-only line plus "Your
  draft is kept"; regaining authoring restores the composer with the draft.
- `storageFullAtom` true: a muted line under the composer, "Unsaved text is not being kept:
  browser storage is full".
- Several pending permissions: pin the oldest card headed "1 of N decisions"; `y`/`n` act on the
  pinned card; after its answer the next one pins.
- Height budget: the composer textarea and Send are always visible. Suggestion cards sit in a
  disclosure "Suggestions (<n>)" showing the newest card, the rest in a list capped at about a
  third of the panel height with its own scroll; chips wrap within two rows, the rest behind
  "+<n> more"; the queue strip shows two first lines and "+<n> more".
- Controls this browser may not use render through plan 2's `DisabledReason` with the reason. In
  read-only server mode (nobody can author) the line is "Read-only: <first missing piece>", never
  "Read-only · <empty> is authoring". During Ending `composerBlock` returns "the chat is closing"
  and plan 7 refuses a send with the same words.
- Permission card bodies (command, input JSON, path, diff) render through W1's `<Untrusted>`.
- Tests: "draft kept when the send is rejected"; "resend offered only for an unconfirmed send";
  "losing authoring keeps the draft and shows the read-only line"; "storage full note"; "second
  pending permission pins after the first is answered"; "1 of 2 decisions"; "20 suggestions, a
  queue of 3, a pending card and 10 chips keep the textbox outside the capped containers"; "read
  -only server mode line names the missing piece"; "reasons are accessible descriptions"; "ending
  shows the closing reason"; "refill uses only queue_returned when the queue changed after Stop".
