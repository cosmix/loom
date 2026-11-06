# Common brief: live preview editor

Every stage and every worker of `PLAN-feat-planner-14-live-preview-editor.md` reads this file
first.

## What the operator asked for

File mode's editor should be a proper Markdown editor: the user can edit text that looks
rendered, and switch to plain Markdown syntax. The operator chose a live preview drawn over the
source text, like Obsidian's editing mode. A rich-text editor that rebuilds Markdown from a document
tree on save was rejected, because it rewrites text the user did not touch. The behaviour is
specified in `doc/product/feat-planner/section_06.md`, "The editor" → "Live preview and source".
Read that subsection in full; it is the requirement.

## The rule nothing may break

**A save writes exactly the text in the editor.** Live preview only adds CodeMirror decorations
and widgets. It never dispatches a change except one the user asked for: typing, a toolbar
command, a shortcut, or a task-box click. Opening a file, switching modes, moving the cursor or
re-rendering must leave `state.doc` byte-identical. The contract
`toggling modes and saving unedited returns identical bytes` guards this.

## Where the code is

- `web/src/plan/editor/codemirror.ts` (380 lines) is the lazy chunk: the only module other code
  reaches CodeMirror through, via `loadCodeMirror()` in `web/src/plan/editor/load.ts`. Its header
  comment (lines 1-2) says it is the only module that statically imports `@codemirror/*`. This
  plan adds three sibling modules inside the chunk: `live-preview.ts`, `md-commands.ts` and
  `marks-gutter.ts` (the validation gutter, `MarkedLine`, `marksField` and `marksGutter` with the
  `setGutterMarks` effect, moved out of `codemirror.ts` so that file stays under 400 lines), plus
  the pure `line-break.ts` (see "Line endings"; it imports no CodeMirror). Only
  `codemirror.ts` imports them, and the header comment changes to say the chunk is
  `codemirror.ts` and the modules it imports. Nothing outside those four files may import
  `@codemirror/*` or `@lezer/*` statically, tests excepted. (`web/src/plan/diff/` and
  `web/src/plan/chat/line-diff.ts` import no CodeMirror values; they reach it through
  `loadCodeMirror()`.)
- Two existing tests scan for static CodeMirror imports and allow only `codemirror.ts`:
  `load.test.ts` (`planSources` filters `/codemirror.ts`; `appSources` for the `@lezer/` scan
  filters `/src/plan/editor/codemirror.ts`) and `editor.contracts.test.tsx` ("only the lazy loader
  imports codemirror", `offenders` filters `file !== loader`). Both fail as soon as a new chunk
  module imports `@codemirror/*` or `@lezer/*` (a type import counts). The replacement invariant:
  the chunk is exactly four files, `codemirror.ts`, `live-preview.ts`, `md-commands.ts` and
  `marks-gutter.ts`; only those four may import `@codemirror/*` or `@lezer/*` statically, and no
  non-test file outside the four may import `./live-preview`, `./md-commands` or `./marks-gutter`
  at all (value or type), so the helpers stay reachable only through the lazy `codemirror.ts`. W2
  owns this migration (it is a prerequisite of the module split, not a test exception for the
  worker to infer): each of the two scans takes one named allow-list of the four files, keeps its
  negative scan over every other file, keeps the positive checks that the scan sees `@codemirror`
  imports in the allow-listed files and that `load.ts` reaches `./codemirror` through a dynamic
  import, and `load.test.ts` gains one assertion for the no-import-of-helpers rule. Nothing else in
  those assertions changes; `bundle-check.mjs` (integration-verify) remains the bundle-level check.
- `web/scripts/bundle-rules.json` forbids `src/plan/editor/codemirror.ts`, the `cm-*` vendor chunks
  and `node_modules/@codemirror/**` in both entry bundles, so a leak of a new chunk module drags
  `@codemirror` in and fails the check. No rule change is needed; the file is not in this plan's
  `files`. W2's files import `EditorMode`, `FormatCommand` and the handle types from
  `./codemirror` with `import type` only (`tsconfig.app.json` sets `verbatimModuleSyntax`, so a
  plain `import { X }` stays a value import and pulls the chunk into the entry).
- `markdown({ codeLanguages })` in `baseExtensions` (`codemirror.ts`) uses the default base,
  `commonmarkLanguage`, which has no task-list, strikethrough or table nodes. Change it to
  `markdown({ base: markdownLanguage, codeLanguages })`. `markdownLanguage` comes from
  `@codemirror/lang-markdown` (6.5.2 installed) and is GFM plus Subscript, Superscript and Emoji.
  Do not add `@lezer/markdown` as a direct dependency; read node names through `syntaxTree` from
  `@codemirror/language`. The switch also reaches the merge and diff views, which share
  `baseExtensions`: `TableHeader` is tagged `heading` (header cells turn bold) and `TaskMarker`
  `atom` (`[ ]` takes the atom colour), and bare URLs become `Autolink`s. Those highlighting
  changes are accepted; nothing else in those views may change.
- `lineNumbers()` is in `baseExtensions` (`codemirror.ts` line 152), shared by the source editor,
  the merge view and the diff view. Line numbers move into the source editor's mode compartment
  (shown in Source mode only). The merge and diff views keep them: `lineNumbers()` is added back at
  every other `baseExtensions` call site, which are five (merge `a` "Version on disk", merge `b`
  "Your version", the unified diff "Changes", diff `a` "Before" and diff `b` "After"). No test
  pins line numbers today, so W1 adds one: `.cm-lineNumbers` present in Source, absent in live
  preview, present in the merge and both diff layouts. In the source editor the mode compartment
  comes before `marksGutter` in the extension list, so line numbers stay left of the validation
  dots.
- `web/src/plan/editor/source-editor.tsx` builds the editor once per path and read-only state
  and keeps it across saves. Switching modes must not rebuild it, because a rebuild drops
  undo history, cursor and scroll.
- `web/src/plan/editor/edit-panels.tsx` `EditToolbar` holds Save and Done. The mode toggle and
  the formatting buttons go in that toolbar.
- `web/src/plan/editor/edit-body.tsx` wires the toolbar, the surface and Add to chat.
- Chat attachments from the editor: `selection-attachment.ts` and `editor-provenance.ts` take the
  text from `selection.toString()`, the DOM text. In live preview the DOM lacks the hidden
  markers, so the attachment would lose them. Section 06 says the editor's attachment carries the
  source "including the Markdown markers live preview hides". The fix: the handle gains
  `rangeText(range)`, which returns the document slice between the range's mapped positions, and
  the provenance prefers it over `selection.toString()`. Plumbing: `editorProvenance(...)` returns
  `(selection, lines, text?: string | null) => AttachmentDraft | null`; it uses `text` when it is a
  non-empty string, else `selection.toString()`. `useEditorSelectionAttachment` calls
  `handle.current?.rangeText?.(range)` on the same `range` it passes to `rangeLines` and hands the
  result as the third argument of its `provenance` callback, whose type gains
  `text?: string | null`. The whole chain is W2's: `rangeText` → the hook's callback type →
  `useSourceAttachment` in `edit-body.tsx` (its callback must declare the third parameter and
  forward it as `provenance(selection, lines, text)`; TypeScript lets a callback silently ignore
  extra arguments, so the type alone proves nothing) → `editorProvenance` → the draft. The diff
  view's `diffProvenance` keeps its two-argument form and so keeps taking the DOM text; the diff
  has no live preview, so nothing is hidden there. Add to chat is driven only by `selectionchange`,
  so this covers every path. The contract `add to chat carries the source markers` drives the
  real Add to chat button.
- Tests that fake the CodeMirror module (`source-editor.test.tsx`, `edit-mode.test.tsx`,
  `editor.contracts.test.tsx`, `merge-panel.test.tsx`, `merge-panel.readonly.test.tsx`,
  `load.test.ts`, `load.retry.test.ts`) must keep compiling against the widened handle. Update
  their fakes; do not weaken an existing assertion.
- `web/src/plan/editor/drafts.ts` shows the project-scoped storage pattern
  (`serverInfoAtom` → `project_key`), and `web/src/plan/chat/storage.ts` `storageKey` builds
  `loom-plan:<project>:<name>` keys. `drafts.ts` `projectKey()` reads
  `store.get(serverInfoAtom)?.project_key` from the module-level store in `@/state/store` (no hook,
  no argument), so `readEditorMode()` takes no argument and does the same. Use
  `storageKey(projectKey, "editor-mode")` for the remembered mode. With no project key yet,
  `readEditorMode()` returns `preview` and `writeEditorMode` writes nothing. Wrap every storage
  read and write in try/catch; a failure means the default, `preview`. A stored value other than
  `"preview"` or `"source"` reads as `preview`.
- **One source of truth for the mode.** `edit-body.tsx` calls `readEditorMode()` once, in its
  `useState` initialiser, and owns the mode. `source-editor.tsx` takes it as a `mode` prop, never
  reads storage, and creates the editor with the prop's current value (through its existing
  `latest` ref, since the build effect depends only on `[path, readOnly]`), so a rebuild (a
  `readOnly` change, or the source editor remounting after the merge view closes) keeps the mode
  the toolbar shows even when storage is blocked.

## Line endings

CodeMirror splits a string on `\n`, `\r` and `\r\n` and `state.doc.toString()` joins with `\n`, so
the editor today turns a CRLF document into LF on open (`createSourceEditor` gives no line
separator, and `getText`, `onChange` and the ready effect in `source-editor.tsx` all read
`doc.toString()`). An untouched CRLF save would therefore rewrite every line. This plan's byte rule
is only true once that is fixed, so the fix is part of this plan (W1, in the chunk):

- `web/src/plan/editor/line-break.ts` (new, pure, imports nothing from CodeMirror) exports
  `detectLineBreak(text: string): string | null`. It returns `null` when `text` contains no `\r`
  (the default splitting is already exact). Otherwise it returns the one break sequence every
  break in the text uses when they are all `\r\n`, or all `\r`; for a text that mixes breaks, or
  has a lone `\r` beside `\n`, it returns `"\n"`, so each stray `\r` stays a character of its line
  and the text round-trips byte for byte (CodeMirror draws such a `\r` as a special-character
  dot; that is accepted for a file that mixes line endings).
- `createSourceEditor` adds `EditorState.lineSeparator.of(sep)` when `detectLineBreak(options.doc)`
  is not null. The facet is fixed when the editor is built (the editor is rebuilt per path).
- Every read of the text goes through `state.sliceDoc()`, which joins with the configured
  separator: `getText`, the `onChange` listener and `rangeText`. No `state.doc.toString()` is left
  in `codemirror.ts`. Typing Enter inserts the configured separator; `setText` and the ready
  effect compare and insert strings that already use it.
- The merge and diff views are unchanged.

The contracts `line endings survive untouched` (the editor) and `saving an unedited document writes
identical bytes` (the React surface, real editor, Ctrl-S, the posted body) guard this.

## The interface (frozen; both workers code against it)

In `web/src/plan/editor/codemirror.ts`:

```ts
export type EditorMode = "preview" | "source";

export type FormatCommand =
  | "bold" | "italic" | "code" | "link" | "heading"
  | "bulletList" | "orderedList" | "taskList" | "quote" | "codeBlock" | "table";

export interface EditorHandle {
  // ...existing members unchanged...
  /** The document text between a DOM range's mapped positions, or null outside this editor. */
  rangeText(range: Range): string | null;
}

export interface SourceEditorHandle extends EditorHandle {
  // ...setGutterMarks unchanged...
  mode(): EditorMode;
  /** Reconfigures one compartment; text, selection and history are kept. */
  setMode(mode: EditorMode): void;
  /** Applies a toolbar command to the selection as one undoable transaction; no-op read-only. */
  format(command: FormatCommand): void;
}

export interface SourceEditorOptions {
  // ...existing fields unchanged...
  /** Defaults to "preview". */
  mode?: EditorMode;
  /** Mod-click on a link in live preview; the raw href from the Markdown source. */
  onFollowLink?(href: string): void;
}
```

`live-preview.ts` exports `livePreview(options: { onFollowLink?(href: string): void }):
Extension`, and `codemirror.ts` calls it by that name (the wiring check greps `livePreview(`).
`md-commands.ts` exports one `StateCommand` per `FormatCommand`, `formatCommand(name)`, and
`formatKeymap(): Extension` (the Mod-b, Mod-i, Mod-k bindings, grepped as `formatKeymap(`).
`marks-gutter.ts` exports what `codemirror.ts` needs of the moved gutter code.

`rangeText` is implemented for every handle (source, merge, diff) in `viewHandle`, using the same
owner lookup and `posAtDOM` mapping as `rangeLines`.

## Behaviour settled for this plan

- **Commands.** `bold` `**`, `italic` `*`, `code` `` ` ``: wrap the selection, or unwrap it when
  it is already wrapped exactly; with an empty selection, insert the pair and put the cursor
  between. `link` makes the selection the text of an inline Markdown link whose target is a
  placeholder, and selects the placeholder. `heading` cycles the
  selected lines through `# `, `## `, `### ` and no heading. `bulletList` `- `, `orderedList`
  `1. `, `taskList` `- [ ] `, `quote` `> `: toggle the prefix on every selected line.
  `codeBlock` wraps the selected lines in a fence. `table` inserts a two-column, two-row table
  skeleton on a new line. Each command is one transaction, so one undo reverses it.
- **Shortcuts.** Mod-b bold, Mod-i italic, Mod-k link, in both modes, installed only by
  `createSourceEditor`, next to `saveKeymap` and before `baseExtensions` (which holds
  `defaultKeymap`), so it wins. `defaultKeymap` binds Mod-i to `selectParentSyntax`, so the
  binding must return true. Each binding returns true even when read-only (the command does
  nothing), because a binding that returns true gets `preventDefault`: Mod-k then never opens the
  browser's location bar and Mod-i never falls through. The merge and diff views get no format
  keymap.
- **Marker hiding.** In live preview, the marks of `HeaderMark`, `EmphasisMark`, `CodeMark`
  (inline only: its parent is `InlineCode`, not `FencedCode`), `StrikethroughMark` and the marks
  of an inline link (exact run: "Links" below) are hidden with `Decoration.replace` on every line
  that no selection range touches. An ATX `HeaderMark` covers only the `#` characters; the replace also takes the
  one space after the opening mark, and a closing `HeaderMark` is hidden with the space before it.
  A bullet `ListMark` (`-`, `*`, `+`) is drawn as a bullet widget and a `QuoteMark` as a bar
  widget the same way; an ordered `ListMark` (`1.`) stays as its text, so numbers are never turned
  into bullets. The `[ ]` text of a task is the `TaskMarker` node (3 characters), a child of the
  block node `Task`, which spans the whole item text; the checkbox widget replaces `TaskMarker`
  only, and on a task item the `ListMark` before it is hidden rather than drawn as a bullet.
  Clicking the checkbox dispatches one change of the `TaskMarker`'s middle character (`[ ]` ↔
  `[x]`; `[X]` counts as checked). The item label after the box (`Task` minus `TaskMarker`) stays
  visible, with its own inline decorations, whether or not the line is touched. No marker inside `FencedCode`, `Table`, `HTMLBlock`, `Comment`
  or `CommentBlock` nodes is hidden, with the fence exception below. The tree has mounted
  sub-trees (the yaml grammar under `CodeText`, the html grammar under comments and `HTMLBlock`,
  and a yaml `Comment` node), so the walk skips the descendants of those node types on enter
  (return `false` from `enter`) rather than testing node names alone.
- **Links.** An inline link is a `Link` node with a `URL` child. Off a touched line, hide its
  opening `[` mark and the whole run from its closing `]` mark to the end of the node (the `](`,
  the `URL`, any whitespace, any `LinkTitle`, the `)`), so `[label](url "title")` reads `label`
  with the label's own inline decorations kept. A `Link` without a `URL` child (reference or
  shortcut form), an `Image` and an `Autolink` hide nothing. Mod-click, below, still follows an
  `Autolink`.
- **Names the contracts read.** Toolbar buttons (accessible names): "Live preview", "Source",
  "Bold", "Italic", "Code", "Link", "Heading", "Bulleted list", "Numbered list", "Task list",
  "Quote", "Code block", "Table" (the same order as `FormatCommand`). DOM classes: `cm-lp-fence-label`
  on the language label widget, `cm-lp-dim` on a dimmed table delimiter mark, and the existing
  `cm-lineNumbers` and `cm-line-mark` (the validation dot, unchanged). The task box is an
  `input[type=checkbox]` inside `.cm-content`.
- **Rebuild triggers.** Both decoration providers rebuild on doc, selection and viewport changes
  AND when `syntaxTree(update.state) !== syntaxTree(update.startState)`. The initial parse covers
  only the first 3000 characters (`@codemirror/language`, `Work.InitViewport`); the rest arrives in
  idle slices that change neither doc nor viewport, and a loom plan is far longer, so without this
  trigger nothing below the first 3000 characters is decorated. `forceParsing` from
  `@codemirror/language` produces exactly such a parse-only transaction, so tests use it. The
  decorations of both providers (inline marks and the fence lines) must follow it without a
  cursor move or an edit.
- **Touching.** A selection range touches a node or line when `range.from <= to && range.to >=
  from` (inclusive at both ends).
- **Fenced code blocks.** The block's lines get a line class for the monospace face on the muted
  ground (`var(--muted)`). The opening and closing fence lines (`CodeMark` and `CodeInfo` of a
  `FencedCode` node) are hidden as whole lines with `Decoration.replace` while no selection range
  touches the block; when any range touches the block they show. The replace runs from the
  `CodeMark` start through the line break that ends the fence line (a content-only replace
  leaves an empty line), so the indentation or `>` before a fence inside a list item or quote
  stays. When a hidden opening fence merges with the first body line, the body line's own
  line class is dropped, so the fence line also carries the code ground class. An unterminated
  fence has one `CodeMark`; only it is hidden. A small widget at the block's first visible line
  shows the language label from `CodeInfo` when there is one (`CodeInfo` exists only when text
  follows the fence). The code inside keeps its language highlighting, so fenced `yaml`,
  including the plan metadata block, stays highlighted. Do NOT add `EditorView.atomicRanges`:
  replaced ranges are then not skipped by the arrow keys, so the cursor can reach a hidden fence
  line, which reveals it, and Backspace or Delete never removes a hidden line whole. For fence
  hiding, the touch test widens the block by one character at each end, so Backspace at the start
  of the line after a hidden closing fence, or Delete at the end of the line before a hidden
  opening fence, reveals the fence instead of joining it to the next line unseen.
- **Tables.** Table lines get a line class for the monospace face, and the `TableDelimiter`
  pipes and the delimiter row are dimmed with a mark class (`var(--muted-foreground)`). The
  delimiter row is itself one `TableDelimiter` node, a direct child of `Table`, and each pipe in a
  header or row is a one-character `TableDelimiter`, so one mark rule covers both. Nothing in a
  table is hidden, and it is never drawn as a grid.
- **The validation gutter.** `marksGutter` (the plan validation markers, `setGutterMarks`) stays
  outside the mode compartment, so validation markers show in both modes. Only `lineNumbers()`
  moves into the compartment.
- **Styling.** Heading lines get line classes sized like the reading view
  (`web/src/plan/markdown/markdown.css`): h1 `1.5em` weight 650 letter-spacing `-0.02em`, h2
  `1.25em` weight 650, h3 `1.08em`, h4 `1em`, h5 and h6 `0.9em` in `var(--muted-foreground)`, all
  in `var(--font-sans)` weight 600 by default. Use these literal em values. Bold, italic,
  strikethrough, links and inline code keep the existing `highlightStyle` rules in
  `codemirror.ts`. Live preview prose uses `var(--font-sans)`; inline code, code blocks and
  tables stay in `var(--font-mono)`. The mode compartment carries
  `EditorView.editorAttributes.of({ class: "cm-live-preview" })` in live preview, and the sans
  face is set on a new `"&.cm-live-preview .cm-content"` rule: `theme.test.ts` pins `&` and
  `.cm-scroller` to `--font-mono`, so those rules stay. The reading view's `--font-reading` and
  `--reading-size` are defined only in `web/src/plan/plan.css`, and `theme.test.ts` requires
  every `var(--x)` in `theme.ts`, comments included, to be defined in `index.css` or
  `tokens.css`, so do not use them. HTML comments are already dimmed by `highlightStyle`'s
  `tags.comment` rule; live preview only must not hide them. Colours are design-token variables
  only, as `theme.ts` requires: `theme.test.ts` checks every property ending in `color`, bans
  colour literals and the string `@codemirror` in `theme.ts`, and a second `".cm-content"` key
  would be a duplicate key, so extend the existing rule.
- **Links.** A plain click places the cursor. Mod-click on a link in live preview follows it.
  `live-preview.ts` installs `EditorView.domEventHandlers({ mousedown })` that, when `metaKey` or
  `ctrlKey` is set and the target lies in a `Link` (or `Autolink`) node, calls `onFollowLink` with
  the node's `URL` text and returns true (which prevents the default and stops CodeMirror's own
  handler). It finds the position with `view.posAtDOM(event.target)`, not `posAtCoords` (jsdom's
  rects are zero). `allowMultipleSelections` is off, so Mod-click has no other meaning today.
  `source-editor.tsx` passes `onFollowLink` from `SourceEditorProps`. `edit-body.tsx` resolves the
  href with `resolveLink(doc.path, href)` from `web/src/plan/files/address.ts`, the resolver the
  reading view's `SafeLink` (`web/src/plan/markdown/inline.tsx`) uses; do not write a second one.
  `doc` navigates to `toAddress(path, anchor)` and `anchor` to `toAddress(doc.path, anchor)`, with
  `useNavigate()` from react-router (edit-body has no navigate today; `edit-session.ts` shows the
  import), so the existing leave guard (`leave-guard.ts`, `useBlocker`) asks first when text is
  unsaved. `external` opens with `window.open(href, "_blank", "noopener,noreferrer")`, which the
  router guard does not see and does not need to (the page stays). `outside` and `text` do
  nothing.
- **The checkbox.** Widget DOM ignores editor events by default (`WidgetType.ignoreEvent`
  returns true), so the checkbox gets its own `click` listener (`addEventListener`, never
  `innerHTML`) that dispatches the toggle through the view, unless the state is read-only.
- **`rangeText`.** It maps the range's ends with `posAtDOM` through the same owner lookup as
  `rangeLines`, applies the same deleted-chunk guard (`inDeletedChunk`), and slices
  `owner.state.doc`. `posAtDOM` inside a replaced widget returns the widget's start, so a
  partial selection that ends beside a hidden marker may leave that marker out; a whole-line or
  wider selection carries every marker. That is accepted.
- **Read-only.** When the editor is read-only, `format` does nothing, the formatting buttons are
  disabled, and a task box does not toggle. The mode toggle still works. The formatting buttons
  are also disabled while the merge view is shown (`state.currentText !== null` in
  `edit-body.tsx`), and `onFormat` narrows the shared `EditorHandle` ref with
  `"format" in handle` before calling it.
- **Toolbar.** After Save and Done: a two-button segmented control "Live preview" / "Source"
  with `aria-pressed`, then icon buttons with `aria-label`s and `title`s naming the command and
  its shortcut. Icons come from `lucide-react`, already a dependency (for example
  `web/src/plan/help/help-dialog-nav.tsx`). The toolbar wraps at phone width.

## Visual design

Every worker that does visual design invokes the `frontend-design:frontend-design` skill before
writing code. That covers both workers, since W1 styles the live preview and its widgets and W2
builds the toolbar, and every fix worker spawned for a visual defect in either stage. This is the
operator's instruction. The skill shapes how the editor looks within the settled behaviour above.
It does not change that behaviour or the design-token rule.

## Lanes

Claude only. The work is UI behaviour checked against jsdom and a real browser, with tests that
iterate. That fits the Claude lane better than codex, which runs no fixture-backed tests.
