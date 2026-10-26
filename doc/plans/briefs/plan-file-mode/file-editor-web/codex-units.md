# file-editor-web: codex units

Lane: `loom-codex-forwarder`, `--model gpt-6.1-terra --effort high`, foreground only, at most six
at once over disjoint files, each spawn with an explicit Bash timeout of 600000 ms. A unit is one
file, or one file plus its test, at most three steps, inside the wrapper's 540 s deadline; a
timed-out unit is re-split, never re-forwarded as is.

## Rules every unit follows (codex reads them from this file; the prompt is only the fixed line)

- Read `doc/plans/briefs/plan-file-mode/common.md` and the spec section named below; write only
  the unit's files. Do not run git; touch no hidden loom state or work path. At most
  `cd web && node_modules/.bin/tsc -b --noEmit`; the orchestrator runs the tests.
- No TypeScript parameter properties (`erasableSyntaxOnly`); `z` only from `@/api/zod`; leave 10%
  under the size limits. Codex does not run `oxfmt`: the orchestrator formats each unit's files.
- `loom map` answers from the base layer; shapes from other units and the Claude workers of this
  stage are pinned in the spec sections named.

## Orchestrator duties

**Module seeding (every stage):** whenever the orchestrator adds a `mod` line, it creates the
module's file at the same moment holding only a `//!` purpose line, so the crate always compiles
and every unit's proof selects its tests as soon as the unit writes them. The unit or worker that
owns the file replaces the seed; the seed is not ownership.


After every unit: `cd web && node_modules/.bin/oxfmt <the unit's files>`, `git status --short`
(only those files), and the unit's proof command.

## Units

| Unit | Wave | Owns | Reads | Entry points | Spec | Proof (orchestrator) |
| ---- | ---- | ---- | ----- | ------------ | ---- | -------------------- |
| E1 | 0 | `web/src/plan/editor/drafts.ts`, `web/src/plan/editor/drafts.test.ts` | plan 2 `ServerInfo.project_key` (`serverInfoAtom`) | `loadDraft`, `saveDraft`, `findDrafts`, `clearDrafts`, `SaveDraftResult` | codemirror-core.md "drafts.ts" | `cd web && node_modules/.bin/vitest run src/plan/editor/drafts.test.ts` |
| E2 | 0 | `web/src/plan/editor/load.ts`, `web/src/plan/editor/load.test.ts` | the codemirror-core worker writes `codemirror.ts` (exports pinned in codemirror-core.md) | `loadCodeMirror`, the static import-boundary test | codemirror-core.md "Lazy boundary" and "Tests" | `vitest run src/plan/editor/load.test.ts` |
| E3 | 0 | `web/src/plan/editor/theme.ts`, `web/src/plan/editor/theme.test.ts` | `web/src/aurora-ui/shared/styles/tokens.css`, `web/src/index.css` (variable names) | `themeSpec` (a plain style-spec object, no `@codemirror` import) | codemirror-core.md "Lazy boundary" (`theme.ts`) | `vitest run src/plan/editor/theme.test.ts` (every colour value is a `var(--...)` reference; no hex or rgb literal; no `@codemirror` import) |
| E4 | 0 | `web/src/plan/editor/request-edit.ts`, `web/src/plan/editor/request-edit.test.ts` | `@/plan/files/address` (`toAddress`) | `requestEditAtom`, `requestEdit` | editor-save.md "Entry point for other plans" | `vitest run src/plan/editor/request-edit.test.ts` |
| E5 | 0 | `web/src/plan/editor/leave-guard.ts`, `web/src/plan/editor/leave-guard.test.ts` | react-router `useBlocker` | `useLeaveGuard(dirty: boolean)` | editor-save.md "Behaviour" (first bullet) | `vitest run src/plan/editor/leave-guard.test.ts` |
| E6 | 1 | `web/src/plan/editor/editor-provenance.ts`, `web/src/plan/editor/editor-provenance.test.ts` | `AttachmentDraft` (`@/plan/attachments`), `SectionView` | `editorProvenance(path, sections, stages)` | editor-save.md "Add to chat in the editor" | `vitest run src/plan/editor/editor-provenance.test.ts` |
| E7 | 0 | `web/src/plan/diff/open-diff.ts`, `web/src/plan/diff/open-diff.test.ts` | `@/plan/files/address` | `openDiff(kind, path, turn?)` | diffs.md "Shapes" (openDiff) | `vitest run src/plan/diff/open-diff.test.ts` |
| E8 | 1 | `web/src/plan/diff/diff-provenance.ts`, `web/src/plan/diff/diff-provenance.test.ts` | `AttachmentDraft`, `SectionView` | `diffProvenance(path, sections)` | diffs.md "Add to chat in a diff" | `vitest run src/plan/diff/diff-provenance.test.ts` |
| E9 | 1 | `web/src/plan/editor/save-flow.ts`, `web/src/plan/editor/save-flow.test.ts` | `saveDoc` (`@/plan/files/api`), E1 drafts API (pinned) | `saveFlow(state, event) -> state` reducer: idle, saving, saved, conflict, invalid, error, with the draft clearing rules | editor-save.md "Behaviour" (Save, Drafts, second conflict) | `vitest run src/plan/editor/save-flow.test.ts` |

Waves: 0 runs E1-E5 and E7 while the Claude `codemirror-core` worker (dependency install,
`codemirror.ts`, `source-editor.tsx`, bundle rules) works in the background; 1 runs E6, E8, E9
while the Claude `editor-save` and `diffs` workers work in the background; those workers use the
codex files and never edit them.
