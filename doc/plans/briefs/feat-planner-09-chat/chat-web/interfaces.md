# chat-web: cross-unit interfaces (wave 3)

Every wave 3 unit reads this after common.md and its own brief. These signatures are fixed so units
written in parallel compile together. A unit exports exactly these names from exactly these files;
a unit never edits a file it does not own. Working dir `web/`; import with the `@/` alias.

Producers already in the tree (read their exports with `loom map --outline <file>`; never edit them):
`chat/model.ts` (`snapshotAtom`, `sessionAtom`, `turnsAtom`, `queueAtom`, `pendingPermissionsAtom`,
`queueReturnedAtom`, `unconfirmedSendsAtom`, `useChatSocket(store)`), `chat/commands.ts`
(`sendMessage`, `startChat`, `withdrawMessage`, `answerPermission`, `interruptChat`, `endChat`,
`suggest`, `answerSuggestion`, `toAttachmentInput`, `getSessionBrief`), `chat/drafts.ts`
(`draftAtom`), `chat/prefs.ts` (`panelWidthAtom`, `panelCollapsedAtom`), `chat/storage.ts`
(`storageFullAtom`), `chat/visible-controls.ts` (`visibleControls`, `<Untrusted text />`),
`chat/format-attachments.ts` (`formatMessage`, `deliveredBytes`), `chat/tool-group.ts`
(`groupToolRows`, `ToolGroup`), `chat/offer-handlers.ts` (`registerAuthoringOffers(send)`),
`review/api.ts`, `review/live.ts` (`openQuestionsAtom`, `hashesAtom`, `useReviewLive`),
`review/checkpoint.ts` (`checkpointAtom`, `changedSince`, `markSeen`), `api/schema-chat.ts` (types
`Item`, `Turn`, `SessionView`, `ChatSnapshot`, `EndedChat`, `Attachment`, `QueuedMessage`).
Plan 2/3/5 producers: `ui/disabled-reason`, `ui/load-state`, `state/atoms.ts`, `offers/actions.ts`,
`attachments/index.ts`, `files/address.ts` (`docRel`, `repoRel`), `diff/open-diff.ts` (`openDiff`),
`editor/request-edit.ts`, the `<Markdown>` renderer.

`ItemOf<T>` below is `Extract<Item, { type: T }>`.

## Components by file (all named exports, function components, no default exports)

| File | Export | Signature |
| --- | --- | --- |
| `chat/patch-view.tsx` | `PatchView` | `({ patch }: { patch: string }) => JSX.Element` (unified diff as text) |
| `chat/tool-row.tsx` | `ToolRow` | `({ item, nested }: { item: ItemOf<"tool">; nested?: ItemOf<"tool">[] }) => JSX.Element` (`nested`: rows whose `parent` is this row's `tool_use_id`) |
| `chat/chips.tsx` | `PendingChips` | `() => JSX.Element \| null` (reads `draftAtom`, `pendingAttachmentsAtom`, `hashesAtom`) |
| `chat/chips.tsx` | `SentChips` | `({ item, turn }: { item: ItemOf<"user">; turn: Turn }) => JSX.Element \| null` |
| `chat/chips.tsx` | `chipLabel` | `(a: { path: string; anchor: string \| null; lines: [number, number] \| null; stage: string \| null; summary_sentence: string \| null; kind: "source" \| "summary" }) => string` |
| `chat/permission-card.tsx` | `PermissionCard` | `({ item, inline }: { item: ItemOf<"permission">; inline?: boolean }) => JSX.Element` |
| `chat/suggestion-cards.tsx` | `SuggestionCards` | `() => JSX.Element \| null` (holder only; subscribes to review `suggestion` messages itself) |
| `chat/composer-gate.ts` | `composerBlock` | `(session: SessionView \| null, ctx: { stopping: boolean; readOnly: boolean }) => string \| null` |
| `chat/stop-control.tsx` | `StopControl` | `({ confirm }: { confirm?: string }) => JSX.Element \| null` |
| `chat/queue-strip.tsx` | `QueueStrip` | `() => JSX.Element \| null` |
| `chat/composer.tsx` | `ComposerArea` | `() => JSX.Element` (queue strip, pinned permission card, suggestion cards, chips, composer) |
| `chat/turn-footer.tsx` | `TurnFooter` | `({ turn, sessionId }: { turn: Turn; sessionId: string }) => JSX.Element \| null` (role group named `Turn <n> changes`) |
| `chat/footer-extras.tsx` | `TurnFooterExtras` | `({ turn }: { turn: Turn }) => JSX.Element \| null` returns `null` |
| `chat/rebuilt-summary-extras.tsx` | `RebuiltSummaryExtras` | `({ id, turn }: { id: string; turn: Turn }) => JSX.Element \| null` returns `null` |
| `chat/restore.tsx` | `RestoreButton` | `({ sessionId, turn, path }: { sessionId: string; turn: Turn; path: string }) => JSX.Element \| null` |
| `chat/restore.tsx` | `RestoreAll` | `({ sessionId, turn, paths }: { sessionId: string; turn: Turn; paths: string[] }) => JSX.Element \| null` |
| `chat/session-divider.tsx` | `SessionDivider` | `({ previous, next }: { previous: EndedChat; next: SessionView \| null }) => JSX.Element` |
| `chat/start-chat.tsx` | `NewChatButton` | `() => JSX.Element \| null` |
| `chat/start-chat.tsx` | `StartingBrief` | `({ session }: { session: SessionView }) => JSX.Element \| null` (Brief link and dialog while Starting) |
| `chat/end-chat.tsx` | `EndChatMenuItem` | `() => JSX.Element \| null` (holder only, hidden for `kind === "pressure"`) |
| `chat/brief-dialog.tsx` | `BriefDialog` | `({ sessionId, onClose }: { sessionId: string; onClose: () => void }) => JSX.Element` |
| `chat/items.tsx` | `ItemView` | `({ item, turn, nested }: { item: Item; turn: Turn; nested?: ItemOf<"tool">[] }) => JSX.Element \| null` |
| `chat/transcript.tsx` | `Transcript` | `() => JSX.Element` (role log) |
| `chat/state-line.tsx` | `StateLine` | `() => JSX.Element` (role status) |
| `chat/run-header.tsx` | `RunHeader` | `({ session }: { session: SessionView }) => JSX.Element \| null` |
| `chat/connection-banner.tsx` | `ConnectionBanner` | `() => JSX.Element \| null`, and `useConnectionDim(): string \| null` returning "Last update <age>" while a banner shows |
| `chat/attention.ts` | `useAttention` | `() => void` (tab title and notifications) |
| `chat/panel.tsx` | `ChatPanel`, `ChatRegistrations`, `ChatSheetButton` | `() => JSX.Element \| null` each; `ChatSheetButton` is the header button below lg |
| `review/send-to-author.tsx` | `registerSendToAuthor` | `() => () => void` |
| `review/changed-since.tsx` | `ChangedSinceControl` | `({ compact }: { compact: boolean }) => JSX.Element \| null` |
| `review/changed-since.tsx` | `useChangedSince` | `() => Array<{ path: string; change: "added" \| "changed" \| "removed" }>` |
| `review/open-questions.tsx` | `OpenQuestionCount` | `({ path }: { path: string }) => JSX.Element \| null` |
| `review/built-marker.tsx` | `BuiltMarker` | `({ section }: { section: string }) => JSX.Element \| null` |

Notes that bind several units:

- Opening the sheet or expanding the rail: `panelCollapsedAtom` (prefs.ts) is the collapsed flag; the
  sheet's open flag is `chatSheetOpenAtom` exported from `chat/panel.tsx`. `StartingBrief` and
  `NewChatButton` set `panelCollapsedAtom` to false and `chatSheetOpenAtom` to true only through
  `startChat` success paths; importing `chatSheetOpenAtom` from `chat/panel.tsx` into start-chat.tsx
  would be a cycle, so `chatSheetOpenAtom` lives in `chat/sheet-state.ts` (owned by the panel unit,
  which creates it first, one line: `export const chatSheetOpenAtom = atom(false);`).
- `ChatRegistrations` calls `registerAuthoringOffers((message) => startChat(...))` with a sender built
  from `commands.ts`, `registerSendToAuthor()` and `useReviewLive()`.
- The pinned permission card is `pendingPermissionsAtom[0]` rendered by `ComposerArea` as
  `<PermissionCard item inline={false} />` above the composer; the transcript renders the same item
  inline with `inline`. Both are role region named `Permission: <tool>`.
- Frozen contracts: `chat/chat.contracts.test.tsx` mounts the real routes, so every export above must be
  reachable from `src/plan/dock.tsx` and `src/plan/header-extras.tsx`.
