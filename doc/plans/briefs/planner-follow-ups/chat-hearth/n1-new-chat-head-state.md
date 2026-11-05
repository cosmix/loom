# N1: lazy New chat, a head with no menu, no idle line, the composer's end states (R13, web behaviour)

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first, then the "Chat home and the chat panel" section of `doc/loom/knowledge/architecture/planner-web-shell.md` and "Chat Panel and Review UI Rules" in `conventions/plan-web.md`. Wave 1, beside B1 (Rust) and T1 (transcript). Behaviour and semantic markup only: write plain class hooks named below and no new CSS. The fable workers of wave 2 set the look.

## Seams with the other workers

- T1 writes `web/src/plan/chat/shown-session.ts` first: `export const clearedChatAtom = atom<string | null>(null)` (the id of a concluded chat the user moved past with New chat), and `shownSessionAtom` returns null for a concluded chat whose id equals it. Import it from `./shown-session`; if it is not there when you need it, report it and stop that step.
- T1 renders the starting state inside the transcript (`StartingCard`, the pane's one roundel while starting) and imports `StartingBrief` from `./start-chat`. You remove `StartingBrief` from `state-line.tsx` and keep exporting it from `start-chat.tsx`.
- Frozen `polish.contracts.test.tsx` mocks `./start-chat` (only `NewChatButton`, `StartingBrief`), `./end-chat` (`EndChatMenuItem`, `useCanEndChat`), `./run-header` (`RunHeader`), `./transcript` (`Transcript`), `./attention` (`useAttention`) and `@/plan/review/changed-since` (`ChangedSinceControl` only). Code that `panel.tsx`, `panel-head.tsx`, `composer*.tsx`, `state-line.tsx` or `empty-chat.tsx` loads may import only those names from those modules: vitest throws on a missing export of a factory mock, and the frozen contract would fail.

## Build

1. **New chat starts nothing** (`start-chat.tsx`). `beginChat` keeps the end-then-continue guard for a live chat (`endCurrentChat`, unchanged copy and confirm through `useNewChat`), then, instead of `startChat(store, null, null, [], ...)`: set `clearedChatAtom` to the id of the chat now shown (`store.get(shownSessionAtom)?.id ?? null`), set `composeAfterAtom` to null, set `focusComposerAtom` to true, and `revealChat(store)`. No chat command is sent. The first Send then starts the session through the existing path (`composer-hooks.ts` `useSendDraft`: no active session means `startChat(store, null, text, attachments, feature, plan)`). Delete the now unused `startChat` import.
2. **`NewChatButton({ variant })`**, `variant: "head" | "concluded"`:
   - `head`: rendered only when `useCanEndChat()` (holder, active session here, not pressure); an icon-only round button (lucide `SquarePen`, 15px, `aria-hidden`) whose accessible name is exactly "New chat" (sr-only text inside the button, so `DisabledReason` keeps its reason line); `look="quiet"`.
   - `concluded`: the existing button with the visible label "Start a new chat" and `look="primary"`.
   - The error line and the read-only rules stay as now for both.
3. **Head** (`panel-head.tsx`). Delete `ChatMenu`, `useDetailsDismiss`, `StateWord` and the `MoreHorizontal` import. Markup: `header.chat-head` holding `div.chat-head-text` (the `h2.chat-head-title`, unchanged attributes, then `p.chat-head-sub`) and `div.chat-head-side` (`<NewChatButton variant="head" />`, then Close chat in the sheet). With no session shown the title reads "New chat" and the `h2` carries `data-new`. The sub-line is `<StateLine />` followed by the place the chat concerns, joined by " · " only when both have text: the session home's title (`sessionHomeAtom`) when a session is shown, else the viewed document's name (`contextDocumentAtom`, `visibleControls` applied), else nothing.
4. **`end-chat.tsx`**: delete `EndChatMenuItem`; keep `useCanEndChat`.
5. **State line** (`state-line.tsx`), now the head's sub-line:
   - Visible words per state: starting "Starting", working "Working", permission the warning glyph (`⚠`, `aria-hidden`, `--tone-warning` as now) and "Needs your answer", ending "Wrapping up", ended "Ended", failed "Stopped". Idle and no session: no visible element at all.
   - The polite live region stays mounted for every state and keeps announcing `stateWords` as now, except idle, which announces nothing (`""`). `announcedElsewhere` is unchanged.
   - Delete `workingDetail`, the age line of the ended state, `StateRow`'s `StartingBrief` branch, and `StateGlyph`'s roundel use: the line draws no roundel (common.md: one roundel per pane, on what is working). Move `Failure` (and its `Notice`) out to T1: export it as `FailureNotice({ session })` from a new `web/src/plan/chat/failure-notice.tsx` (yours), with the same text; T1 renders it at the transcript's end. Keep the root `.chat-state-line[data-state][data-kind]`.
   - Keep `STATE_NAME` in `chat-state-glyph.tsx` unchanged: the header's Chat button and the kickoff head use it.
6. **Run line** (`run-header.tsx`): `RunHeader` returns `<PressureHeader session={session} />` for a pressure session and null otherwise. Delete the authoring line (holder, watchers, tokens) and its imports. `.chat-run-header` stays on the pressure head.
7. **Empty panel** (`panel.tsx`, new `empty-chat.tsx`). In `SessionBody`, when `shownSessionAtom` is null render `<EmptyChat />` in `.chat-panel-middle` in place of `<Transcript />`. `EmptyChat` (`section.chat-empty`, `aria-labelledby` its heading): `h3.chat-empty-title` "Good morning. What are we deciding?" (morning before 12:00, afternoon before 18:00, else evening, by the local clock at render) and `p.chat-empty-lede` "I'll read the documents beside you and write down what we settle. Your first message starts the session." `ContextPrompts` stays where it is in `ComposerArea` (wave 2 places it).
8. **Composer views** (`composer.tsx`; it is at 379 lines, so move `ConcludedChat`, `NewChatBesideBox` and `DraftKept` into a new `composer-concluded.tsx` first). `View` gains `"ending"`: while the shown session is `ending`, the composer area renders `p.chat-composer-ending` "The composer comes back with your next chat." and `DraftKept`, and no input. The concluded view renders `p.chat-composer-ended` "This chat has ended." beside `<NewChatButton variant="concluded" />`. `NewChatBesideBox` uses `variant="concluded"`.
9. **Help text**: `web/src/plan/chat/stop-control.tsx`'s comment says "after Stop or when the chat ends" instead of naming End chat.

## Tests

- Update what your change breaks: `start-chat.test.tsx`, `start-chat-brief.test.tsx`, `start-chat-test-kit.tsx`, `panel-header.test.tsx`, `panel.test.tsx`, `panel-a11y.test.tsx` (its mocks of `./run-header`, `./end-chat`, `./composer` must match the exports), `panel-offers.test.tsx`, `state-line.test.tsx`, `state-line-header.test.tsx` (the run-line tests go with the run line), `state-line-kit.tsx`, `pressure/seams.test.tsx` ("RunHeader switches ..." now proves the authoring branch renders nothing), `composer.test.tsx`, `composer-concluded.test.tsx`, `composer-delivery.test.tsx`, `session-home.test.tsx` (line ~79 pins "Waiting for you"). Delete `panel-head-menu.test.tsx`.
- Add: New chat on a concluded chat sends no command and focuses an empty composer; New chat on a live chat sends `end` and no `start`; the ending view hides the input; the head's sub-line shows "Needs your answer" for a permission state.
- Frozen, planned (`chat-fixes.contracts.test.tsx`): delete the test "a live chat's menu offers End chat and not New chat" and the imports only it used (`PanelHead`, and `within` if nothing else uses it); in "an opening shows its label and hides the prompt until Show" change the `item(...)` call's `origin: "start"` to `origin: "offer"` and nothing else.
- If `help/chat-scene.test.tsx` "draws a single warning glyph, inside the state line" breaks, update it to the new words; V2 owns the scene's pins in wave 2.

## Contracts (written before you start; make them pass, do not edit them)

`web/src/plan/chat/new-chat.contracts.test.tsx`: "New chat on an ended chat starts no session until the first send", "New chat on a live chat ends it and starts nothing". `web/src/plan/chat/hearth.contracts.test.tsx`: "a live chat head has no menu and no End chat", "the state line draws nothing for an idle chat".

## Run once at the end

`node_modules/.bin/vitest run src/plan/chat src/plan/help/chat-scene.test.tsx src/plan/new`, from `web/`. T1 works in `src/plan/chat` at the same time: a failure in T1's files (`transcript*`, `items*`, `tool-row*`, `turn-footer*`, `wrap-up*`, `closing*`) is theirs; report it, do not fix it.

## Report

Files changed; the test result; every pinned or deleted test by file and name; every importer outside your files that a removed export (`EndChatMenuItem`, `StateGlyph`'s roundel, `RunHeader`'s authoring line) still reaches.
