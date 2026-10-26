# plan-web-shell / shell: PlanShell, header, presence, Share, banners, 401 screen, checklist view

Tier: sonnet, Claude lane (UI on the dashboard's design system; the orchestrator escalates to
fable only on a failed review of the visual result). Starts after codex waves A to C.
Read `doc/plans/briefs/plan-server/common.md` first.

Design rules (binding): `doc/loom/knowledge/conventions/web-dashboard-typography.md` (Inter
12-13px weight 500 sentence case for chrome; `--font-display` for headings only; state as glyph
plus toned label; no coloured left borders; warnings in `--tone-warning`, never amber).
Spec: `doc/product/index.md` "The screen" (header: numbered seven-step tracker with the current
step marked, tabs); `doc/product/section_08.md` "Share", "Read-only browsers" (presence),
"Connection and server errors"; `doc/product/section_12.md` "One token, every data route" (401
screen wording), "Remote binds" (banner), "Sharing access"; `doc/product/section_02.md`
"Missing prerequisites" (read-only banner).

## Files you own

`web/src/plan/shell.tsx`, `web/src/plan/shell.test.tsx`, `web/src/plan/dock.tsx`,
`web/src/plan/header-extras.tsx`, `web/src/plan/help-keys.ts`, `web/src/plan/plan.css`,
`web/src/plan/checklist-view.tsx`, `web/src/plan/checklist-view.test.tsx`,
`web/src/plan/unauthorized.tsx`, `web/src/plan/unauthorized.test.tsx`,
`web/src/plan/offers/offer-button.tsx`, `web/src/plan/header/overflow.tsx`, `web/plan.html` and
`web/src/plan/main.tsx` (both specified in `entry.md`, section "`web/plan.html`, `main.tsx`, ..."). Codex units (see
`codex-units.md`) own and have already written `offers/actions.ts` (W8), `ui/disabled-reason.tsx`
(W9), `ui/load-state.tsx` (W10), the api client, socket and atoms (W1-W4); read them, never edit
them. Their specs stay in this brief and api-state.md.
`web/src/plan/header/{header.tsx, step-tracker.tsx, tabs.tsx, presence.tsx, share.tsx, banners.tsx, name-prompt.tsx, popover.tsx}`,
`web/src/components/help-dialog.tsx` (export `ShortcutGroups` only)
and `web/src/plan/header/*.test.tsx`.

## Components

- `PlanShell` (`shell.tsx`): the whole tree is wrapped in `TooltipProvider delayDuration={250}` (as
  `web/src/routes/shell.tsx:36` does; later plans' tooltips need it); root
  `<div data-loom-plan className="flex h-dvh ...">` (the attribute is
  the bundle check's sentinel; never put it in shared code); when `unauthorizedAtom` -> render only
  `<Unauthorized/>`; else `<PlanHeader/>`, `<Banners/>`, a row with `<main><ErrorBoundary resetKey={pathname}><Outlet/></ErrorBoundary></main>`
  and `<Dock/>`; the `?` key toggles a help dialog (copy the dashboard's handler in
  `web/src/routes/shell.tsx`: ignore modifiers and typing targets) rendering a `Dialog`
  (`web/src/components/ui/dialog.tsx`) that lists `PLAN_HELP_KEYS` with `HelpKeys`
  (`web/src/components/help-keys.tsx`) in the markup of `ShortcutGroups`
  (`web/src/components/help-dialog.tsx:162`, private: export it from that file rather than copying
  it; that one-word edit in `help-dialog.tsx` is yours). `ErrorBoundary` is
  `@/aurora-ui/feedback/ErrorBoundary`.
- `dock.tsx`: `export function Dock(): JSX.Element | null` returning `null`, with a doc comment
  naming plan 9 (the chat panel mounts here). `header-extras.tsx`:
  `export function HeaderExtras(props: { compact: boolean }): JSX.Element | null` returning `null`
  likewise (binding: plans 9, 10, 11 each add one line rendering their control and pass `compact`
  on; a control renders a compact form, glyph plus count, when `compact`). These are extension slots with a defined empty state, not stubs.
- `help-keys.ts`: `export const PLAN_HELP_KEYS: HelpShortcutGroup[]` (type from
  `web/src/components/help-topics.ts`) with one group "General": `?` "Open or close Help".
- `PlanHeader` (`header/header.tsx`): left `StepTracker`, then `Tabs` over `PLAN_TABS`
  (`web/src/plan/tabs.ts`, from `entry`; empty in plan 2 renders nothing), right
  `<HeaderExtras compact={narrow}/>`, `Presence`, `Share`. Below the `lg` breakpoint (`narrow`, from
  `matchMedia`): the tracker collapses to "Step 3 of 7 · Metaplan" (or "Ready to run"), a button
  whose popover lists the seven steps; `Presence` and `Share` move into an overflow menu
  (`header/overflow.tsx`, a popover); `HeaderExtras` stays in the header bar (plan 9's chat button
  must stay reachable) in its compact form. The header never wraps to a second row and never
  scrolls the page sideways.
- `StepTracker`: the seven steps from `checklistAtom`, numbered 1-7 with names, each with its
  mark as glyph plus label for screen readers (`aria-current="step"` on the active one); `ready`
  and `ready_all_done` mark no step current and show "Ready to run" / "All plans done".
- `Presence`: compact avatars-free list: each browser's name, "(you)" for `you.public_id`, the
  holder marked "authoring", "away" with the free-up time (`frees_at`) when away, and the document
  being read as a muted second line on hover; names render as text only.
- `NamePrompt`: on first open, if `localStorage["loom-plan-name"]` is unset, a small inline field
  in the presence popover asks for a name; saving sends `{ type: "set_name", name }` and stores it;
  on every `hello` the stored name is re-sent. Wrap storage access in try/catch.
- `Share`: a button opening a popover that fetches `GET /api/share` each time it opens (never
  cached in an atom; the link is dropped from state on close), shows the URL in monospace with a
  Copy button, then on loopback "Works on this machine. From another machine, tunnel first:" and
  the `tunnel` command with its own Copy button; on a remote bind the plain-HTTP warning in
  `--tone-warning`. `web/src/components/ui/` has no popover: build `web/src/plan/header/popover.tsx` on
  `Popover` from `radix-ui` (already a dependency; `ui/dialog.tsx` shows the import style), styled
  like `ui/dialog.tsx`; use `ui/button.tsx` for buttons.
- `Banners`: remote bind -> warning banner for the life of the page ("This server is plain HTTP.
  Anyone who can see the traffic can read the link and every document."); read-only mode -> one
  banner listing each `MissingPrereq` (`message`, `fix`); stopping -> "loom plan is stopping."
- `Unauthorized`: the one screen of section 12: "Open the link `loom plan` printed. Running
  `loom plan` again in this project prints it." When `connectionAtom.everOpen` is true (this page
  was connected, so the server restarted), it leads with "loom plan restarted. Open the URL it
  printed in the terminal." (section 08's row) and keeps section 12's sentences after it. This is the
  only place either wording appears; plan 9 shows no restart row of its own.
- `ChecklistView` (index route): the current step and the step list as a short readable page (step
  name, mark, one line each), then the primary offer and the compact checklist of gaps (`items`),
  each with its offer; it is the app's first screen until plans 3 and 5 add tabs.
- `offers/actions.ts` (binding; plans 9 and 11 register here):
  `export type OfferHandler = (offer: OfferView) => void`;
  `export function registerOfferAction(kind: OfferKind, handler: OfferHandler): () => void`;
  `export function offerAction(kind: OfferKind): OfferHandler | undefined`;
  `export function runOffer(offer: OfferView): boolean` (false when no handler). Plan 9 registers the
  authoring offers (start a chat), plan 11 `pressure_test` and `start_run`.
  `offer-button.tsx` `<OfferButton offer={offer} />`: with a registered handler, a button that
  is `<DisabledReason reason={authorBlockReasonAtom} onClick={handler}>{offer.label}</DisabledReason>`;
  with no handler, the label as plain text (no handler is registered in plan 2, so every offer
  reads as text until plan 9; that is the defined behaviour, not a stub).
- `ui/disabled-reason.tsx` (binding; every later plan uses it for a write control a browser may
  not use): `<DisabledReason reason={string | null} onClick={() => void}>{label}</DisabledReason>`.
  With `reason` null it is an ordinary enabled button calling `onClick`; with a reason it renders
  the button with `aria-disabled="true"`, still focusable, a click does nothing, and the reason as
  visible muted text linked by `aria-describedby`. Never a `disabled` button with a tooltip.
- `ui/load-state.tsx` (binding; plans 3, 5, 9, 10 use it): `<LoadState state={"loading" | "empty" | "not_found" | "error"} title? message? action?={{ label, onClick }} />`
  renders `Skeleton` (`web/src/components/ui/skeleton.tsx`) for loading, `EmptyState`
  (`@/aurora-ui/feedback/EmptyState`) for empty and not_found, `HazardPanel`
  (`@/aurora-ui/feedback/HazardPanel`) for error, with the action as a button (Retry, Back to the
  tree). Read both aurora components' props first.
- `plan.css`: only planning-specific layout classes; tokens come from `@/index.css`.

## Tests

shell.test.tsx: unauthorized replaces the whole shell; with `everOpen` it leads with the
restarted sentence, without it shows only section 12's; `?` toggles help; `Dock` renders nothing;
a `Tooltip` rendered inside `PlanShell` mounts without error; at a mocked narrow width the
tracker shows "Step N of 7 · <name>", Presence and Share are inside the overflow menu and
`HeaderExtras` receives `compact`. disabled-reason.test.tsx: the button is focusable, a click does
nothing, `toHaveAccessibleDescription(reason)`. load-state.test.tsx: each state renders its
component and the action button calls back. offers with a handler and another browser holding:
the reason "Dana holds authoring" is the accessible description.
offers: `registerOfferAction` then `OfferButton` renders a button that calls the handler; with no
handler it renders text; the unregister function removes it. header tests: tracker marks the active step with `aria-current`; presence marks holder, you, away;
a name with `<img src=x onerror=alert(1)>` renders as literal text; Share fetches on every open
and holds no URL after close (assert the popover content is gone and a second open fetches again);
loopback shows the tunnel line, remote shows the warning. checklist-view.test.tsx renders a
fixture `checklist.json`. unauthorized.test.tsx pins the wording.

## Done when

`bun run --cwd web typecheck`, `lint`, `format:check`, `test` pass.
