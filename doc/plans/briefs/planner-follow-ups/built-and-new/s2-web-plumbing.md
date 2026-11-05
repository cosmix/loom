# S2: web plumbing for the Built state and the follow-up kind (R5, R12)

Tier: sonnet (`loom-software-engineer`). Read `common.md`, then this brief, then
`doc/loom/knowledge/architecture/planner-feature-screens.md` ("Layout route and landing",
"Offers have one owner each", "Run page") and `conventions/plan-web.md` ("UI Repair Test Rules",
"Planner Polish Rules") before code. You build behaviour and pure modules; the two fable workers
(F1 `/new`, F2 the Built page) build the screens on them in the next wave, so no new CSS and no
page markup beyond what is named here. Anchor edits by symbol: earlier stages of this plan edited
`run-page.tsx`, `offer-handlers.ts`, `offer-flow.ts` and `panel.tsx`.

## 1. Schema: `web/src/plan/api/schema.ts`

Mirror S1's Rust wire (`loom/src/plan_server/wire/checklist_built.rs`):

```ts
export const runReviewViewSchema = z.strictObject({ slug: z.string(), path: z.string() });
export const specItemViewSchema = z.strictObject({
  stage: z.string(), stage_id: z.string(), text: z.string(), review: z.string(),
});
export const candidateViewSchema = z.strictObject({
  kind: z.enum(["finding", "question"]),
  stage: z.string(), stage_id: z.string(), text: z.string(), review: z.string(),
});
export const builtViewSchema = z.strictObject({
  reviews: z.array(runReviewViewSchema),
  spec: z.array(specItemViewSchema),
  candidates: z.array(candidateViewSchema),
});
```

`checklistViewSchema` gains `built: builtViewSchema.optional()` with a one-line comment: the
server always sends it; it is optional so the typed `ChecklistView` literals that build a
checklist by hand (`home/features-contracts-kit.tsx`, `home/feature-card.test.tsx`,
`home/home.test.tsx`, `new/kickoff-landing.test.ts`, `new/kickoff-route.test.tsx`) stay valid.
Export `BuiltView`, `CandidateView`, `SpecItemView`, `RunReviewView` types. Every reader goes
through `builtOf` below, never `checklist.built` directly.

## 2. `web/src/plan/feature/built-model.ts` (new) and its test

```ts
export const EMPTY_BUILT: BuiltView; // all three lists empty
export function builtOf(checklist: ChecklistView): BuiltView; // checklist.built ?? EMPTY_BUILT
/** Every plan is done: the server says the feature is ready and all done, and every live plan
 *  row is done (a row still written or running, or an unwritten metaplan entry, is not built). */
export function isBuilt(checklist: ChecklistView): boolean;
// checklist.current.kind === "ready_all_done" && allPlansDone(checklist) (feature/run-scope.ts)
export function reviewFile(path: string): string; // last path segment
/** Where a run review opens: inside its feature when it lives there, else the file browser. */
export function reviewHref(path: string): string;
// featureSourceHref(featureOfDocPath(path), path) (feature/source-href.ts)
/** The text that seeds a Plan follow-ups session with one candidate and its source (R3). */
export function followUpSeed(candidate: CandidateView): string;
// `${text} (open ${kind === "finding" ? "finding" : "question"} from ${reviewFile(review)}, stage ${stage_id})`
// R3's source is the review file and the stage id; the stage name stays on screen only.
```

Tests: `isBuilt` false for `ready_all_done` with a written row, false for `ready` with all rows
done, true for both; a retired row does not count; `reviewHref` of
`doc/plans/REVIEW-PLAN-x.md` is the `/files/` address and of
`doc/plans/atlas/archive/REVIEW-PLAN-x.md` is `/f/atlas/source/...`; `followUpSeed` of a finding
and of a question.

## 3. Landing and the Run page

- `feature/landing.ts`: `landingPath` returns `featureHref(feature, "built")` first when
  `isBuilt(checklist)`; the doc comment names the four cases. `landing.test.ts`: one case.
- `feature/run-page.tsx`: delete `Followups` and every import only it used. When
  `isBuilt(checklist)`, render after `PlanRows`:
  `<p className="plan-page__note">Every plan is done. <Link to={featureHref(checklist.feature, "built")}>See what was built</Link></p>`.
  Update `run-page.tsx`'s `RunPage` doc comment. In `feature/run-page.test.tsx`, delete the
  follow-up offer cases and add: a built feature's Run page links to `/f/atlas/built` and draws
  no button whose name is `Plan follow-ups`.
- The spine's mapping of `built` to the Run group is S3's (`spine-model.ts` is S3's file).
- `rg -n 'Next round|new_metaplan_round|plan_follow_ups' web/src/plan` after your edits and
  report every hit outside your files.

## 4. Offers carry a seed

- `offers/actions.ts`: `OfferHandler = (offer: OfferView, seed?: string) => void`;
  `runOffer(offer, seed?)` passes it on. `offers/actions.test.ts`: a seed reaches the handler.
- `chat/offer-handlers.ts`: `OfferStart` gains `text?: string`; the registered handler sends
  `{ offer: offer.id, ...feature, ...(seed !== undefined ? { text: seed } : {}) }`.
  `chat/offer-handlers.test.ts`: a seeded offer sends its text.
- `chat/offer-flow.ts`: `createOfferStarter(...).start(offer, feature?, text?)` passes `text` to
  `startChat` (third argument). `chat/panel.tsx`: the registration lambda passes `text`.
  `chat/start-chat.test.tsx` (where `createOfferStarter` is tested): a started seeded offer's
  socket frame carries `text`.

The server already joins a typed first message after an offer's generated opening and shows the
typed part as the user's first message (`plan_server/guidance/provider.rs`,
`write_opening_typed`), so no server change is needed for the seed.

## 5. `/new` behaviour modules (F1 renders them)

`web/src/plan/new/follow-up-features.ts` (new) and its test:

```ts
export const PLAN_FOLLOW_UPS_ID = "plan-follow-ups";
export interface FollowUpFeature {
  name: string;
  built: boolean;            // isBuilt
  done: number;              // live rows in state done
  total: number;             // livePlans(checklist).length
  status: string;            // featureStatus
  candidates: CandidateView[]; // builtOf(checklist).candidates
}
/** The features a follow-up can be for: every feature with a metaplan, done or not, in order. */
export function followUpFeatures(checklists: ChecklistView[]): FollowUpFeature[];
/** "done · 13 plans", "in progress · 4 of 6 plans", or "no plans yet". */
export function featureStatus(built: boolean, done: number, total: number): string;
```

Use `plural` (`ui/plural.ts`) for "13 plans" and "6 plans".

`web/src/plan/new/kickoff-start.ts` (new) and `kickoff-start.test.tsx`: move `beginKickoff` and
`useKickoffStart` here from `new/empty-kickoff.tsx` (lines ~60-119; do not edit that file: F1
deletes its copies and imports yours). Export:

```ts
export type WorkKind = "feature" | "plan" | "follow-up";
export type PickedDocument = StarterDocument & { id: string };
export function useKickoffStart(onFollowUp?: (feature: string) => void): {
  start: (kind: WorkKind, text: string, documents: PickedDocument[], feature: string | null) => Promise<void>;
  pending: boolean;
  error: string | null;
};
```

The kickoff kinds keep today's behaviour exactly (`feature` sends `kickoff`, `plan` sends
`kickoff-small`, then `waitForKickoffSession`). `follow-up` with `feature === null` starts
nothing. `follow-up` with a feature sends
`startChat(store, PLAN_FOLLOW_UPS_ID, text, attachments, feature)` and, once that command
resolves, calls `onFollowUp(feature)`; it does not wait for a kickoff session and never sets
`kickoffSessionAtom` (a follow-up is a feature chat, not a kickoff, and `KickoffRoute` would move
the tab to `/new?session=`). The guards stay as they are: no double start, authoring required,
text or a document required, and an open chat refused with `noPanelRefusal`. Tests (mount with
`harness` from `@/plan/chat/start-chat-test-kit` as `empty-kickoff.test.tsx` does, through a tiny
test component that calls the hook): each kind's frame; a follow-up without a feature sends
nothing; `onFollowUp` gets the feature after the result; `kickoffSessionAtom` stays null for a
follow-up.

## Planned edits of frozen tests (only these; name each in your report)

- `web/src/plan/feature/polish.contracts.test.tsx`, test "a feature with a written spec opens on
  its product overview": its `renderRoutes("/f/atlas", finishedChecklist())` becomes
  `renderRoutes("/f/atlas", pressureChecklist())`. An all-done feature now lands on Built;
  `pressureChecklist()` (two written plans, product spec written) keeps the test's subject.
- `web/src/plan/feature/pages.contracts.test.tsx`: if a test still expects a follow-up offer
  button on the Run page (it was "a finished feature's Run page holds the compare offer once"
  before follow-ups-core), delete it and the helpers and registrations only it used. If
  follow-ups-core already deleted it, leave the file alone.

No other frozen contract may change. `new/review-fixes.contracts.test.tsx`,
`feature/spine.contracts.test.tsx`, `feature/review-fixes.contracts.test.tsx` and the home
contracts must pass unedited.

## Files

Your row in the stage's worker table. Never edit S1's, S3's, F1's or F2's files: not
`new/empty-kickoff.tsx`, `new/kickoff-route.tsx`, `router.tsx` or anything under `new/` other
than your two modules and their tests.

## Run once at the end

`node_modules/.bin/vitest run src/plan/feature src/plan/offers src/plan/chat/offer-handlers.test.ts src/plan/chat/start-chat.test.tsx src/plan/new/kickoff-start.test.tsx src/plan/new/follow-up-features.test.ts src/plan/api`,
from `web/`. `feature/built.contracts.test.tsx` fails in this run until F2 routes the Built
page next wave; every other failure is yours.

## Report

Files changed; the test run and its result; each frozen and pinned test you changed and why;
every `rg` hit outside your files.
