# C1: End chat only while live; openings show their label (#3, #4)

Tier: haiku (`loom-software-engineer` with model haiku). These are two small, fully specified fixes, each pinned by a frozen contract. Apply them exactly. Read `../common.md` first. Knowledge: `doc/loom/knowledge/architecture/planner-web-shell.md` ("Chat home and the chat panel").

## #3: a live chat's menu offers End chat only (operator decision)

- `web/src/plan/chat/panel-head.tsx` `ChatMenu` (~L41-58) renders `{canEnd && <EndChatMenuItem />}` and `{canStart && <NewChatButton />}`, where `canStart = isActiveSession(session) && canAuthor && !readOnly`. The comment above it says it carries New chat beside a running chat.
- Change: the menu holds End chat only. Remove `canStart` and the `NewChatButton` from the menu, and rewrite the comment. Then drop the imports that become unused in `panel-head.tsx` (`isActiveSession`, `canAuthorAtom`, `serverInfoAtom` if nothing else uses them): the lint gate is `oxlint --deny-warnings`.
- New chat stays where it already is: the head button when there is no session (`{session ? <StateWord .../> : <NewChatButton />}`) and the concluded composer area (`composer.tsx` `ConcludedChat`).
- The menu renders nothing when End chat is not allowed. Read-only behaviour is unchanged.
- `start-chat.tsx` `beginChat` is private and keeps its end-then-start branch. Offers on other screens do not use it: `offer-flow.ts` ends a chat through its own `endCurrentChat`. After this change no production mount reaches that branch (New chat is drawn only with no active session); leave it in place with a one-line comment saying so, because `start-chat-brief.test.tsx` (not yours) still drives it.
- Pinned tests: `chat/panel-a11y.test.tsx` (about lines 157-177) pins "menu offers New chat while a chat runs, not after it concluded". Rewrite it to the new rule. `chat/session-home.test.tsx` (about lines 308-310) also expects New chat beside a live chat: rewrite it. Run `rg -n 'New chat' web/src/plan/chat/*.test.tsx` and fix every test that expects New chat inside the menu.
- The frozen contract renders the real `useCanEndChat` (holder, active session, non-pressure kind) and also asserts that no "New chat" button appears anywhere in the head while a chat is live.

## #4: an opening shows its label until Show

- `web/src/plan/chat/items.tsx` `UserItem` (~L140-182) collapses an item with an `origin` to its first line: `shown = collapsed ? first line : item.text`. For a New chat that first line is "loom plan: no step chosen.", which reads as junk.
- Change: while collapsed, an origin item draws only its label row (`OpeningLabel` for an opening, "Closing message from loom plan" for a closing) and the Show button. The prompt text is not in the DOM while collapsed. Keep the `<p id={bodyId}>` element so `aria-controls` points at something, but render it empty until expanded. A `hidden` paragraph that still holds the text fails the contract, because text queries find hidden text.
- The duplicate opening (#4's "twice") is fixed on the server by B1 in the backend stage (Claude Code's `<pasted_content>` wrapper defeated the sent-item match). Do not add a client-side dedupe.
- Update `chat/items.test.tsx` and any transcript or replay test that expected the first line of an origin item: `rg -n 'no step chosen|Show' web/src/plan/chat/*.test.tsx web/src/plan/help`.
- The frozen contract renders `ItemView` with `{ item, turn }`; before Show it expects the label "Opening message" (no brief loaded) and no prompt text, and an element with the id the Show button's `aria-controls` names; after Show it finds the prompt by regex.

## Also yours

`web/src/plan/help/pressure-scene.tsx` (about line 98) carries a comment saying "RunStrip mounts DashboardButton". V2 removes `RunStrip` in this stage: rewrite the comment to name `DashboardButton` alone, or drop the sentence.

## Files you own

`web/src/plan/chat/panel-head.tsx`, `chat/panel-a11y.test.tsx`, `chat/items.tsx`, `chat/items.test.tsx`, `chat/transcript.test.tsx`, `chat/replay.test.tsx`, `chat/start-chat.test.tsx`, `chat/session-home.test.tsx`, `web/src/plan/help/**`.

## Contracts (frozen before you start; make them pass)

`web/src/plan/chat/chat-fixes.contracts.test.tsx`:

- "a live chat's menu offers End chat and not New chat"
- "an opening shows its label and hides the prompt until Show"

The same file holds V2's "the composer has a visible border"; it is not yours.

## Proof

Run once at the end: `node_modules/.bin/vitest run src/plan/chat src/plan/help` from `web/`.
