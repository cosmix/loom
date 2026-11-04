# V5 — Changed since you looked becomes a wide page (issue 12)

Tier: opus, effort medium (`loom-senior-software-engineer`). First invoke `frontend-design:frontend-design` with the Skill tool. Then read `common.md` (Design rules, Visual work), `web/src/plan/ui/DESIGN.md`, `doc/loom/knowledge/architecture/planner-web-shell.md` ("Changed since and seen text"), and this brief.

## Today

`web/src/plan/review/changed-since.tsx` `ChangedSinceWidget` (~270-305):

- a Radix `Dialog`, `DialogContent className="changed-since"`, at most `min(1120px, 100vw - 2rem)` wide;
- opened from `RowTrigger`, a button in the activity popover's footer, through `ChangedSinceControl` (~308-315);
- the body is `ComparisonList`, a list column (about 44%, at least 420px) beside `SelectedDiff`.

The data hooks are in `changed-since-data.ts`.

## Fixed targets

- **Route** `{ path: "changes", … }`: a child of `/` in `web/src/plan/router.tsx`, rendering a new `review/changes-page.tsx`. `dock.tsx` `hasChatPanel` already returns true for `/changes`, so `dock.tsx` needs no edit. `shell.test.tsx` lists the routes in one array: add `/changes` there.
- **Page:**
  - an H1 "Changed since you looked" and the one-line lede of what it compares (kept from the dialog);
  - then `ComparisonList` across the page's full width: the list a fixed 22rem column at 1024px and up, the diff filling the rest;
  - the diff's side-by-side view gets the width (`diff/` views unchanged);
  - under 1024px, the list, then the diff below the selected row;
  - "Mark all seen", the reference form and the limits stay in the page foot.
- **Trigger.** `ChangedSinceControl` keeps its export and its accessible name "Changed since you looked · N". It stays a `<button>` (the frozen `chat/chat.contracts.test.tsx` "chat changed since you looked…" finds it with `findByRole("button", …)`, clicks it, and expects the changed documents) whose click calls `navigate("/changes", { state: { from: <current location> } })`, still drawn as the popover footer row (`activity-row` classes). It closes the popover, and the popover must not return focus to the pill afterwards (`header/activity.tsx` `PillContent`'s `onCloseAutoFocus` would undo the focus on the page heading: prevent it for this close; `activity.tsx` is the H1 worker's file, so report that one-line change to the main agent). It renders nothing when there are no changes.
- **Focus.** On arrival at `/changes`, focus goes to the H1. After "Mark all seen" empties the list, the page says "Nothing changed since you looked", with a link back to where the reader came from (`location.state.from`, set by the trigger), else Home. `focusReadingRegion` and the dialog's `onCloseAutoFocus` go. Remove what that orphans.
- **Tests:**
  - `review/changed-since-test-kit.tsx`: `mount` renders `/changes` in a memory router, and `openList` resolves once the list "Changed documents" is present. Keep the kit's exports, so the frozen `review/changed-since-fixes.contracts.test.tsx` holds unchanged.
  - Update `review/changed-since.test.tsx` (the dialog, Escape and focus-return tests become page tests), `changed-since-mark-all*.test.tsx`, `changed-since-seen.test.tsx` and `review/review.test.tsx` (it expects `queryByRole("dialog")` null at 164; keep that).
  - The stubs of `ChangedSinceControl` in `chat/panel*.test.tsx` need only the export.
  - `chat/chat.contracts.test.tsx` "chat changed since you looked lists documents whose hash differs from the checkpoint" (frozen) mounts the real `routes` at `/f/demo/product`, clicks the button, and must then find the documents on `/changes`: keep it green.
  - The stage contract `changes-is-a-page` renders `routes` at `/changes` with its own store and does not use the kit's `mount`.
- Exceptions: you do not own `ui/DESIGN.md`. Report any deliberate exception (the wide page) as `exception: <surface>: <rule>: <why>`.

## Run once at the end

`node_modules/.bin/vitest run src/plan/review src/plan/router.test.tsx src/plan/shell.test.tsx src/plan/chat/chat.contracts.test.tsx` from `web/`.
