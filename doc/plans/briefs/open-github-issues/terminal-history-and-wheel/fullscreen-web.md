# Brief: fullscreen-web (#35 labels)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

`web/src/components/terminal/history-labels.ts` (new), `web/src/components/terminal/terminal-glyph.tsx`, `web/src/routes/stage.tsx`, `web/src/components/stage-modal.tsx`, `web/src/components/terminal/history-view.tsx`, `web/src/components/attention-blocked.tsx`, `web/src/routes/terminal.tsx`, `web/src/api/schema.ts`, and the tests `web/src/components/terminal/terminal-glyph.test.tsx`, `web/src/components/stage-modal-history.test.tsx`, `web/src/components/terminal/history-view.test.tsx`, `web/src/components/attention-panel.test.tsx`, `web/src/api/schema.test.ts`.

Frozen, read-only: `web/src/components/terminal/history-labels.contract.test.ts`. Read only: `web/src/api/fixtures/snapshot.json` (the Rust worker adds `terminal_history_kind` there).

## Wire (pinned)

- Stage summary: `terminal_history_kind?: "history" | "last_screen"`, present exactly when `terminal_history_session` is present.
- Attention entry: `terminal_history_kind?: "history" | "last_screen"`, present when `terminal_history` is true.

## Change

1. `history-labels.ts`: `HistoryKind` and `historyLabels(kind)` exactly as the stage PUBLIC SURFACE lists. The `"history"` strings are today's literals, collected from: `routes/stage.tsx:59,62` (aria "open terminal history", text "Terminal history"), `stage-modal.tsx:358-364` (title "The session ended; its saved screen opens read-only"), `history-view.tsx:112,139,163,40-41` (aria "Saved terminal history", eyebrow "Terminal history", footer "read-only · the session ended · this is its saved screen", notices), `attention-blocked.tsx:41` ("View terminal history").
2. `terminal-glyph.tsx`: `terminalHistory(terminals, stage)` returns `{ session: string; kind: HistoryKind } | null` (kind defaults to `"history"` when absent). Update its callers.
3. Every place in step 1 takes its strings from `historyLabels(kind)`. `history-view.tsx` receives the kind as a prop; `routes/terminal.tsx` passes it.
4. `schema.ts`: add the two optional fields (enum) to `stageSummarySchema` and the attention entry schema; both stay `.strict()`.

## Tests

Update the listed tests for the new helper shape; add one case per surface where `last_screen` shows "Last screen" wording and no "history" text. Keep the existing history assertions.

## Done

`cd web && ./node_modules/.bin/vitest run src/components/terminal src/components/stage-modal-history.test.tsx src/components/attention-panel.test.tsx src/api/schema.test.ts src/routes` passes and `./node_modules/.bin/tsc -b --noEmit` is clean. `schema.test.ts` may fail until the Rust worker's fixture lands; say so in your report if it does.

## Traps

- Do not change `historyCommand` (`less -R .loom/work/terminal-history/<id>.terminal`): the file path is the same for both kinds.
- No label for `last_screen` may contain "history" in any case.
