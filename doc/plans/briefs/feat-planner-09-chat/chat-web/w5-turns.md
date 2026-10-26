# chat-web W5: turn footers, Restore, starting and ending chats

Lane note: `offer-handlers.ts` (`registerAuthoringOffers`) and its test are codex unit C8; the
section below on it is reference.

Working dir `web/`. You own `src/plan/chat/{session-divider.tsx, rebuilt-summary-extras.tsx, turn-footer.tsx, turn-footer.test.tsx,
footer-extras.tsx, restore.tsx, restore.test.tsx, start-chat.tsx, start-chat.test.tsx,
end-chat.tsx, brief-dialog.tsx, replay.test.tsx}` and `src/plan/concept/empty-offer.tsx` (plan
5's file: replace its body, keep its props). W1's atoms, commands and API clients exist. Frozen
contracts: `src/plan/chat/chat.contracts.test.tsx`.

## turn-footer.tsx

`role="group"` named "Turn <n> changes". First line: duration (`turn_duration`) and fresh tokens
(`fresh` and `compactTokens` from `web/src/lib/tokens.ts`) with a tooltip of the four parts.
"Changed <n> files" ("Changing" while running): per `changed_files` row the path relative to
`doc/` (monospace), `+a −b` or new/deleted, and a Diff link opening plan 3's "What a turn changed"
diff for the file and turn (disabled while running with "Available when the turn ends");
`by_tool: false` rows add "changed during this turn"; a path outside `doc/` has a
`--tone-warning` glyph, "outside doc/" and no Diff link. Under a changed product section: the
plans implementing it (chat-server `implemented-by`) grouped by state, written plans "may need
updating" with **Check these plans**, running and done plans "built from earlier text" with
**Plan a follow-up** (holder only): each fills the composer with "Check these plans against the
change to section_02." (or "Plan a follow-up for the change to section_02.") and attaches the
changed section and each plan as source entries. "Rebuilt summaries": one row per
`summaries_rebuilding` entry linking its summary page, "Rebuilding" with `BusyRoundel` while the
subagent runs; after the turn the row's tokens when the model carries them, "failed" with the
error on hover, or "left for the background rebuild". Last child: `<TurnFooterExtras turn />`
from `footer-extras.tsx`, which returns `null` (plans 10 and 11 fill it; keep the props).

## restore.tsx

Holder only, only while no turn runs, only for files the turn modified (not created, not
deleted): Restore on each footer row of a modified file, beside its Diff link (the Diff link's
`openDiff` call is in "Fix-round additions" below) and **Restore all** on a footer with two or more modified files, which confirms first with
the file list. `postRestore({ session, turn, paths, force })`: `restored` shows a short "Restored" note;
`changed` opens plan 3's editor merge view (section 06 "When the file changed underneath")
with the current text on the left and the turn-start text on the right as the unsaved edit (plan 3's
`requestEdit(store, { path, text: turn_start_text, openedHash: <the hash the file had at the turn's
end> })` then navigate to the returned `/files/<path>?edit=1`; the stale hash opens the merge view
at once); `invalid` asks section 06's "Save with errors" and resends with `force: true`;
`refused` shows the reason.

## start-chat.tsx, end-chat.tsx, brief-dialog.tsx, empty-offer.tsx

Start: from an offer (checklist offers, concept empty-state offers through `empty-offer.tsx`, file
mode's missing tree entries), **New chat**, or the first Send with no chat (W4 sends; the server
starts a refinement session with that text as the opening message). Starting opens the panel if
collapsed and shows Starting with the display name, model and effort, and a **Brief** link opening
the session's brief read-only in `brief-dialog.tsx`, all from plan 8's `GET /api/session/brief`
(`SessionBriefView.label` also labels the opening message; 404 `unknown_session` means not
rendered yet: retry once a second while Starting). Start commands carry plan 1's `Offer::id`, or
plan 8's start ids (`refine:<path>`, `refine-passage:<path>`, `knowledge`, `knowledge:<path>`)
from file mode and concept mode; `offer-handlers.ts` exports `registerAuthoringOffers()`, which calls plan 2's
`registerOfferAction(kind, handler)` (`web/src/plan/offers/actions.ts`; you never edit that
file) for every authoring kind (`start_vision`, `write_section`, `add_section_to_vision`,
`start_metaplan`, `write_plan`, `add_plan_to_metaplan`, `refine_spec`, `new_metaplan_round`,
`fix_vision_table`); each handler sends `start` with the `OfferView.id`. W2's `ChatRegistrations` calls it
once on mount. Plan 5's `EmptyOffer` body becomes plan 2's `<OfferButton offer />` for the offer
looked up in `checklistAtom`. With a chat
running, accepting another offer asks "End the current chat first? It will write its open
questions into the documents before it stops." and starts the new one when the old one has
ended. Losing a start race shows "<holder> started a chat first". End chat (panel menu, holder; hidden while `session.kind === "pressure"`):
"End this chat? The session will write its open questions and anything pending into the
documents, then stop." `empty-offer.tsx`: an enabled button for the holder or while nobody
holds authoring, the text only for viewers and in read-only mode.

## replay.test.tsx (the metaplan's replayed-session test)

Seed plan 7's recorded model fixtures step by step through W1's `applyModelMessage`: attach a
passage through plan 3's atom, send (assert the command payload carries the structured
attachments), apply the recorded turn (items, footer with a changed file and a Diff link), then
Restore the file with `postRestore` stubbed to `restored`.

## offer-handlers.ts

Offer labels come from plan 2's `OfferView.label`; the opening message's label from plan 8's
`SessionBriefView.label`. No label table here.

## session-divider.tsx, rebuilt-summary-extras.tsx

`<SessionDivider previous next />` renders between the snapshot's `previous` chat and the current
one: "Chat ended at <HH:MM>" (plan 11 adds a pressure-step branch; keep it one `switch`).
`<RebuiltSummaryExtras id turn />` ends each rebuilt-summary row and returns `null` (plan 10
adds the per-summary tokens). Keep both props exactly.

## Required test cases (coverage audit; named tests)

`turn-footer.test.tsx`: "duration and token tooltip"; "rows and diff link"; "running turn
disables diff"; "by_tool false note"; "plans under a changed section grouped by state"; "no files
no list"; "rebuilt rows states"; "check these plans fills composer and attaches section and
plans"; "plan a follow-up fill"; "viewer sees no fill actions". `restore.test.tsx`: "viewer sees
no restore"; "hidden while working"; "no restore on new or deleted rows"; "changed opens merge
view"; "invalid asks save with errors and resends force". `start-chat.test.tsx`: "offer start";
"new chat start"; "first message start"; "starting opens panel and shows brief view"; "brief
dialog read-only"; "end current chat first"; "lost race message"; "end chat confirms then ends";
"end chat hidden for pressure sessions"; "viewer has no end chat, brief opens". Session divider
(in `start-chat.test.tsx`): "Chat ended at HH:MM". `offer-handlers.test.ts`: "every authoring kind
registered", "handler sends start with the offer id", "pressure_test and start_run left to plan
11".

## Fix-round additions (pressure test px-seams-03, px-browser-20)

- Diff links: `openDiff("turn", docRel(file.path), `${session.id}/${turn.id}`)` (the full turn id
  `<session>/t<n>`); plan 3 encodes it. `postRestore` sends the footer's repo-relative paths; the server converts.
- The brief fetch while Starting retries once a second at most 30 times, then shows "Brief not
  available" (plan 2's `LoadState`); Restore and the fills show a failed request inline with its
  reason.
- Tests: "footer diff link carries the session/turn key and a doc-relative path"; "brief retry
  stops after 30 tries"; "restore request failure shows the reason".
