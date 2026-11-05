# C1: the chat panel is always shown on wide screens (issue 8, D7; issue 5 behaviour, D9)

Tier: sonnet (`loom-software-engineer`). Read `common.md` D7 and D9, and the "Chat home and the chat panel" section of `doc/loom/knowledge/architecture/planner-web-shell.md`. V3 (fable) redesigns the sidebar in parallel. You change behaviour and markup only, with no CSS. V3 may send you head markup changes through the main agent.

## Build

1. Remove the collapse:
   - in `chat/prefs.ts`, `panelCollapsedAtom` and `collapsedAtom`;
   - in `chat/panel.tsx`, `CollapsedRail` and its branch in `ChatPanel`;
   - in `chat/panel-head.tsx`, the Collapse chat button (keep Close chat in the sheet);
   - every `store.set(panelCollapsedAtom, false)` caller: `chat/offer-flow.ts:106`, `home/move-into-feature.tsx:69`, `attachments/add-to-chat.tsx:55` and `chat/start-chat.tsx` (`rg -n panelCollapsedAtom web/src` lists them).
   - On narrow screens, Add to chat and an offer still open the sheet as they do today (`chatSheetOpenAtom`).
   - A stored `panel-collapsed` value is ignored; no migration (CLAUDE.md: no backwards compatibility).
2. In `chat/offer-flow.ts`, `DESKTOP` becomes `(min-width: 1280px)`. `ChatSheetButton` and `header-extras.tsx` show the Chat button below that width. Check that the header's own 1024px layout switch does not read `DESKTOP` (`rg -n DESKTOP web/src/plan`), and report any consumer whose meaning changes.
3. Chat menu (D9), in `chat/panel-head.tsx` `ChatMenu`:
   - Escape closes the `<details>` and returns focus to its summary;
   - a pointer-down outside closes it;
   - choosing End chat closes it.
   - Keep the `<details>`, the summary's `aria-label="Chat menu"`, and `ChatMenu` returning null without `useCanEndChat()`.
4. Tests:
   - rewrite the collapse tests in `chat/panel.test.tsx` (lines 121-140, 236-238, 345-360) to prove there is no rail: a seeded `panel-collapsed: true` still renders the full panel at a wide viewport;
   - update `panel-header.test.tsx`, `panel-a11y.test.tsx` (its mocks of `./composer`, `./run-header` and `./end-chat` must match the exports), `prefs.test.ts`, `move-into-feature.test.tsx` and `add-to-chat.test.tsx`;
   - add a menu test for Escape and outside-click.
   - `matchMedia` stubs that match `min-width` queries stay valid at 1280px.

## Contract (written before you start; make it pass, do not edit it)

`web/src/plan/chat/polish.contracts.test.tsx`, "the chat panel has no collapse at a wide viewport".

## Files

Your row in the plan's worker table. Never edit V3's files.

## Run once at the end

`node_modules/.bin/vitest run src/plan/chat src/plan/home/move-into-feature.test.tsx src/plan/attachments src/plan/new`, from `web/`.
