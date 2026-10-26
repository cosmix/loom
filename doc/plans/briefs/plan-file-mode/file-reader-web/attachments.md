# file-reader-web / attachments: "Add to chat" selection control

Claude worker `attachments-ui` (sonnet) plus codex units R5 (`types.ts`, `atoms.ts`), R6
(`secondary.ts`), R7 (`view-provenance.ts`) of `codex-units.md`, whose specs are the matching
sections below. The Claude worker starts after codex wave A. Read `doc/plans/briefs/plan-file-mode/common.md` first.
Spec: `doc/product/section_06.md` "Taking a passage into chat"; `doc/product/section_08.md`
"Attachments" (only to see what plan 9 does with the drafts).

## Files you own

Claude `attachments-ui`: `web/src/plan/attachments/{index.ts, use-selection-attachment.ts, add-to-chat.tsx}`
and their tests (`use-selection-attachment.test.ts`, `add-to-chat.test.tsx`); `index.ts` re-exports
the codex units' modules. Codex: R5 `types.ts`, `atoms.ts`, `atoms.test.ts`; R6 `secondary.ts`,
`secondary.test.ts`; R7 `view-provenance.ts`, `view-provenance.test.ts`.

## Shapes (binding; plans 5 and 9 consume them)

```text
type AttachmentDraft =
  | { kind: "source"; path: string /* repo-relative doc/... */; anchor: string | null; text: string; lines: { start: number; end: number } | null; stageId: string | null; hash: string | null /* DocView.hash at selection */ }
  | { kind: "summary"; summary: string /* summary id, e.g. product/section_05 */; sentence: string /* sentence id, e.g. s-3fa9c2d1 */; text: string /* the sentence text as shown */; restsOn: { path: string /* repo-relative doc/... */; anchor: string }[]; hash: string | null /* the summary file's hash at selection */ };
pendingAttachmentsAtom: PrimitiveAtom<AttachmentDraft[]>
addAttachmentAtom: WritableAtom<null, [AttachmentDraft], void>   // de-duplicates identical drafts
useSelectionAttachment(container: RefObject<HTMLElement>, provenance: (sel: Selection) => AttachmentDraft | null): { draft: AttachmentDraft | null; rect: DOMRect | null; clear(): void }
<AddToChat draft={draft} rect={rect} onAdded={clear} />   // floating control next to the selection
registerSelectionSecondary(render: (draft: AttachmentDraft) => JSX.Element | null): () => void   // secondary.ts, exported from index.ts
```

Paths in drafts follow plan 2's wire path rule (repo-relative `doc/...`); `hash` is taken at
selection time: for the source arm `DocView.hash`; for the summary arm the summary page's file
hash, which plan 5 defines as `planning::sections::normalized_hash` of the store file
`doc/loom/summaries/<summary>.md` read no-follow (`ConceptPage.file_hash`), the same value plan 7's
`resolve` recomputes from the store for a summary entry (discovery does not list the store, so
`Project::content_hash` cannot be used there), so plan 9 can compute `selected_hash` and "changed since selected".

- `AddToChat` renders whenever a draft exists. Inside it, the "Add to chat" button shows only
  while `canAuthorAtom` (plan 2) is true; beside it, every renderer registered through
  `registerSelectionSecondary` is called with the draft and its result rendered (plan 9 registers
  "Send to author" for read-only browsers from its own module). Every mount site (view, editor,
  diff, plan 5's concept passages) gets the secondary with no edit. With no button and no
  secondary result, `AddToChat` renders nothing.
- `view-provenance.ts`: `renderedViewProvenance(path: string, sections: SectionView[])` returns a
  provenance callback for the rendered Markdown view: path; anchor = the id of the nearest heading at
  or before the selection start (walk the DOM to the closest preceding element with an `id` that is a
  section anchor); text = `selection.toString()` (the text as the reader saw it); `lines: null`;
  `stageId` = the `stage:<id>` row the selection starts in, when inside the Stages panel.
  A selection that is empty, collapsed, or crosses outside `container` gives `null`.
- Text over 16 KiB (section 12, "One attachment's text") is refused with an inline note on the
  control, not truncated silently.
- Mount it in the reading column: `reading-column.tsx` is `tree-search`'s; export a
  `<SelectionAttach container path sections />` wrapper and tell the orchestrator the one-line mount
  for that worker's file, which the orchestrator applies after both return.

## Tests

provenance picks the anchor of the containing section and the stage id inside a stage row; a
selection outside the container gives null; holder-only visibility of the button (holder, other holder, free); `canAuthorAtom` false plus a
registered secondary renders the secondary and no "Add to chat"; unregistering removes it; the
source draft carries the document's hash and repo-relative path;
adding the same draft twice keeps one; 16 KiB + 1 refused.

## Done when

`cd web && node_modules/.bin/tsc -b --noEmit`, `oxlint --deny-warnings`, `oxfmt --check`,
`vitest run src/plan/attachments` pass.
