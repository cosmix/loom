# T1: the transcript's ending, one roundel per pane, no prompt, tokens or "Thought for" (R13, web behaviour)

Tier: sonnet (`loom-software-engineer`). Read `../common.md` first, then the "Chat home and the chat panel" and "Roundel and busy text" sections of `doc/loom/knowledge/architecture/planner-web-shell.md`, and "Chat Panel and Review UI Rules" and "Planner Polish Rules" in `conventions/plan-web.md`. Wave 1, beside B1 (Rust) and N1 (head, state line, composer). Behaviour and semantic markup with the class hooks named below; no new CSS (V1, fable, sets the look in wave 2).

## First, before anything else

Write `web/src/plan/chat/shown-session.ts`'s new export, because N1 imports it:

```typescript
/** The concluded chat the user moved past with New chat: its state and turns are no longer shown. */
export const clearedChatAtom = atom<string | null>(null);
```

and make `shownSessionAtom` return null when the concluded session it would return (from `session` or from `previous`) has that id. An active session is never cleared.

## Settled decisions

- **The closing turn is never drawn as a turn.** A closing turn is a turn holding a user item with `origin === "closing"`. Its prompt, its assistant text (`Closing write done.` and the `loom-closing` block), its tool rows and its turn footer are not rendered; the turn renders as a card instead (below). `items.tsx` `UserItem` keeps its origin label and Show only for `origin === "offer"`; delete the "Closing message from loom plan" branch.
- **The closing data** (new pure module `web/src/plan/chat/closing.ts`, unit-tested in `closing.test.ts`):
  - `isClosingTurn(turn: Turn): boolean`.
  - `userWrote(turns: Turn[]): boolean`: a user item with `origin === null` and `delivery !== "undelivered"` (the server's rule, B1).
  - `closingTicks(turn: Turn): Array<{ path: string; state: "done" | "live" }>`: the closing turn's tool items named `Edit`, `Write` or `MultiEdit` with status `pending` (live) or `done`; the path is the item's `summary` (the server makes it repo-relative, `transcript/tools.rs` `summary`); one entry per path, first appearance order, a path done once any of its items is done.
  - `leftBehind(turns: Turn[]): { documents: Array<{ path: string; added: number | null; removed: number | null }>; decisions: string[]; questions: string[]; closed: boolean }`: documents are the union of every turn's `changed_files` paths, first appearance order; `added`/`removed` sum the `added`/`removed` of the chat's done `Edit`/`Write`/`MultiEdit` items whose `summary` equals the path (null when none carries counts); `decisions`/`questions` come from the last `loom-closing` block in the closing turn's text items, parsed with `JSON.parse` in a try and a non-strict zod object `{ decisions: z.array(z.string()).max(50), questions: z.array(z.string()).max(50) }` (import `z` from `@/api/zod`), entries trimmed and empty ones dropped; anything unparsable gives empty lists. `closed` says a closing turn exists.
- **Fence helper.** Generalise `kickoff-fence.ts`: export `withoutFence(markdown: string, info: string): string` and `fenceBody(markdown: string, info: string): string | null` (the body of the last fence with that exact info string, null when none or still open), keeping the existing scanning rules; `withoutKickoffFence(markdown)` stays as `withoutFence(markdown, "loom-kickoff")`. Extend `kickoff-fence.test.ts`.
- **Cards** (new `web/src/plan/chat/wrap-up.tsx`, semantic markup, plain class hooks):
  - `WrapUpCard({ ticks })`, `div.chat-wrap[role=status][aria-live=polite]`: a head with the pane's roundel (`RoundelGlyph`, size 22, inside `span.chat-wrap-mark`), `h3` "Writing down what we decided" and `p` "Putting this chat's decisions and open questions into the documents."; then `ul.chat-wrap-ticks` with one `li[data-state=done|live]` per tick holding `span.chat-wrap-tick` (aria-hidden; a lucide `Check` when done) and the path in `span.chat-wrap-path`.
  - `LeftBehindCard({ summary })`, `section.chat-left-behind` labelled by its `h3` "Here's what this chat left behind": `ul.chat-left-behind-counts` (one `li` per non-zero count: `plural(n, "document")` + " changed", `plural(n, "decision")`, `plural(n, "open question")`, `ui/plural.ts`; a `span[aria-hidden]` glyph `✓` before the first two and `?` before questions), `ul.chat-left-behind-docs` (each `li`: `i[aria-hidden]` dot, the path, and `+added −removed` when known), `h4` "Decided" over `ol` of decisions, `h4` "Still open" over `ul.chat-left-behind-questions`, and a `Link` "See the changes" to `CHANGES_PATH` (`@/plan/review/changed-since`). Sections with nothing in them are not drawn. Every string from the model renders through `<Untrusted>`.
  - `StartingCard({ session })`, `div.chat-starting`: `RoundelGlyph` (size 16) and `StartingBrief` from `./start-chat` (N1 rewrites its words to "Starting the session" and a fine line with the name, model, effort and the Brief link).
- **Where they render** (`transcript.tsx`):
  - `TranscriptTurn` of a closing turn renders `WrapUpCard` with `closingTicks(turn)` while `turn.open`, else `LeftBehindCard` with `leftBehind(<all turns of that chat>)`.
  - At the end of `.chat-transcript-flow`, a new `ChatOutcome`: while the shown session (`shownSessionAtom`) is `ending`, `userWrote(turns)` and no closing turn exists yet, `WrapUpCard` with no ticks; while it is `starting`, `StartingCard`; when it is `failed`, N1's `FailureNotice` (`./failure-notice`); when it is `ended`, `userWrote` holds, no closing turn exists and `leftBehind(...).documents` is not empty, `LeftBehindCard` (a chat whose closing write was cut off still shows its documents).
  - Hide what New chat cleared: `TurnList` drops `previous` when `previous.session.id === clearedChatAtom`, and drops the current turns when the session is concluded and its id equals it.
- **One roundel per pane** (common.md, user decision). While the shown session is `working`, the newest open turn that is not a closing turn ends with `div.chat-turn-activity` holding `RoundelGlyph` (size 16) and an sr-only "Working". Nothing else in the transcript draws a roundel: `FoldedTools` loses its `chat-tool-fold-busy` glyph; `tool-row.tsx` `ToolStatus` draws no `RoundelGlyph` for `pending` (the word stays; V1 adds the shimmer); `turn-footer-rebuilt.tsx` loses the `ActivityGlyph` before "N summaries rebuilding". The only roundels left in the pane are `chat-turn-activity`, `WrapUpCard` and `StartingCard`, which never show together. Pressure sessions are out of scope for this rule.
- **No tokens, no "Thought for".** `items.tsx` `ThinkingItem` and `transcript.tsx` `mergeThinking` go: a thinking item renders nothing. `turn-footer.tsx` `TurnStats` goes (duration and "fresh tokens"). `transcript.tsx` `turnLabel` reads `Turn N · <plural(n, "file")> changed`. `turn-footer-rebuilt.tsx` stops rendering `RebuiltSummaryExtras`; delete `rebuilt-summary-extras.tsx` and its test, and delete `useTurnTokens` from `web/src/plan/summaries/state.ts` (and its tests) when `rg -n useTurnTokens web/src` shows no other importer.
- `session-divider.tsx` stays as is (V1 restyles it).

## Tests

Update `transcript.test.tsx` (the "Thought for" cases and "closing origin labelled Closing message from loom plan" become tests of the new truth), `items.test.tsx`, `tool-row.test.tsx`, `turn-footer.test.tsx` (the "fresh tokens" case), `transcript-test-kit.tsx` if needed, `replay.test.tsx` if the fixture's origin change (B1: the `start` item of `chat-snapshot.json` becomes `origin: null`) breaks it. Add: `closing.test.ts` (ticks, union and counts, block parsing with a malformed block and a missing one); `wrap-up.test.tsx` (each card's sections, untrusted text, empty sections not drawn); a transcript test that the cleared previous chat is not rendered.

## Contracts (written before you start; make them pass, do not edit them)

`web/src/plan/chat/hearth.contracts.test.tsx`: "an ended chat shows what it left behind and never the closing prompt", "a working chat draws one roundel and an idle chat none". The frozen `chat.contracts.test.tsx` "chat ended turn folds three consecutive tool rows and keeps an error row outside" keeps its label "Used 3 tools · edited 2 files"; the turn footer group "Turn 1 changes" keeps its Diff links and Restore all.

## Run once at the end

`node_modules/.bin/vitest run src/plan/chat src/plan/summaries src/plan/new src/plan/help/chat-scene.test.tsx`, from `web/`. N1 works in `src/plan/chat` at the same time: a failure in N1's files (`panel*`, `state-line*`, `start-chat*`, `composer*`, `run-header*`, `end-chat*`) is theirs; report it.

## Report

Files changed; the test result; every pinned or deleted test by file and name; the final class hooks of the three cards and `chat-turn-activity`, for V1.
