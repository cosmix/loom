# file-editor-web / codemirror-core: the lazy CodeMirror chunk and the editor surface

Claude worker (sonnet), wave 0, beside codex units E1 (`drafts.ts`), E2 (`load.ts`), E3
(`theme.ts`) of `codex-units.md`, whose specs are the matching sections below. Read
`doc/plans/briefs/feat-planner-03-file-mode/common.md` first. Spec: `doc/product/section_06.md` "The
editor". Pattern: `web/src/components/terminal/emulator.ts` (`createEmulator`, about :150-160:
`Promise.all([import("@xterm/xterm"), ...])`, so the main bundle never carries xterm).

## Files you own

`web/src/plan/editor/{codemirror.ts, source-editor.tsx, source-editor.test.tsx}`; `web/package.json`,
`web/bun.lock` (through `bun add` only, first thing, so the codex units compile);
`web/scripts/bundle-rules.json`. `codemirror.ts` imports E3's `themeSpec` from `./theme`.

## Dependencies

`cd web && bun add @codemirror/state @codemirror/view @codemirror/commands @codemirror/language @codemirror/lang-markdown @codemirror/lang-yaml @codemirror/merge @codemirror/search`
(`@codemirror/merge` for the merge and diff views; `lang-markdown` with `codeLanguages` resolving
`yaml` fences to `lang-yaml`). No `codemirror` meta package and no `basicSetup`: assemble line
numbers, history, default keymap and search keymap from the packages above.

## Lazy boundary

- `codemirror.ts` is the only module that statically imports `@codemirror/*`. It exports
  `createSourceEditor(parent, { doc, readOnly, onChange, onSave })`, `createMergeView(parent, { left, right, onChange })`
  (left read-only, right editable, chunk copy controls left to right), `createDiffView(parent, { before, after, unified })`
  (read-only both sides; `unifiedMergeView` below 900px), each returning a handle with `destroy()`,
  `getText()`, `setText()`, `searchOpen(): boolean` (`searchPanelOpen` from `@codemirror/search`).
- `load.ts`: `loadCodeMirror(): Promise<typeof import("./codemirror")>` = `import("./codemirror")`,
  memoised. Every other file reaches CodeMirror only through `loadCodeMirror()`.
- `theme.ts`: a plain style-spec object (no `@codemirror` import) reading CSS variables only
  (`var(--background)`, `--foreground`, `--muted`, `--border`, `--primary`, the monospace font), so
  `.dark` on `<html>` switches it with no second palette; `codemirror.ts` wraps it with
  `EditorView.theme`.
- `web/scripts/bundle-rules.json`: add to both `index.html` and `plan.html` the chunk globs
  `node_modules/@codemirror/**`, `node_modules/@lezer/**`, `src/plan/editor/codemirror.ts`, and the
  text `cm-editor` (CodeMirror's root class) to `forbidText`.

## `source-editor.tsx`

`export function SourceEditor(props: { path: string; text: string; openedHash: string; readOnly?: boolean; onChange(text: string): void; onSave(): void; gutterMarks?: number[] /* 1-based lines */ }): JSX.Element`:
loads CodeMirror on mount (a muted "Loading editor" line meanwhile), Markdown mode for every
`.md`, line numbers, Ctrl-S / Cmd-S calls `onSave` (prevent the browser's save), marks
`gutterMarks` lines in the gutter.

## `drafts.ts`

`loadDraft(path, openedHash): string | null`, `saveDraft(path, openedHash, text): SaveDraftResult`
(`"saved" | "too_large" | "storage_full" | "unavailable"`), `findDrafts(path): { openedHash: string; text: string; savedAt: number }[]`
(newest first), `clearDrafts(path)` (every hash of that path). Key
`loom-plan-draft:<project_key>:<path>:<openedHash>` (`project_key` from plan 2's `ServerInfo`, so two
projects on one origin never share drafts); the value stores `savedAt`. Every access in
try/catch: a read that throws is "unavailable" (private windows); a write that throws
`QuotaExceededError` first evicts this module's oldest drafts (by `savedAt`, never another
module's keys) until the write fits or none is left, then returns `storage_full`. Total budget
2 MiB across all drafts, oldest evicted first; a single draft over 1 MiB is `too_large` and not
stored.

## Tests

`load.test.ts`: `loadCodeMirror` is the only importer (a static test that reads every
`web/src/plan/**/*.ts(x)` except `codemirror.ts` and `*.test.*` files, and fails on a static
`@codemirror/` import);
the Markdown language config resolves a `yaml` fence to the YAML language (unit test on the
`codeLanguages` resolver exported from `codemirror.ts`, imported in the test only); drafts round trip, survive a thrown `localStorage`, keyed by project, path and hash; `findDrafts`
lists every hash of a path newest first; a quota error evicts the oldest and retries, and with
nothing left returns `storage_full`; the 2 MiB budget evicts oldest first; `SourceEditor` with a mocked
`loadCodeMirror` calls `onSave` on Ctrl-S.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/editor` pass; after `node_modules/.bin/tsc -b && node_modules/.bin/vite build`,
`node scripts/bundle-check.mjs` passes with the new rules.
