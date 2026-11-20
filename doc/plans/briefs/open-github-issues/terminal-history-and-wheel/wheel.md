# Brief: wheel (#41 trackpad paging)

Tier: `loom-software-engineer` (sonnet). Read `doc/plans/briefs/open-github-issues/common.md` first.

## Owns

- `web/src/components/terminal/viewer-wheel.ts`
- `web/src/components/terminal/viewer-wheel.test.ts`

Read only: `web/src/components/terminal/viewer-touch.ts`, `emulator.ts`, the frozen `web/src/components/terminal/viewer-wheel.contract.test.ts`.

## Today (start from `installViewerWheel` and `createViewerPager` in `viewer-wheel.ts`)

- `onWheel` returns at line 58 when `!disableStdin || ctrlKey || deltaY === 0`, before `preventDefault()`.
- PIXEL deltas convert to lines with `terminalLineHeight` (screen height / rows, 18 px fallback in jsdom).
- `createViewerPager.move` (lines 26-45) zeroes the remainder on a sign change, sends `pages = clamp(trunc(remainder / rows), -1, 1)` on the alternate screen and keeps `remainder % rows`. `reset()` exists but the wheel path never calls it.
- A full viewport per page is about 750 px, which a trackpad swipe never reaches.

## Change

1. Gesture grouping in `installViewerWheel`: keep `lastTime` (from `event.timeStamp`). When `event.timeStamp - lastTime > 250`, call `pager.reset()` before moving. Export `GESTURE_GAP_MS = 250`.
2. Lead distance in the alternate-screen branch of the pager: within a gesture the first page needs `max(3, Math.ceil(rows / 4))` lines; each later page needs `rows` lines. At most 3 pages per gesture (count resets with `reset()`), at most one page per `move` call. Export `MAX_PAGES_PER_GESTURE = 3`.
3. Reversal: a sign change still resets the remainder and the gesture's page count, so the reversal test keeps passing.
4. `deltaY === 0` in View mode on the alternate screen: call `preventDefault()` and `stopPropagation()`, then return without moving. Ctrl-wheel (pinch) and Control mode stay untouched.
5. The normal-buffer branch (local `scrollLines`) is unchanged.
6. `viewer-touch.ts` shares the pager: it calls `reset()` on `touchstart`, which must keep working. Its tests (`viewer-touch.test.ts`) must still pass: a touch swipe uses the same lead rule, so re-check `accumulates moves within a swipe` expectations by running the file.

## Tests

Update `viewer-wheel.test.ts`: the existing tests pin the old full-viewport pixel threshold (test at line 44) and the line threshold (line 61). Rewrite only the expectations that the lead rule changes; keep these assertions as they are: a single line-mode notch sends no page, a reversal resets, the normal buffer scrolls locally, Control mode is not prevented. Add: deltaY 0 is prevented in View mode; a 400 ms pause resets the remainder. Set `timeStamp` with `Object.defineProperty(event, "timeStamp", { value: t })`.

## Done

`cd web && ./node_modules/.bin/vitest run src/components/terminal/viewer-wheel.test.ts src/components/terminal/viewer-touch.test.ts src/components/terminal/viewer-wheel.contract.test.ts` passes, and `./node_modules/.bin/tsc -b --noEmit` is clean.

## Traps

- The contract file is frozen; never edit it. Its tests use 45 rows and the 18 px fallback.
- A single slow mouse notch (40 px, about 2.2 lines) must not page: the lead is 12 lines at 45 rows.
