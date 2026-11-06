# Stage brief: live-preview

Read `doc/plans/feat-planner/briefs/feat-planner-14-live-preview-editor/common.md` first. It
holds the requirement, the frozen interface and the behaviour settled for this plan. This file
splits the work between two workers and says what the main agent does after them.

Each worker, and every fix worker for a visual defect, invokes `frontend-design:frontend-design`
before writing code (`common.md`, "Visual design").

Both workers start at once. The interface in `common.md` is frozen, so W2 codes against it while
W1 implements it. Neither worker edits a file the other owns.

## W1 — the CodeMirror chunk

Owns `web/src/plan/editor/codemirror.ts`, `web/src/plan/editor/live-preview.ts` (new),
`web/src/plan/editor/md-commands.ts` (new), `web/src/plan/editor/marks-gutter.ts` (new),
`web/src/plan/editor/line-break.ts` (new), `web/src/plan/editor/line-break.test.ts` (new),
`web/src/plan/editor/theme.ts`,
`web/src/plan/editor/codemirror.test.ts`, `web/src/plan/editor/live-preview.test.ts` (new),
`web/src/plan/editor/md-commands.test.ts` (new), `web/src/plan/editor/theme.test.ts`.

1. In `codemirror.ts`: add the interface types from `common.md` (including `onFollowLink` on
   `SourceEditorOptions`); switch the Markdown base to `markdownLanguage`; move `lineNumbers()`
   out of `baseExtensions` and add it back at the five merge and diff call sites (`common.md`
   names them), while the source editor holds it in a `mode` compartment with
   `livePreview({ onFollowLink })` and the `cm-live-preview` editor attribute; the compartment
   sits before `marksGutter`. Implement `mode`, `setMode` (one `compartment.reconfigure`, never a
   new state), `format` (delegating to `formatCommand` in `md-commands.ts`; a no-op when
   read-only) and `rangeText` (in `viewHandle`, for every handle, as `common.md` specifies).
   Fix the line endings as `common.md` "Line endings" says: `line-break.ts`, the
   `lineSeparator` facet in `createSourceEditor`, and `state.sliceDoc()` for `getText`, the
   `onChange` listener and `rangeText` (no `doc.toString()` remains in the file).
   Install `formatKeymap()` from `md-commands.ts` in `createSourceEditor` beside `saveKeymap`,
   before `baseExtensions`. Move `MarkedLine`, `marksField`, `marksGutter` and the
   `setGutterMarks` effect into `marks-gutter.ts`. Update the header comment (the chunk is
   `codemirror.ts` and the modules it imports). Every file under `src/plan/editor/` stays at 400
   lines or fewer; a stage acceptance command counts them.
2. `live-preview.ts`: a `ViewPlugin` that builds decorations from `syntaxTree` over the visible
   ranges, rebuilt on doc, selection and viewport changes and when the syntax tree changes
   (`common.md`, "Rebuild triggers"). It provides marker hiding, the bullet,
   quote-bar and checkbox widgets, heading line classes, comment dimming, the code block ground,
   fence hiding and language label, and table dimming, as `common.md` specifies. Links hide
   the run `common.md` "Links" gives, titles included; the checkbox replaces `TaskMarker` only
   and the item label stays visible. Whole-line
   replacements (the fence lines) span line breaks, which a `ViewPlugin` may not provide; put
   them in a `StateField` that provides `EditorView.decorations` and rebuilds on doc and
   selection changes and when the syntax tree changes; build its set with
   `Decoration.set(ranges, true)` so the ranges are sorted, and test an empty-body fence, whose
   two replaced lines are adjacent. The checkbox click handler dispatches the toggle unless the
   state is read-only. Mod-click on a link calls the `onFollowLink` option. It exports
   `livePreview(options: { onFollowLink?(href: string): void }): Extension`.
3. `md-commands.ts`: one exported function per `FormatCommand`, each a CodeMirror `StateCommand`
   that dispatches one transaction and returns false without dispatching when the state is
   read-only, plus a lookup `formatCommand(name)` and `formatKeymap()` (Mod-b, Mod-i, Mod-k; each
   binding returns true even when the command did nothing, `common.md` "Shortcuts").
4. `theme.ts`: rules for the live preview classes, design-token colours only. `theme.test.ts`
   checks that every colour is a token variable; keep it passing.
5. Tests: unit tests for each command (wrap, unwrap, empty selection, multi-line prefixes,
   heading cycling, one-step undo, read-only no-op), for the decorations (which marks hide off the
   cursor line, including the space after `#`; a link drawn as its text; bullet and quote-bar
   widgets; an ordered list keeping its number; the heading line classes; nothing hidden inside a
   code block's body, a table, an HTML block or a comment; fence lines hidden while the cursor is
   outside the block and shown when it enters; the language label; table pipes dimmed; a fence
   placed after 3000 characters of text decorated once the parse finishes, using
   `forceParsing` from `@codemirror/language`, asserting both an inline mark and a fence below the
   3000th character with the document and selection unchanged, and confirming the assertion fails
   when the syntax-tree trigger is removed), a titled link, `line-break.ts` (no `\r`, all CRLF,
   all CR, mixed), line numbers (`.cm-lineNumbers` present in
   Source, absent in live preview, present in the merge view and both diff layouts), the
   validation gutter marks showing in both modes, Mod-b/Mod-i/Mod-k in both modes (Mod-i must
   not run `selectParentSyntax`), Mod-click following a link and a plain click not following
   it, and read-only behaviour (no format, no task toggle, mode toggle still works).
   Use the real-CodeMirror setup in `codemirror.test.ts` (its `beforeAll` at lines 14-18 patches
   `Range.prototype.getBoundingClientRect` and `getClientRects`; the patch is inline and not
   exported, so a new test file copies those lines, as six other test files already do; never
   import a `.test.ts` file).

## W2 — the React side

Owns `web/src/plan/editor/source-editor.tsx`, `web/src/plan/editor/edit-panels.tsx`,
`web/src/plan/editor/edit-body.tsx`, `web/src/plan/editor/editor-mode.ts` (new),
`web/src/plan/editor/editor-mode.test.ts` (new), `web/src/plan/editor/selection-attachment.ts`,
`web/src/plan/editor/editor-provenance.ts`, `web/src/plan/editor/editor-provenance.test.ts`,
`web/src/plan/editor/source-editor.test.tsx`, `web/src/plan/editor/edit-mode.test.tsx`,
`web/src/plan/editor/editor.contracts.test.tsx`, `web/src/plan/editor/merge-panel.test.tsx`,
`web/src/plan/editor/merge-panel.readonly.test.tsx`, `web/src/plan/editor/load.test.ts`,
`web/src/plan/editor/load.retry.test.ts`.

1. `editor-mode.ts`: `readEditorMode(): EditorMode` and `writeEditorMode(mode)`, using
   `storageKey(projectKey, "editor-mode")` with the project key read as `drafts.ts`
   `projectKey()` reads it (`common.md`). Every storage access is in try/catch; the default and
   the fallback are `preview`. It imports `EditorMode` with `import type`.
2. `source-editor.tsx`: accept `mode` and `onFollowLink` props. Create the editor with
   `mode` and `onFollowLink` read from the `latest` ref (never from storage). A change of the
   `mode` prop calls `handle.setMode` without rebuilding the editor (keep `mode` out of the build
   effect's dependencies, as `text` is kept out), and the sync effect does nothing when the
   handle's `mode()` already equals the prop, so no `setMode` runs on mount.
3. `edit-panels.tsx` `EditToolbar`: the segmented mode control and the formatting buttons from
   `common.md`. It takes `mode`, `onMode(mode)`, `onFormat(command)` and `formatDisabled`.
4. `edit-body.tsx`: hold the mode in state, initialised by `useState(() => readEditorMode())`
   (the wiring check greps `readEditorMode()`). On toggle, write it and pass it down. Route
   `onFormat` to the handle after narrowing the `EditorHandle` ref with `"format" in handle`;
   the buttons are disabled while the merge view is shown (`state.currentText !== null`) and
   when read-only. Resolve links with `resolveLink(` and `useNavigate()` as `common.md` says.
5. Attachments: `selection-attachment.ts` calls `handle.current?.rangeText?.(range)` on the same
   range it passes to `rangeLines` and hands it to the provenance as the third argument;
   `editor-provenance.ts` uses it as the text when it is a non-empty string, else
   `selection.toString()` (`common.md` gives the signature). `useSourceAttachment` in
   `edit-body.tsx` declares the third parameter and forwards it as
   `provenance(selection, lines, text)`; the hook's callback type gains `text?: string | null`.
   Extend `editor-provenance.test.ts` for both cases, and test that
   `useEditorSelectionAttachment` passes the `rangeText` result and that the real Add to chat path
   in `EditBody` carries it into the draft. `diff-provenance.ts` is not edited.
6. Update every fake handle in the owned tests to the widened interface without weakening an
   assertion. The fake in `edit-mode.test.tsx` (lines 21-37) lacks `rangeLines`, `rangeText`,
   `mode`, `setMode` and `format`; typecheck of the tests is the constraint.
7. Migrate the two lazy-boundary scans, `load.test.ts` and `editor.contracts.test.tsx`, to the
   replacement invariant in `common.md` ("Where the code is"): the named allow-list of the four
   chunk modules, the retained negative and positive checks, and the new no-import-of-the-helpers
   assertion in `load.test.ts`. This is the first thing W2 does, because the rest of the stage's
   gate cannot pass without it. Nothing else in those tests changes.

## Contracts

The contract session writes the three contract files named in the plan's YAML before the workers
start; the third, `src/plan/editor/editor-bytes.contracts.test.tsx`, uses the real CodeMirror
chunk through the real loader (no `./load` mock) with the same jsdom patches and renders
`EditMode` like `edit-mode.test.tsx`, with `fetch` stubbed to capture the save body. The tests in `src/plan/editor/live-preview.contracts.test.ts` use the real CodeMirror
module through `createSourceEditor`, with `codemirror.test.ts`'s jsdom patches, and read state
through `EditorView.findFromDOM`; the jsdom `Range` patch is copied inline from
`codemirror.test.ts` lines 14-18 (it is not exported). The tests in
`src/plan/editor/editor-mode.contracts.test.tsx` mock `./load` with a fake module whose
`createSourceEditor` records its calls and options and returns a handle that records `setMode`
calls and answers `mode()` with the last mode set. They clear `localStorage` and seed
`serverInfoAtom` with a `project_key` in `beforeEach` (jsdom storage persists across the tests of
one file, so the second contract would otherwise open in Source), and render the editor through
`EditMode` as `edit-mode.test.tsx` does, with an authoring browser. Each contract is a top-level
`it()` named exactly as its `test:` string. Workers must make the contracts pass and must not edit
them.

## Main agent, after the workers

1. Run the stage's acceptance commands once with `loom check live-preview`; send all failures to
   one fix worker.
2. Real-browser walk. `scripts/plan-screens.sh` only shoots routes and has no scripted step, so
   write a throwaway Playwright driver under `$LOOM_SCRATCH_DIR` that follows the script's server
   start (`doc/loom/knowledge/conventions/plan-web.md`, "Visual Stages and the Screenshot
   Script"): first `cd web && bun run build` and `cargo build` (the binary embeds `web/dist`);
   start `loom plan` on a scratch clone of `doc/` (`--worktree-doc` semantics) and drive the
   browser in the SAME Bash call (loopback is per call); import `playwright-core` by absolute
   path (`<worktree>/web/node_modules/playwright-core/index.mjs`) and launch Chromium with
   `args: ["--no-sandbox"]`. A stage sandbox has no Claude login, so the server answers
   read-only: if Edit is unavailable, say so, check what read-only allows (live preview drawn,
   the mode toggle, disabled formatting buttons) and report the editing steps as not run, never
   as passed. At 1440x900 in `ledger` (light) and `graphite` (dark), open a product section and a
   plan file in file mode, click Edit, type a word in bold with Mod-b, tick a task box if the
   document has one, check that the plan's YAML block shows its language label with the fences
   hidden until the cursor enters it and that a table shows dimmed pipes, toggle to Source and
   back, then Done without saving. At 390x844, check that the toolbar wraps without a sideways
   scroll. Save screenshots of both modes under `$LOOM_SCRATCH_DIR`. Then restore the build
   output before the review round: `git restore web/dist`, `git clean -fdq -- web/dist`, and
   confirm `git status --untracked-files=all web/dist` is empty (`web/dist` is outside this
   stage's `files`; integration-verify rebuilds and commits it). Record what you saw in
   `loom memory note`. A defect goes to a fix worker that invokes frontend-design first; after
   its fix, rebuild web and the binary and repeat the affected steps against that build (the walk
   always exercises the current source, never a bundle older than the last edit).
3. Spawn one `loom-code-reviewer` over the diff with this brief and `common.md`. Ask it to check
   the byte-exact rule, the read-only paths, the storage try/catch, the single source of truth for
   the mode, and that nothing outside the four chunk modules imports `@codemirror/*` or `@lezer/*`
   statically (and W2's files use `import type` for `./codemirror`). Fix its findings through one
   worker.
