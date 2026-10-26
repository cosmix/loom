# chat-web W6: Send to author, changed since you looked, open questions, built marker

Lane note: `checkpoint.ts` and its test are codex unit C7 (`checkpointAtom`, `changedSince`,
`markSeen`); the section below on it is reference.

Working dir `web/`. You own `src/plan/review/{send-to-author.tsx, send-to-author.test.tsx,
changed-since.tsx, changed-since.test.tsx,
open-questions.tsx, built-marker.tsx, review.test.tsx}`, and the bodies of plan 5's
`src/plan/concept/page-header-extras.tsx` and `key-chip-extras.tsx` and plan 3's
`src/plan/files/tree-entry-extras.tsx` (keep their props), plus the help rows in
`src/plan/help-keys.ts` (add a "Chat" group with `c`: "Focus the chat composer"). W1's API
clients exist. Frozen contracts: `src/plan/chat/chat.contracts.test.tsx`.

## send-to-author.tsx

`send-to-author.tsx` registers **Send to author** through plan 3's selection-secondary slot (see
"Fix-round additions" below). Clicking it opens a small form with the passage label and a note field (at most 1,000 characters, counted with
`[...note].length`), Send issues W1's `suggest`. Sender status: "Sent to <holder>", then
"Attached", "Dismissed" or "Dropped: authoring changed hands" from `suggestion_status`, or
"<holder> is away". Hidden when nobody
holds authoring.

## checkpoint.ts

`localStorage` key `loom-plan:<project key>:checkpoint`, value `{ commit: string | null; hashes:
Record<string, string> }`, `try/catch` around every access, zod-validated. The first visit sets
it silently from `getHashes()`. `markAllSeen()` replaces it; `markSeen(path)` updates one hash.
`changedSince(checkpoint, current): Array<{ path; change: "added" | "changed" | "removed" }>`
compares hashes (pure; unit tests).

## changed-since.tsx

`<ChangedSinceControl />`, which W2 mounts in plan 2's header slot (you do not edit
`header-extras.tsx`): "Changed since you looked · <n>" (hidden at 0). It opens
a list: each row a document with its change and a diff, a fourth kind beside plan 3's three
(before: `getAtCommit(checkpoint.commit, path)`, after: the current file), opened in plan 3's diff
view; opening one row's diff calls `markSeen`. **Mark all seen**. A ref field compares instead
with a typed ref: `resolveRef(name)` then `getCompare(commit)` and `getAtCommit`, not stored.
Limits shown with the list: a document that had uncommitted edits when the checkpoint was set
diffs against its committed text (row note); `available: false` from `getCompare` lists the files
as changed with only the uncommitted and ref diffs offered; local storage belongs to the origin
(port) and to this browser. Expose `useChangedSince()` for the dots below.

## open-questions.tsx, built-marker.tsx, the extension files

`<OpenQuestionCount path />`: "Open questions · <n>" linking to `/files/<path>#open-questions`,
from W1's `openQuestionsAtom` (`src/plan/review/live.ts`, kept fresh there); nothing when the document
has no such heading. `page-header-extras.tsx` renders `<OpenQuestionCount path={page.path} />`
(the stage's wiring check greps `<OpenQuestionCount` there) and, on a product section's page,
`<BuiltMarker section={page.path} />`: for each `ChangedAfter` "Changed after <slug> was built"
linking to the diff from `base_commit` to the current file, with a tooltip stating the limits of
section 10 (a re-run moves the date later; a done plan without a review shows no marker); for
each `Running` "This plan is running; changes here do not reach it". `key-chip-extras.tsx` and
`tree-entry-extras.tsx`: a small dot labelled "changed since you looked" when the document is in
`useChangedSince()`; the tree entry also shows the open-question count.

## review.test.tsx

Send to author form limits and statuses; first-visit checkpoint set silently; Mark all seen;
ref comparison with `resolveRef` refusing (shows the server's reason); the marker's two forms.

## Required test cases (coverage audit; named tests)

`send-to-author.test.tsx`: "shown only to viewers while held"; "form limits"; "statuses"; "away
status"; "hidden without holder". `checkpoint.test.ts`: "stored per project key"; "removed
document listed"; "changedSince pure comparison". `changed-since.test.tsx`: "opening a row's diff
marks it seen"; "diff before from at-commit"; "ref comparison lists and does not store";
"uncommitted note on row"; "unavailable commit offers only uncommitted and ref diffs"; "limits
text shown". `review.test.tsx`: "first-visit checkpoint set silently"; "mark all seen";
"resolveRef refusal shown"; "count on summary header and tree entry"; "count links to
open-questions anchor"; "dots on key chip and tree entry"; "marker links diff from base commit";
"marker tooltip states limits"; "running note"; "help keys hold the Chat group".

The marker tooltip's limits come from `loom/src/fs/plan_lifecycle.rs`
(`mark_plan_done_if_all_merged` regenerates the review when `loom run` runs again on a done plan)
and from a done plan without a review showing no marker.

## Fix-round additions (pressure test px-browser-01, -23, -25, px-seams-08)

- Send to author mounts through plan 3's app-wide selection-secondary slot
  (`registerSelectionSecondary(render: (draft) => JSX.Element | null) -> unregister` from
  `@/plan/attachments`; `AddToChat` renders for every browser with a draft and shows its own
  button only while `canAuthorAtom`). `send-to-author.tsx` exports
  `registerSendToAuthor()`, which registers a render that returns the Send to author button only
  while `canAuthorAtom` is false and someone holds authoring; W2's `ChatRegistrations` calls it on mount.
  No mount site of plan 3 or plan 5 changes. The frozen contract
  `send-to-author-in-reading-column` checks it in the real file reading column.
- Help rows in the Chat group: `c` "Focus the chat composer", `y` "Allow the pinned permission
  request", `n` "Deny it, with a note".
- The checkpoint key uses `ServerInfo.project_key`; open-question counts arrive as the `review`
  kind `open_questions` (W1's `live.ts` applies them); the compare list refetches in plan 2's
  `onResync` callback (counts and hashes are W1's).
- Tests: "dropped status shown to the sender"; "help keys hold c, y and n"; "checkpoint keyed by project key"; "resync refetches the
  compare list".
