# O1: "Changed since you looked", fixed and redesigned (#15)

Tier: opus at effort medium (`loom-senior-software-engineer`, spawned with `effort: medium`). This is visual work, and the behaviour below is fully settled.

**First action, before reading code:** invoke the `frontend-design:frontend-design` skill with the Skill tool. Then read `../common.md` (its "Design direction" section is the brief for the skill) and `web/src/plan/ui/DESIGN.md`. Where the skill's general advice and DESIGN.md disagree, DESIGN.md wins. The behaviour section below is fixed. The look section gives starting values you may refine within DESIGN.md.

Product contract: `doc/product/feat-planner/section_10.md:259-290`.

## Purpose (from the spec)

Each browser keeps its own record of what it has seen of each document: a checkpoint `{commit, hashes}` in localStorage (`web/src/plan/review/checkpoint.ts`). The dialog lists the documents whose hash differs from the checkpoint (added, changed, removed), for example after an agent or another person edited them, and shows each one's diff. "Mark all seen" moves the whole checkpoint to now. Opening one file's diff marks that file seen. Comparing with a git ref (for example `main`) is a separate, unstored mode.

## The defect

- `useSelectedDiff` (`review/changed-since.tsx` ~L94-120) always diffs `getAtCommit(selection.commit, path)` against the current text. `acknowledge` (~L78-92) then advances only that file's **hash** (`markSeen`, `review/checkpoint.ts`).
- The checkpoint's `commit` never moves per file. The next change to a seen file therefore diffs from the old commit again and shows edits the user already saw.
- Related gaps found at planning time (each confirmed in the tree):
  - the diff does not refetch when the selected file changes (`doc_changed`), because the effect depends only on `[store, selection, attempt]` (~L118);
  - `acknowledge` no-ops when `store.get(checkpointAtom) !== selection.checkpoint` (~L83), a stale object after any earlier mark;
  - the "Uncommitted diff" link in `ChangeRows` never acknowledges. `ChangeRows`, `Limits` and `RefForm` live in `review/changed-since-parts.tsx`, not `changed-since.tsx`.

## Behaviour (settled)

- A file's baseline is what this browser saw of it: the text at the moment its diff was shown, or at Mark all seen.
- **Mark all seen.** It moves the checkpoint (commit and hashes) to now, as `useMarkAllSeen` does today, and stores the current text of every document the list shows at that moment, fetched through the existing `/api/doc` route within the caps below. A document not in the list keeps the baseline it had. A fetch that fails leaves that document on the new checkpoint commit and deletes its earlier stored text, so an older text never becomes the baseline beside a newer seen hash.
- **Text store.** No new server route. Each stored entry pairs the seen text with the hash it was read at (`{hash, text}`), written in one storage write, and is used as a baseline only while that pair is intact. Keep each seen text in browser storage beside the checkpoint, through `storageKey` and `safeStorage` (both in `chat/storage.ts`). Wrap every read and write so a failed or empty storage degrades to the commit baseline. Cap: at most 256 KiB per document and 2 MiB in total. Evict the least recently seen first. A document over the cap keeps only its hash, and its diff falls back to the checkpoint commit. Its row then says "Changes since commit <short sha>". `safeStorage.setItem` swallows a quota error and sets the global `storageFullAtom` banner: evict before writing so the seen-text store never trips that banner by itself.
- **Fallback.** With no stored seen text, the baseline is the checkpoint commit, as today.
- **Availability is per document in checkpoint mode.** Today `ComparisonList` (`changed-since.tsx` ~L248-282) computes one `available` flag from the checkpoint commit and `/api/compare`, and `ChangeRows` (`changed-since-parts.tsx` ~L117-125) hides every row's diff button when it is false; `onOpen` also refuses without a commit. A stored seen text is a baseline of its own, so in checkpoint mode a row offers its diff when it has a valid stored pair, else when the commit is available. A diff from a stored text never fetches `/api/at-commit`. Pass availability per row (for example `available(path)`), keep the `Show diff for <path>` accessible name, and keep ref mode on its existing global rule. The frozen contract "seen text outlives a missing checkpoint commit" proves it: stored text, then `/api/compare` reporting the commit unavailable, and the diff still opens from the seen text.
- **Generations.** Removing the object-identity check in `acknowledge` (~L78-92) removes the guard it gave against stale responses, so replace it: the checkpoint carries a reset generation (a counter bumped by Mark all seen and by any checkpoint replacement), and the project binding `storageKey` already gives. Every diff fetch and every acknowledgment captures the generation when it starts and writes nothing when the generation has moved by the time it resolves. Mark all seen also clears the current selection's pending acknowledgment. A delayed diff response that resolves after Mark all seen therefore never overwrites the newer hash and text with an older snapshot. Successive updates to one document resolve in order: a response for an older hash than the one already stored is dropped. The open diff's displayed before and after stay as fetched when its own successful acknowledgment updates storage (no flicker to the new baseline until the next change).
- **Live refresh.** The open diff refetches when the selected document's hash changes. Marking a file seen never depends on object identity of the checkpoint.
- **Named tests** the stage's `after_stage` checks, each written to fail on the base behaviour, with deferred `fetchMock` responses (resolve them by hand):
  - `mark all seen during a diff fetch keeps the newer baseline`: open A's diff with `/api/doc` held, press Mark all seen and let it finish, then release the old `/api/doc` response; the stored entry and the checkpoint hash for A are Mark all seen's, not the old snapshot's.
  - `seen text survives a reload`: store a seen text, build a fresh jotai store and remount (localStorage kept), and A's next diff starts from the stored text.
  - Also cover, in tests you name freely: two successive updates to one document resolving out of order, a document deleted then recreated (its old stored text is dropped on deletion), and a storage read or write that throws.
- **Ref mode.** Comparing with a ref never advances anything.

## Look

- The dialog opens with a one-line lede: "Documents that changed since you last looked at them in this browser." A `.plan-fine` line under it reads: "Opening a document's changes marks it seen. Mark all seen starts over from now."
- The list: one row per document on a hairline, with a status glyph and word (added, changed, removed; no badge), the path in mono truncated from the left, and a "View changes" quiet `size="sm"` button. The selected row is marked with `aria-current` and the `--muted` wash.
- **Fixed accessible names** (the test kit's `openList` and four existing tests wait on them, and so does the frozen contract): the list stays named "Changed documents", each row's button keeps the accessible name `Show diff for <path>` (its visible text may read "View changes"), and the trigger keeps "Changed since you looked · N".
- The selected document's diff fills the space below the list, or to its right at 1024px and wider.
- "Mark all seen" is the single `secondary` button in the dialog's foot. "Compare with a ref…" is a quiet link that reveals `RefForm`.
- The three-paragraph `Limits` block becomes a short `.plan-fine` note in the foot.
- New CSS goes in a new `review/changed-since.css`, imported from `review/changed-since.tsx` as `import "./changed-since.css";` (the plan's wiring check greps that import in that file, so keep it there, not in `changed-since-parts.tsx` or the data module). The footer row trigger (`RowTrigger`, ~L290-309) uses V2's row seam, which V2 keeps stable while it restyles in parallel: the classes `activity-row`, `activity-row__body`, `activity-row__title`, `activity-row__sub`, `activity-row__time` and `ActivityTile({ kind?, paused?, children })` from `ui/activity-glyphs.tsx`. Use nothing else of V2's. Do not edit `header/activity.css`: V2 owns it.
- `changed-since.tsx` is 346 lines and grows with this work. Move the data hooks (`useChangedSince`, `useCompare`, `useSelectedDiff`, `acknowledge`) into a new `review/changed-since-data.ts` (a declared artifact of the stage, so it must exist with real content).
- Do not edit `review/changed-since-test-kit.tsx` in a way that changes what its exports do: the frozen contract overrides `fetchMock` itself and uses `mount`'s `emit` handle and `openList`.

## The product spec

`doc/product/feat-planner/section_10.md:259-290` says the diff's before side is the text at the checkpoint commit and lists limits this work removes. Rewrite that passage to the settled behaviour above, in the spec's current-state voice. You own that file for this change. Its stored summary turns stale; that is expected.

## Proof note

You cannot build or take screenshots: the main agent builds, shoots, and sends a refinement round with the shots if anything misses. In your report, list each design decision you made beyond the starting values and why.

## Files you own

`web/src/plan/review/**` (except the frozen contract file), `doc/product/feat-planner/section_10.md`.

## Contract (frozen before you start; make it pass)

`web/src/plan/review/changed-since-fixes.contracts.test.tsx`: "a viewed document's next diff starts from what was seen" and "seen text outlives a missing checkpoint commit".

## Proof

Run once at the end: `node_modules/.bin/vitest run src/plan/review src/plan/header` from `web/` (the footer row renders inside the activity popover).
