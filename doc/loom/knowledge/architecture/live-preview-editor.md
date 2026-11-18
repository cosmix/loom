# Live Preview Editor

> Editor

## Modes and ownership

The file-mode editor (`web/src/plan/editor/`) has two modes over one CodeMirror document: live preview (default) and source. `createSourceEditor` (`codemirror.ts`) puts the mode in `modeCompartment`; `setMode` is one `reconfigure`, so text, selection and undo history survive a toggle. `modeExtension` returns `lineNumbers()` for source and `livePreview(...)` plus the `cm-live-preview` editor class for preview, so line numbers belong to Source only. `baseExtensions` holds no line numbers; the merge and diff views add them through `withLineNumbers` and always show source.

`editor-mode.ts` remembers the mode per project under `storageKey(project_key, "editor-mode")`: `readEditorMode` answers `preview` for no project key, blocked storage or an unknown value, and `writeEditorMode` writes nothing in those cases. `edit-body.tsx` holds the mode state and writes it on every toggle. The toolbar (`edit-toolbar-controls.tsx` `FormatButtons`) calls `SourceEditorHandle.format(command)`, which runs a `FormatCommand` from `md-commands.ts` and refocuses the view.

## Live preview decorations

`live-preview.ts` exports `livePreview(options)`: `inlinePreview` (a `ViewPlugin`), `fenceField` (a `StateField`) and `followLinks`. It draws over the Markdown source and changes the document only when a task box is clicked (`toggleTask`), so an untouched save writes the bytes the editor holds.

- **Reveal rule.** `hideRun` hides a marker run (emphasis stars, heading hashes, link brackets and destination) unless a selection range touches that line (`touches`, inclusive at both ends); the cursor line shows its raw source. A run never spans lines. Decorations rebuild on a doc, selection or viewport change and when the syntax tree grows (the parse is incremental, so a long document decorates as it parses).
- **Widgets.** `SpanWidget` draws the bullet, the quote bar and the fence's language label; `CheckboxWidget` draws a task box that carries its own click listener (widget DOM ignores editor events) and cancels the browser's toggle, so a read-only editor keeps the document's state.
- **Block styling.** Heading lines take `cm-lp-h1..6`, table lines `cm-lp-mono` with dimmed pipes, fenced code `cm-lp-code[-first|-last]` lines. The closing fence is hidden from the previous line's end (`[prevLine.to, closeLine.to]`), not through its own break: a replace that merges lines keeps only the first line's line decorations, so hiding through the break made the next line (a heading) inherit the code ground.
- **Mod-click.** `followLinks` calls `onFollowLink(href)` for Ctrl or Cmd plus a primary-button click on a `Link` or `Autolink` URL (an `Image` is skipped). `hrefAt` returns null for a `.cm-line` or `.cm-content` target, because a browser targets the line element for unstyled text, and resolves the syntax node at side 1 only, so text that starts where a link ends never reaches back into it. `destination` strips `<...>` and backslash escapes the way the reading view parses them. `useFollowLink` (`edit-body.tsx`) sends the href through `resolveLink`: a `doc` or `anchor` target navigates with `toAddress`, an `external` one opens a new tab with `noopener,noreferrer`, and `outside` or `text` does nothing ([markdown rules](../conventions/plan-web.md#markdown-renderer-rules)).
- **Typography.** `theme.ts` sets `fontVariantLigatures: "none"` for source and preview alike: Inter's `calt` drew `-->` of an HTML comment as an arrow.
- **Language.** `markdown({ base: markdownLanguage, codeLanguages })` (GFM: tasks, strikethrough, tables, autolinks). The merge and diff views share the base, so `TableHeader` highlights as a heading and `TaskMarker` as an atom there.

## Formatting commands

`md-commands.ts` (`inline`, `togglePrefix`, `link`, `quote`, `heading`, `codeBlock`, `table`, `formatCommand`, `formatKeymap`, `saveKeymap`) edits the selection as one transaction, so one undo reverses it. Every formatting command returns false in a read-only state; `formatKeymap` binds Mod-b, Mod-i and Mod-k and returns true even when the command did nothing, so the browser's own Mod-k and the default keymap's Mod-i never run. `saveKeymap` (Mod-s to `onSave`) lives in this file only to keep `codemirror.ts` under 400 lines and does not refuse read-only. `heading` cycles none, 1, 2, 3, none.

## Lazy loading

`EditMode` is a `React.lazy` chunk (`web/src/plan/files/reading-column.tsx`) inside a `Suspense` whose fallback is the "Loading editor" `role="status"` line, the pattern help uses. The chunk is `edit-mode.js` (about 21 kB); `plan.js` stays near 412 kB, under the 500 kB warning ([the bundle rule](../conventions/plan-web.md#live-preview-editor-rules)). A failed `edit-mode.js` fetch is not retried ([concerns](../concerns/planner-ui-repair-followups.md#live-preview-editor-reviewer-suggestions)).

## Failure and validation surfaces

SourceEditor and MergePanel show Aurora ErrorPanel inside role=alert when the lazy editor cannot load, retaining reload guidance. ValidationPanel groups errors into an error HazardPanel, and warning buckets into one warning HazardPanel; notes stay outside the hazards. Source-line jump buttons and the explicitly offered Save with errors action retain their behavior. In file mode, plan/validation-list.tsx applies the same grouping to the document heading validation popover, preserving stage links and location text. plan/stages-panel.tsx wraps unreadable YAML in ErrorPanel and keeps the raw metadata visible; reading-column.tsx wraps a failed document refresh in a warning HazardPanel with Retry and keeps the earlier document text visible. Stripes stay in the header; body text remains neutral. The backend unavailable modal is documented in planner-web-shell.md#backend-unavailable-modal.
