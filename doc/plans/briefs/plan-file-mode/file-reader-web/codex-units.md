# file-reader-web: codex units

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
| R1 | A | `web/src/plan/files/api.ts`, `web/src/plan/files/api.test.ts` | plan 2 `web/src/plan/api/{client.ts, socket.ts}`, `web/src/plan/api/schema-files.ts` | `fetchTree`, `fetchDoc`, `searchDocs`, `fetchHeadDiff`, `fetchTurnDiff`, `saveDoc`, `useFileSocket` | files-api.md "api.ts" | `cd web && node_modules/.bin/vitest run src/plan/files/api.test.ts` |
| R2 | A | `web/src/plan/files/atoms.ts`, `web/src/plan/files/atoms.test.ts` | `schema-files.ts` | `treeAtom`, `openDocAtom`, `docChangedAtom`, `validationAtom`, `previousOpenTextAtom`, `openDocIsStaleAtom` | files-api.md "atoms.ts" | `vitest run src/plan/files/atoms.test.ts` |
| R3 | A | `web/src/plan/files/address.ts`, `web/src/plan/files/address.test.ts` | none | `toAddress`, `parseAddress`, `resolveLink`, `docRel`, `repoRel` | files-api.md "address.ts" | `vitest run src/plan/files/address.test.ts` |
| R4 | A | `web/src/plan/files/plan/split.ts`, `web/src/plan/files/plan/split.test.ts` | `DocView` type (schema-files.ts) | `splitPlan(text, metadataLine) -> { prose, prose_offset, record, record_offset }` | plan-view.md "Rendering" item 2 | `vitest run src/plan/files/plan/split.test.ts` |
| R5 | A | `web/src/plan/attachments/types.ts`, `web/src/plan/attachments/atoms.ts`, `web/src/plan/attachments/atoms.test.ts` | plan 2 `web/src/plan/state/atoms.ts` | `AttachmentDraft`, `pendingAttachmentsAtom`, `addAttachmentAtom` | attachments.md "Shapes" | `vitest run src/plan/attachments/atoms.test.ts` |
| R6 | A | `web/src/plan/attachments/secondary.ts`, `web/src/plan/attachments/secondary.test.ts` | R5 types (pinned) | `registerSelectionSecondary`, `selectionSecondaries()` | attachments.md "Shapes" and the AddToChat bullet | `vitest run src/plan/attachments/secondary.test.ts` |
| R7 | B | `web/src/plan/attachments/view-provenance.ts`, `web/src/plan/attachments/view-provenance.test.ts` | R5 types, `SectionView` | `renderedViewProvenance(path, sections)` | attachments.md "view-provenance.ts" bullet | `vitest run src/plan/attachments/view-provenance.test.ts` |

Waves: A runs R1-R6; then B runs R7 while the Claude workers (`markdown`, `tree-search`,
`plan-view`, `attachments-ui`) work in the background; they read R1-R6's files and never edit
them.
