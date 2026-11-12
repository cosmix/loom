# W2: chat web, quieter turns and the agent-ended card (codex terra, high)

Read `common.md` beside this file first. Decision D7 and the web half of D6 are yours.

## Files you own

- `web/src/plan/chat/turn-footer.tsx` and `turn-footer.test.tsx`
- `web/src/plan/chat/transcript.tsx` and `transcript.test.tsx`
- `web/src/plan/chat/chat.contracts.test.tsx`
- `web/src/plan/chat/closing.ts` and `closing.test.ts`
- `web/src/plan/chat/kickoff-fence.ts`
- `web/src/plan/chat/wrap-up.tsx`
- `web/src/plan/chat/state-line.tsx`
- `web/src/plan/chat/chat-tools.css`

## Steps

1. Issue 3, `turn-footer.tsx`. Delete the `{!file.by_tool && <span>changed during this turn</span>}`
   label (line 98) and anything only it used. Wrap the `ChangedFiles` list (about line 150) in a
   `<details>`, closed by default. Its `<summary>` is the existing "Changed N file(s)" header text.
   Keep each row's diff button and its disabled state while the turn runs. Update
   `turn-footer.test.tsx`: delete the "by_tool false note" case (lines 91 to 95), and open the
   `<details>` before asserting rows.
2. Issue 4, `transcript.tsx` `FoldedTools` (lines 77 to 96). Drop `open={turn.open}` so the
   `<details className="chat-tool-fold">` is uncontrolled and closed, also while the turn runs. The
   summary label from `groupToolRows` is unchanged. Error and denied rows and Agent rows stay
   outside the fold as today. Update `transcript.test.tsx` lines 295 to 317 (a running turn's card
   is now closed; a click opens it and it stays open while the turn runs) and
   `chat.contracts.test.tsx` around line 241.
3. Issue 14, web half.
   - `closing.ts`: add `DONE_INFO = "loom-done"` and `doneLists(turn)`, which reads the final
     assistant text's `loom-done` fence with the same zod limits as the closing block (`fenceBody`
     from `kickoff-fence.ts`). Only the final block of the last assistant text counts, as in
     `common.md`.
   - `leftBehind(turns)` (about line 85): with no closing turn, take the decisions and questions
     from the last turn's done fence.
   - Add `endedByAgent(turns): boolean`, true when the last turn carries a valid done fence and
     there is no closing turn.
   - `kickoff-fence.ts`: hide a `loom-done` fence from the rendered reply text the way the
     `loom-closing` fence is hidden (read how `loom-closing` is hidden first).
   - `transcript.tsx` `ClosingCard` (about line 200) or the ended-chat view. For a chat
     `endedByAgent`, show `LeftBehindCard` with its decisions and questions under a heading "The
     planner finished this chat". Never show `WrapUpCard`.
   - `state-line.tsx` (lines 44 to 49): an ended chat whose turns are `endedByAgent` reads "Finished
     by the planner", in the same slot the other ended texts use.
4. Tests in `closing.test.ts`:
   - fence at the end;
   - text after it;
   - info string `loom-done-later`;
   - over-long lists, refused whole;
   - a user item holding the fence, which is ignored;
   - `leftBehind` falling back to the fence.

   Add one transcript test: an agent-ended chat shows the card and no raw fence.

## Traps

- Do not change the wire or `schema-chat.ts`. The server needs no new field: the web reads the
  fence from the turns it already receives.
- `userWrote` in `closing.ts` mirrors the server rule for user ends. Leave it unchanged.

## Done

`cd web && node_modules/.bin/tsc -b --noEmit` is clean, and `node_modules/.bin/vitest run
src/plan/chat` passes if vitest runs for you. Report the files changed.
