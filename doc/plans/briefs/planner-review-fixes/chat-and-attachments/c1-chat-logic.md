# C1 — chat behaviour in the browser (issues 7, 8, 9, 10)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first (D6, D7, D8, D9). V2 restyles in parallel. You own behaviour in the files the plan YAML gives you; V2 owns CSS and the layout components (`panel.tsx`, `panel-head.tsx`, `context-line.tsx`, `composer.tsx`, `chat-state-glyph.tsx`). Report every change you need in a V2 file instead of making it.

## Transcript (D6)

- `loom-kickoff` fences never render. In the chat's assistant markdown path (find where an assistant `text` item renders, `chat/items.tsx` and the markdown it calls), a fenced code block whose info string is exactly `loom-kickoff` is dropped before rendering. Prose around it stays. Do this in chat code (a new `chat/kickoff-fence.ts`), not in `web/src/plan/markdown/**`, which documents use.
- Skill tool rows never render: a `tool` item whose tool name is `Skill`. Drop it in `chat/tool-group.ts` before grouping. Name the unit test `it("drops a Skill tool row …")` (a plan `after_stage` greps for that).
- Folding (`chat/tool-group.ts` `groupToolRows`, `transcript.tsx` `TurnItems`/`FoldedTools`): every run of one or more non-error, non-denied, non-subagent tool rows folds into one quiet line, while the turn is open and after it ends.
  - The line keeps today's label form, "Used <n> tools · edited <m> files": the frozen `chat/chat.contracts.test.tsx` expects exactly "Used 3 tools · edited 2 files" for Edit, Edit, Read. While a row in the run is still running, the line ends with the activity glyph.
  - Folded rows stay in the `<details>` DOM, as today: the frozen "chat expanded tool row" test reads them there.
  - Errors, denials and permission cards stay outside, as now. The frozen contract "chat ended turn folds three consecutive tool rows and keeps an error row outside" must hold.
  - Add a unit test that a running turn folds a single tool row.
- Remove `text-[13px]` from the transcript root in `transcript.tsx`; V2 sets the transcript type sizes in `chat-transcript.css`.
- Doubled user message: T1 reports whether the Rust side duplicates. If it does not, fix the browser side.
  - Make the in-flight guard of `new/proposal.tsx` `useProposalActions` per store (a `WeakSet` per store, as `composer-hooks.ts` does for Send), so `ShapeSize` (`new/shape-size.tsx`, which also calls `useProposalActions`) and `Proposal` cannot both fire.
  - Check `chat/model.ts` `unconfirmedSendsAtom`/`shownCounts` for an echo that survives the delivered item.
  - Name your regression test `it("one click on a shape choice sends one message", …)` in `new/proposal.test.tsx`. Show it failing against the unfixed guard before you fix it. If it passes there, the cause is not found: say so in your report.

## Context and selections (D7, D8)

- Context: V2 moves the context line into the composer. Its behaviour (`chat/viewed-document.ts`, `screen-sync.tsx`, dismiss, the attachment on send) stays as the frozen `chat/context.contracts.test.tsx` pins it.
- `chat/chips.tsx` `chipLabel`: its input `LabelInput` is a `Pick` of `Attachment` with no quote today, and the stage contract calls `chipLabel(toAttachmentInput(draft))`.
  - `LabelInput` gains `quote`. A summary or source selection reads as its quoted start, `“<first 48 characters of the quote>…”`, with whitespace collapsed.
  - A new `chipSource(input)` gives the secondary text: `section_04 summary`, or `section_04.md › Heading · lines 3–9`.
  - A whole-document attachment keeps its file name. A file chip reads its `file.name`.
  - `chat/suggestion-cards.tsx` also calls `chipLabel`: pass `quote: suggestion.passage.text`. `SentChips` passes the chip's `quote`.
  - Update `chat/chips.test.tsx` where it pins the old labels. `chips.tsx` is 367 lines: put the file chip in a new `chat/file-chip.tsx`.
- Add to chat opens the chat. `attachments/add-to-chat.tsx`, after `add(draft)`: set `panelCollapsedAtom` to false at the wide layout, or `chatSheetOpenAtom` to true below it, as `chat/start-chat.tsx:33-34` does.

## File attachments (D9)

The wire shapes are pinned in `u1-uploads.md` "Wire shapes". Build to them exactly; U1 builds the Rust side at the same time.

- `web/src/plan/api/schema-chat.ts`: the `file` attachment input `{kind: "file", id, name}`. A transcript chip of kind `file` (`path: ""` and `file: {id, name, media_type, bytes}`), parsed from the delivered block.
- Fixtures: add a `file` input example to `web/src/plan/api/fixtures/chat-commands.json`, and to `chat-deltas.json` an `item_added` user item carrying one file chip in U1's shape. U1's Rust parity tests and `schema-chat.test.ts` both read them.
- Draft types: `AttachmentDraft` (`attachments/types.ts`) stays as it is, because `concept/selection.ts` (reading stage) and `review/send-to-author.tsx` (home stage) read its `text`, `path`, `anchor` and `lines`. File chips get their own `FileDraft {kind: "file", id, name, media_type, bytes}`; the chat's chip list holds `AttachmentDraft | FileDraft`; `toAttachmentInput` gains a `FileDraft` overload returning exactly `{kind, id, name}`.
- `web/src/plan/chat/drafts.ts` (the saved chip schema, a strict union of source and summary today) gains the file arm, so file chips survive a remount; update `drafts.test.ts`.
- `web/src/plan/attachments/upload.ts` (new):
  - `uploadAttachment(file: File): Promise<{id, name, media_type, bytes}>` POSTs the raw body to `/api/attachments?name=${encodeURIComponent(file.name)}`, with `Content-Type: file.type`. There is no filename header.
  - An empty `file.type` (common for `.md`) is taken from the extension: `.md` is `text/markdown`, `.txt` is `text/plain`; anything else with an empty type is refused.
  - It refuses before sending a type outside D9's list, or a file over 10 MiB, with a visible reason.
  - It renders the server's 413, 415, 422 and 507 reasons through `<Untrusted>`.
- `web/src/plan/chat/attach-document.tsx` (the paperclip) becomes an Attach menu: "A document in this project" (today's `DocumentStarter`) and "A file from this computer…" (a hidden `<input type="file" multiple accept=…>`).
- The composer accepts files dropped on it and images pasted into it (`paste` with `clipboardData.files`). Export `useFileIntake(): { onDrop, onDragOver, onDragLeave, onPaste, dragging }` from `chat/composer-hooks.ts`; V2 spreads it on the composer box in `composer.tsx` (a plan wiring check greps `useFileIntake(` there) and draws the drop target from `dragging`.
- `web/src/plan/new/document-starter.tsx`: the /new "Attach notes or a document" gets the same two choices and calls `uploadAttachment(` itself (a plan wiring check). The kickoff stage later restyles /new; keep your change confined to the attach choice.
- File chips show the name and size. An image chip shows a thumbnail through `URL.createObjectURL` from the local `File`, revoked on removal. Nothing is fetched from the server to show it.
- Limits: an upload counts toward the 10-attachment cap (frozen contract "chat eleventh attachment is refused with a note"). Chips survive a remount through local storage as today, keeping `{id, name, media_type, bytes}` and never the bytes.

## Tests

- The frozen stage contracts `kickoff-fence-hidden`, `selection-chips-differ` and `file-attachment-is-sent-by-id`.
- Unit tests for: the fold label counts; the Skill drop; `uploadAttachment` refusals; paste and drop; your doubled-message test.

## Run once at the end

`node_modules/.bin/vitest run src/plan/chat src/plan/attachments src/plan/new/document-starter.test.tsx src/plan/new/proposal.test.tsx src/plan/api` from `web/`.
