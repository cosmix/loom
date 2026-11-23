# web-frontend / w1-editor: save races, per-tab drafts, stale reading view, codemirror split

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/web.md`, sections U1, U2, U3, CR-65 (second half only), CR-78.f.

All paths below are relative to `web/` unless they start with `doc/`. Run commands from `web/`.

## Findings covered

- U1: "Save and close" leaves the editor when a newer edit was typed while the save was in flight, so the newer text leaves the live session.
- U2: an `invalid` verdict for an older submission is kept as the current status; "Save with errors" then force-saves newer, never validated text.
- U3: two tabs editing one document share one recovery copy per base hash; a save or discard in one tab deletes the other tab's copy.
- CR-65 second half: after a save the reading view keeps the pre-edit text until the file watcher pushes `doc_changed`.
- CR-78.f: `src/plan/editor/codemirror.ts` is 403 lines (limit 400).

## Files

Own (write):
- `src/plan/editor/edit-session.ts`
- `src/plan/editor/save-flow.ts`
- `src/plan/editor/drafts.ts`
- `src/plan/editor/edit-flow.ts`
- `src/plan/files/api.ts`
- `src/plan/editor/codemirror.ts`
- `src/plan/editor/view-handle.ts` (new)
- `src/plan/editor/edit-mode.test.tsx`
- `src/plan/editor/save-flow.test.ts`
- `src/plan/editor/drafts.test.ts`

Read only: `src/plan/editor/draft-writer.ts` (needs no change, see 3), `src/plan/editor/edit-body.tsx`, `src/plan/editor/editor.contracts.test.tsx`, `src/plan/id.ts`.
Frozen (contract tests, never edit): `src/plan/editor/edit-save.contract.test.tsx`, `src/plan/editor/save-flow.contract.test.ts`, `src/plan/editor/drafts.contract.test.ts`.

## Changes

### 1. U1: `edit-session.ts`, `useLeaving`

Today the `closing` effect (the `useEffect` after the `leaveTo` effect) leaves whenever `state.status === "saved"`. The reducer is correct: after `saved`, `state.text` holds anything typed since and `state.savedText` holds only the submitted text.
Change the effect body to:

```text
if (!closing || state.status === "saving") return;
if (state.status === "saved" && state.text === state.savedText) setLeaveTo(toAddress(doc.path));
else setClosing(false);
```

Add `state.text` and `state.savedText` to the dependency array. Do not use `dirty` (it is computed after this hook). Do not touch `save-flow.ts` for this. After the change a saved-with-newer-text result keeps the user in the editor with the newer text and `dirty` true, so Done and the leave guards still ask. No automatic re-save.

### 2. U2: `save-flow.ts`, `saveResult`, case `"invalid"`

Today it returns `{ ...state, status: "invalid", validation: value.validation, submittedText: null }` whatever the current text is. Change it: when `state.submittedText !== null && state.text !== state.submittedText`, return `{ ...state, status: "idle", validation: null, submittedText: null }` (drop the verdict; the panel falls back to the watched validation). Otherwise keep the existing return. Leave the `saved` and `conflict` cases alone. Do not touch `edit-flow.ts` or any server file for this.

### 3. U3: `drafts.ts`, per-tab copies

Decision: a module constant `TAB_ID = newId()` (import `newId` from `@/plan/id`; do not call `crypto.randomUUID` directly, it exists only in secure contexts and the dashboard is served over plain HTTP on LAN binds). Created once per page load.

- `keyFor(project, path, hash, editorId)` returns `${PREFIX}${project}:${path}:${hash}:${editorId}`.
- `StoredDraft` gains `editorId: string`. `readEntries` must require `typeof draft.editorId === "string"` and compare `key` against `keyFor(..., draft.editorId)`. No migration: a stored entry in the old key shape fails the check and is skipped (the project is pre-release).
- `saveDraft(docPath, openedHash, text, editorId = TAB_ID)`: key and stored value use `editorId`. The eviction logic (`MAX_TOTAL`, quota loop) stays as it is.
- `clearDrafts(path, editorId = TAB_ID)`: removes only entries of that project and path whose `draft.editorId === editorId`.
- `findDrafts(path)` still returns every tab's drafts for the path, newest first, in the unchanged `Draft` shape (`openedHash`, `text`, `savedAt`; do not add `editorId` to `Draft`), so a reopened tab recovers any tab's text. `loadDraft` is unchanged.
- The default parameters mean `draft-writer.ts`, `edit-flow.ts` and every existing caller need no edit for this item. Do not export `TAB_ID`.

Add one line to the comment above `loadDraft` saying orphan copies of closed tabs stay until the 2 MiB eviction trims them.

### 4. CR-65 second half: `files/api.ts` and `edit-flow.ts`

- `src/plan/files/api.ts`: export `markChanged` (currently the file-private `const markChanged = (store, path, hash)`); change only the `const` to `export const`.
- `src/plan/editor/edit-flow.ts`: `useEditFlow` already has `store` (`useStore()`). Pass it to `useFlowActions` as a new parameter (add `store: Store` after `path`; `Store` is already imported as a type). In `save`'s `.then`, after `dispatch({ type: "result", result })`, add: `if (result.ok && result.value.result === "saved") markChanged(store, path, result.value.hash);`. Add `store` to the `useCallback` dependency array. Import `markChanged` next to `fetchDoc, saveDoc`. This makes `openDocIsStaleAtom` true so `useStaleRefetch` refetches; it also fixes plain Ctrl-S saves that stay in edit mode.

### 5. CR-78.f: split `codemirror.ts`

Create `src/plan/editor/view-handle.ts` holding, moved verbatim: `replaceAll`, `scrollToLine`, `scrollToFirstChange`, `inDeletedChunk`, `mapRange`, `rangeLines`, and `viewHandle` (the block from the `replaceAll` function through the end of `viewHandle`, with its comments). `editableSide` (the Compartment just above `replaceAll`) stays in `codemirror.ts`. Export only `viewHandle`.

- `view-handle.ts` imports `getChunks` from `@codemirror/merge`, `searchPanelOpen` from `@codemirror/search`, `EditorView` from `@codemirror/view`, and `import type { EditorHandle, LineSpan } from "./codemirror"` (a type-only import is erased, so there is no runtime cycle).
- `codemirror.ts` imports `{ viewHandle }` from `./view-handle`; remove the imports `getChunks` and `searchPanelOpen` it no longer uses (keep `MergeView`, `unifiedMergeView`, `search`, `searchKeymap`).
- Update the file's top comment: the lazy chunk now also covers `view-handle`.
- Nothing else may import `view-handle.ts`: it must stay reachable only through `codemirror.ts`, which is dynamic-import-only (`src/plan/editor/load.ts`). The target is `codemirror.ts` under 340 lines.

## Interfaces shared with other workers

- `markChanged(store: Store, path: string, hash: string | null): void` exported from `src/plan/files/api.ts` (only `edit-flow.ts` uses it).
- Contract-visible signatures: `saveDraft(docPath: string, openedHash: string, text: string, editorId?: string): SaveDraftResult`; `clearDrafts(path: string, editorId?: string): void`; `findDrafts(path: string): Draft[]` unchanged; `saveFlow` and `initialSaveFlow` unchanged.

## Tests

Existing helpers: `edit-mode.test.tsx` has `renderEdit()` (returns the router), `source()`, the hoisted `fake` editor whose textarea `input` event drives `onChange`, and `beforeEach` seeding `docChangedAtom` to `{}`. A fetch reply is `{ ok, status, json: () => Promise.resolve(body) }` (see `answer()` in `editor.contracts.test.tsx`). `files-save-saved.json` is not needed; build the saved body inline: `{ result: "saved", hash: "sha256:new", validation: null }`.

1. U1, in `edit-mode.test.tsx`, new `describe("EditMode saving", ...)` at the end of the file: stub `fetch` with `vi.fn(() => new Promise(...))` and keep its resolver. `fireEvent.input(editor, { target: { value: "first edit" } })`, click Done, click "Save and close", then `fireEvent.input(editor, { target: { value: "second edit" } })`, then resolve the save with the saved body. Assert `router.state.location.search` is still `?edit=1` and the textarea value is `second edit`. Fails today (router leaves the edit page). Add the control: with no typing after the click, the router leaves (`search` becomes `""`) - an existing test already covers Discard, so write this control with Save and close.
2. U2, in `save-flow.test.ts`, inside `describe("saveFlow edits after a verdict")`: `change` -> `save` -> `change` (newer) -> `result` with `invalid` (the `invalid` const already there): expect `status: "idle"` and `validation: null`. Fails today (`"invalid"`). Control: unchanged text still ends `"invalid"`.
3. U3, in `drafts.test.ts`, inside `describe("draft storage")`: `saveDraft("a.md", "h1", "tab one", "tab-1")` and `saveDraft("a.md", "h1", "tab two", "tab-2")` (mock `Date.now` so `savedAt` differs); `findDrafts("a.md")` lists both texts newest first; `clearDrafts("a.md", "tab-1")`; the remaining list is only "tab two"; `clearDrafts("a.md", "tab-2")` empties it. Fails today (the second save overwrites the first).
4. CR-65, in the same `describe("EditMode saving")`: stub `fetch` to resolve at once with the saved body, edit, press Ctrl-S on the fake editor (`fireEvent.keyDown(editor, { key: "s", ctrlKey: true })`), then `await waitFor(() => expect(store.get(docChangedAtom)[doc.path]).toBe("sha256:new"))`. Fails today (stays `{}`).
5. CR-78.f: no new test; `src/plan/editor/codemirror.test.ts` and `live-preview*.test.ts` must still pass unchanged.

`edit-mode.test.tsx` is 348 lines; the two new tests together must leave it at 398 lines or fewer after `oxfmt`. Keep them short; if they do not fit, put them in a new file `src/plan/editor/edit-saving.test.tsx` with a trimmed copy of the `fake` harness limited to `createSourceEditor`.

Contracts to turn green: `save-and-close-keeps-newer-text`, `stale-invalid-verdict-never-forces`, `draft-copies-per-tab`.

## Proof

```text
node_modules/.bin/vitest run src/plan/editor src/plan/files
node_modules/.bin/tsc -b --noEmit
node_modules/.bin/oxlint --deny-warnings
node_modules/.bin/oxfmt --check src/plan/editor src/plan/files
```

Do not run `vite build`: the main agent builds `dist/` once after every worker is done. Report that you saw tests 1 to 4 fail before and pass after.

## Traps

- `startState` in `edit-flow.ts` matches a same-hash draft by `openedHash` only; with per-tab keys, a second tab opening the same base hash restores the first tab's newest text. That is intended; do not filter by tab id in `findDrafts`.
- `clearDrafts` is also called from the `state.clearDrafts` effect (after a clean save) and from `discard`; both now clear only this tab's copies. Do not add an `editorId` argument at those call sites.
- U1: the effect must call `setClosing(false)` in the newer-text case, or the next save-and-close in the same session leaves immediately on a stale `closing` flag.
- U2 compares `state.text` with `state.submittedText` at the time the result arrives; `submittedText` is set by the `save` event and cleared by every result, so test the sequence in order.
- The `drafts.ts` key check in `readEntries` (`key !== keyFor(...)`) is what rejects hand-edited entries; keep it, with the new `editorId` term.
- Moving code out of `codemirror.ts` must not change behaviour: the `viewHandle` default `others = []` and the `rangeLines` side logic are exercised by `codemirror.test.ts`.
- `edit-flow.ts` already imports from `@/plan/files/api`; add `markChanged` to that existing import line instead of a second import.
