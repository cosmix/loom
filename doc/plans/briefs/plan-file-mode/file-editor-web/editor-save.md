# file-editor-web / editor-save: Edit, Save, conflicts, validation panel, Add to chat in the editor

Tier: sonnet, Claude lane. Wave 1, after the Claude `codemirror-core` worker and codex wave 0. Read
`doc/plans/briefs/plan-file-mode/common.md` first. Spec: `doc/product/section_06.md` "Viewing and
editing", "The editor", "What the server does on save", "When the file changed underneath",
"Plan validation".

## Files you own

`web/src/plan/editor/{edit-mode.tsx, validation-panel.tsx, merge-panel.tsx}` and their tests
(`edit-mode.test.tsx`, `validation-panel.test.tsx`, `merge-panel.test.tsx`). Codex units own
`request-edit.ts` (E4), `leave-guard.ts` (E5), `editor-provenance.ts` (E6) and `save-flow.ts` (E9,
the save state reducer), each with its test; their specs are the matching sections below; use
them, never edit them. Also yours: `web/src/plan/files/doc-header.tsx` (add the Edit / Done controls only; it is
`tree-search`'s file from the previous stage); `web/src/plan/files/reading-column.tsx` (swap in
`EditMode` while editing, only); `web/src/plan/help-keys.ts` (append the editor group only).

## Behaviour

- Edit control shown only when `DocView.editable` and `canAuthorAtom` (plan 2); clicking it swaps
  the rendered view for `EditMode` with the full source; Done returns, asking to save or discard
  when there is unsaved text. Leaving the document (navigation or `beforeunload`) with unsaved
  text asks first (`leave-guard.ts`).
- Drafts: on every change `saveDraft(path, openedHash, text)`; a `storage_full`, `too_large` or
  `unavailable` result shows a persistent note "Unsaved text is not being kept: browser storage is
  full" (or "... this browser does not allow storage") under the editor until a write succeeds.
  On entering edit mode, `findDrafts(path)`: a draft for the current hash is restored with the note
  "Restored unsaved text"; a draft for an older hash opens the merge panel at once (left: current
  text, right: the draft, base: the current hash), as `requestEdit` does. A successful save, a
  Discard, or a merge saved successfully calls `clearDrafts(path)`.
- Losing authoring while editing: when `canAuthorAtom` turns false (another browser took it, or
  this one was away past the grace period), the editor turns read-only, Save is a
  `<DisabledReason reason={authorBlockReasonAtom}>` (plan 2, `@/plan/ui/disabled-reason`), and a
  line says "<holder> took authoring. Your text is kept as a draft."; regaining authoring makes it
  editable again with the text intact.
- The open file renamed or deleted while editing (`tree_changed` without the path, or
  `doc_changed` with `hash: null`): the editor keeps its text in a read-only panel "This file was
  renamed to <new path> / deleted; your text is kept" with Copy and Discard (Discard clears the
  drafts); no save is attempted (saves never create files).
- Save (button, Ctrl-S / Cmd-S) -> `saveDoc(path, text, openedHash, false)` (`@/plan/files/api`):
  `saved` -> new opened hash, back to clean, show validation if any; `conflict` -> `MergePanel`
  with `createMergeView` (left: `current_text` read-only, right: the user's text; base hash becomes
  `current_hash`, so the next save succeeds unless the file changes again); `invalid` ->
  `ValidationPanel` lists the errors with a "Save with errors" button that calls
  `saveDoc(.., true)`; an error result shows the server's message inline (for example "Another
  browser holds authoring").
- The merge view is also offered, without waiting for a save, when `doc_changed` reports a new
  hash for the open path while editing (`openDocIsStaleAtom`): as a non-modal line above the editor,
  "Changed on disk · Compare", that takes no focus; Compare opens the merge panel. A second conflict
  while in the merge panel re-opens it with the right side as currently merged (never the original
  text) and the newest current text on the left.
- Esc acts as Done (asking about unsaved text) only when CodeMirror's search panel is closed
  (the editor handle's `searchOpen()`, from `codemirror-core`); with the panel open, Esc closes it as CodeMirror does.
- `ValidationPanel` above the source: errors, warnings by bucket, notes, as `loom plan verify`
  groups them; an item with `line` marks that line (`gutterMarks`) and clicking scrolls to it.
  Validation runs server-side only (on save and on watcher batches); no client-side check.
- Add to chat in the editor: `editor-provenance.ts` exports `editorProvenance(path, sections, stages)`, a provenance callback for CodeMirror
  selections (exported: plan 9 reuses it): path, the anchor of the section containing the selection start (from
  `DocView.sections` by line), the source text, `lines` 1-based inclusive, `stageId` when the
  start line is inside a stage span. Use plan 3's `useSelectionAttachment` and `AddToChat`.
- Entry point for other plans (binding; plan 9's Restore fallback uses it):
  `request-edit.ts` exports `requestEditAtom: { path: string; text: string; openedHash: string } | null`
  and `requestEdit(store, req): string` (sets the atom and returns the address
  `/files/<path>?edit=1` to navigate to). `ReadingColumn` with `?edit=1` enters edit mode; `EditMode`
  consumes the atom on mount: the editor text is `req.text`, and when `req.openedHash` differs from
  the document's current hash the merge panel opens at once (left: current text, right: `req.text`,
  base hash: the current hash), exactly as after a save conflict.
- Help keys: append `{ title: "Editor", rows: [["Ctrl + S / ⌘ S", "Save"], ["Esc", "Leave the editor (closes the search panel first)"]] }`
  (match `HelpShortcutGroup`'s fields).

## Tests

conflict opens the merge panel with both texts and the next save sends `current_hash`; invalid
shows the panel and "Save with errors" sends `with_errors=1`; a restored draft for the same hash;
an older-hash draft is never loaded as the editor text and opens the merge panel with the draft on the right;
a quota failure shows the not-kept note; losing authoring makes the editor read-only with the
reason as Save's accessible description, regaining it restores editing; a deleted open file shows
the kept-text panel; a second conflict keeps the merged right side; the watcher offer is a line
that does not move focus; Esc with the search panel open closes only the panel, then a second
Esc acts as Done; Edit hidden for a locked doc and for a non-holder; `requestEdit` with a stale opened hash opens the
merge panel with the given text on the right; leave guard fires
with unsaved text; Done with unsaved text asks to save or discard; Ctrl-S sends path, opened hash
and text; a `doc_changed` for the open path while editing offers the merge view; a validation item
with a line marks that line; editor provenance gives lines and stage id. Mock `loadCodeMirror`.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/editor src/plan/files` pass.
