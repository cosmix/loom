# K2 — New work starts fresh, drafts on Home, rename, kind of work (issues 2, 10, 11; browser)

Tier: sonnet (`loom-software-engineer`). Read `common.md` first (D10-D13). K1 builds the server in parallel, from the shapes in K1's brief (`doc/plans/briefs/planner-review-fixes/kickoff-and-drafts/k1-drafts-server.md`, "Wire" and "Commands"). Mirror those exactly. V3 restyles after you return.

## Wire and state

- `web/src/plan/api/schema-kickoff.ts`: `kickoffDraftViewSchema` (strict, with required nullable fields).
- `web/src/plan/api/schema.ts`: add `kickoff_drafts` to `PLAN_MESSAGE_TYPES` and spread its schema into `planMessageSchema`.
- `web/src/plan/state/atoms.ts`: export `kickoffDraftsAtom = atom<KickoffDraftView[]>([])`, holding the snake_case wire objects. The stage contracts seed it with `store.set(kickoffDraftsAtom, …)`; keep that name and type.
- `web/src/plan/api/socket.ts` `PlanSocket.apply`: a `kickoff_drafts` case sets `kickoffDraftsAtom` from the frame. The server sends the frame on hello, so no fetch is needed for the list.
- `web/src/plan/api/schema-chat.ts`: the command union gains strict arms for `rename_kickoff {name, client_id}`, `set_aside_kickoff {session, client_id}`, `continue_kickoff {session, client_id}` and `discard_kickoff {session, client_id}` (K1's "Commands"); `ChatRequest` in `chat/commands.ts` is inferred from it.
- Commands in `chat/commands.ts`: `renameKickoff`, `setAsideKickoff`, `continueKickoff` and `discardKickoff`, sending those shapes.
- Fixtures (K1 writes no web file): `web/src/plan/api/fixtures/kickoff-drafts.json` exactly as K1's brief "Wire" gives it, and one example of each new command in `chat-commands.json`. K1's Rust parity tests read both.
- `web/src/plan/chat/viewed-document.ts` `featurePageOf`: `/f/<f>/source/<rel>` maps to `{kind: "file", path: "doc/<rel>", name: <basename>}`, exactly as `/files/<rel>` does (the reading stage added that route; today it returns `NO_DOCUMENT`, so the chat loses "Viewing …" and the attachment on send there). Add `viewed-document.test.ts` rows: `/f/demo/source/product/demo/section_01.md` gives that file document; `/f/demo/source/` (empty rest), a rest holding `..` or a backslash, and `/f/demo/source/x` under a feature the project does not hold give `NO_DOCUMENT`, as the `/files` grammar treats them. Add `web/src/plan/chat/viewed-source.test.tsx`, named `it("a Source page keeps the viewed document and sends it")`: mount the chat panel with `ScreenSync` under a memory router at a feature summary page, follow its Source link to `/f/<f>/source/<rel>`, and assert the context chip names that document and a send carries it as the attachment; dismissing the chip there and sending carries none. A plan `after_stage` greps the name.

## /new (D11, D13)

`web/src/plan/new/kickoff-route.tsx` (`KickoffRoute`, which renders the live session whenever one matches today) and `empty-kickoff.tsx`.

- `/new` with no `session` query always renders `EmptyKickoff`, even while a kickoff is live.
  - When `kickoffDraftsAtom` holds a live draft, the form shows a notice above it: "<name or Untitled> is still open", with "Continue" (a link to `/new?session=<id>`) and "Set it aside" (sends `set_aside_kickoff`, then the form is usable).
  - Starting while a kickoff is live is refused as today (the live-session refusal in `empty-kickoff.tsx`), with the notice's two actions as the way out.
- `/new?session=<id>`:
  - the live kickoff session with that id renders today's active view (transcript, composer, Taking shape);
  - an ended draft renders a read-only draft view (`new/draft-view.tsx`): it fetches `GET /api/kickoffs/<id>` (`{draft, shape, messages}`, K1's "Wire") for the problem, decisions, questions and "Your messages", which the list entry does not carry, and shows "Continue this draft" (`continue_kickoff`) and "Discard" (confirm first, then `discard_kickoff`);
  - an unknown id renders a `LoadState` error with a link to `/new`.
  - `isKickoffSession` (`new/kickoff-state.ts`) keys on the query id. The in-tab `kickoffSessionAtom` is used only to navigate: when this tab has a pending start and `sessionAtom` shows a kickoff session, `navigate("/new?session=" + id, { replace: true })`. The pinned test "recognizes the tab's kickoff before its shape arrives" (`kickoff-route.test.tsx`) changes with this; name it in your report.
- Every new action (Continue, Set it aside, Continue this draft, Discard, Rename) is gated like the proposal's actions: `canAuthorAtom` / `authorBlockReasonAtom` through `DisabledReason`, as `proposal.tsx` does.
- The kind of work (D13): a visible two-option control in a new `new/kind-control.tsx`, rendered by `empty-kickoff.tsx` as `<KindControl …>` (a plan wiring check) above the textarea. Two radios named "Feature" and "Standalone plan".
  - It replaces the hidden `small` toggle. The three `STARTERS` (the `STARTERS` constant in `empty-kickoff.tsx`) set it visibly.
  - `location.state.small` preselects Standalone plan (Add plan on Home); the stage contract `add-plan-preselects-standalone` checks the radio.
  - Keep the visible H1 and the textbox name /What do you want to build/ (journeys).
- `empty-kickoff.test.tsx`'s "keeps the draft in memory only" still holds for the pre-start form: nothing is stored before Start.

## Taking shape (D12)

`web/src/plan/new/taking-shape.tsx` `ShapeIdentity`: the Name row is editable until the work is confirmed (D12).

- It shows "Rename" ("Name it" when the shape's name is empty) while a shape exists and `target` is null, and nothing once `target` is set (the server refuses a rename then). It opens an inline field validated by `new/feature-name.ts` `isFeatureName`, with the same messages as `proposal.tsx` `RenameField`.
- Confirm sends `rename_kickoff`, and Escape cancels.
- Reuse `RenameField` rather than duplicating it: move it to `new/rename-field.tsx` if both need it.

## Home and activity (D11)

- `web/src/plan/home/drafts-shelf.tsx` (new), with a test:
  - a `<section aria-labelledby=…>` whose visible heading is "Unfinished" (the stage contract finds `getByRole("region", { name: "Unfinished" })`), listing `kickoffDraftsAtom`, newest first;
  - each row is a link to `/new?session=<id>`, titled by name, else `Untitled · <date started, locale short date>`, with the problem's first line and "open" or "set aside";
  - nothing renders when there are no drafts.
  - Mount it in `web/src/plan/home/home.tsx` above the feature cards. V3 styles it, and the home stage redesigns Home around it.
- `web/src/plan/header/use-activities.ts` and `activity-rows.tsx` `PlainRow`: a waiting session row is a link.
  - A kickoff links to `` `/new?session=${id}` ``, built in `activity-rows.tsx` (a plan wiring check greps `/new?session=` there); any other chat links to its home screen (`chat/session-home.ts`).
  - Its title is the draft's name, or the session brief's label, never `session.name` (the tmux label "loom plan: kickoff").
  - Update `use-activities.test.tsx` and `activity-popover.test.tsx`.

## Journeys

`web/scripts/plan-journeys-chat.mjs` drives the kickoff. Keep its textbox and button names, or update the script in the same change.

## Tests

- The frozen stage contracts `new-work-starts-fresh-beside-an-open-draft`, `home-lists-unfinished-drafts`, `name-is-editable-while-taking-shape` and `add-plan-preselects-standalone`.
- Unit tests for the `/new?session` branches and the activity link.

## Run once at the end

`node_modules/.bin/vitest run src/plan/new src/plan/home src/plan/header src/plan/api src/plan/chat` from `web/` (one command). The plan schema parity test `api/schema.test.ts` must pass with your `kickoff_drafts` arms.
