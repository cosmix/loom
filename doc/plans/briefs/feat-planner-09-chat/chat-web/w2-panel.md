# chat-web W2: panel shell, state line, session line, connection states

Working dir `web/`. You own `src/plan/chat/{panel.tsx, panel.test.tsx, state-line.tsx,
state-line.test.tsx, run-header.tsx, connection-banner.tsx, attention.ts, attention.test.ts,
chat.css}`, the panel's mount in plan 2's `src/plan/dock.tsx` and the header additions in
`src/plan/header-extras.tsx` (plan 2's files: add only): the narrow-screen chat button and W6's
`<ChangedSinceControl />` (import it from `src/plan/review/changed-since.tsx`; W6 owns that file), and one change in
`src/components/terminal/terminal-notice.tsx`: export the existing `Stamp` function (it is
private today, defined after `refusedText`); change nothing else in that dashboard file.

## panel.tsx

`<ChatRegistrations />` (in `panel.tsx`, rendered unconditionally by the dock beside the panel or
sheet, at every width and whether the sheet is open or closed) calls W5's
`registerAuthoringOffers()`, W6's `registerSendToAuthor()` and W1's `useReviewLive()` once on mount and their
unregisters on unmount; registration never depends on the panel's layout. `<ChatPanel />`,
mounted in plan 2's `<Dock />` (the stage's wiring check greps `<ChatPanel` in
`dock.tsx`): an `aside` labelled "Chat", on `--card` with a `--hairline` left
border. Order: header (title "Chat", collapse control, menu with End chat from W5), state line,
`<RunHeader session />`, the connection banner, `<Transcript />` (W3), the queue strip, chips,
permission and suggestion cards and composer (W4's `<ComposerArea />`). Width from W1's prefs,
resizable by dragging the left edge (pointer events, keyboard: the handle is a separator with
arrow keys), 20-40rem. Collapsed: a 2.5rem rail with the state glyph and a badge when a
permission waits; the reading column recentres (the shell's grid reads the width). Below `lg`
the panel leaves the layout and becomes a sheet over the reading column opened from the header
button (state glyph, permission badge, attachment count); adding a chip does not open it.

## state-line.tsx

`role="status"`, polite, announcements at most once every five seconds while Working. One row
per section 08 state: none ("No chat" and the primary offer plus New chat, W5's buttons); Starting
(`BusyRoundel`, "Starting session", name and model); Idle (roundel at rest, "Waiting for you");
Working (`WorkRoundel` from `web/src/components/activity-roundel.tsx`, "Working · <elapsed>" with
`formatElapsed` from `web/src/lib/format.ts` counted from the turn's first `UserPromptSubmit`, the
newest tool row's summary, "<n> subagents"); Permission (`--tone-warning` glyph, "Needs a
decision: <tool>"); Ending ("Writing final notes into the documents", the 10-minute bound);
Ended (the exported `Stamp`, "Chat ended · <age>"); Failed (`HazardPanel` error tone,
`web/src/aurora-ui/feedback/HazardPanel.tsx`, "The session could not start" with the spawn error
or "The session stopped unexpectedly" with the last activity time, plus "The documents keep what
the session recorded as it went. A new chat starts fresh."). Read-only mode
(`serverInfoAtom.read_only` non-empty): `HazardPanel` warning tone naming each missing piece with
its `message` and `fix`, no offers.

## run-header.tsx

`<RunHeader session={session} />`: the session line "<holder> authoring · <n> watching ·
<tokens>" (count hover lists browser names from plan 2's `presenceAtom`; tokens `compactTokens` of the
session's fresh total, `web/src/lib/tokens.ts`; time left from `quotaLeftText` when plan 2 or
plan 10 supplies a quota reading, else omitted), "<holder> is away · authoring frees at <HH:MM>"
from `away_until`, "Read-only · <holder> is authoring" for viewers. Keep it one component whose
body is a `switch` on nothing but the session line today: plan 11 adds one branch for
`session.kind === "pressure"`.

## connection-banner.tsx

Four rows of section 08 "Connection and server errors", from plan 2's `connectionAtom`,
`serverInfoAtom.stopping` and its 401 state: lost and reconnecting (retrying after 1, 2, 5, then every 10 seconds is plan 2's client;
show its phase), the holder's 10-minute line, stopping ("loom plan is stopping. The session is
writing its notes." and the composer disabled through W4's gate), a refused command (W4 shows it
under the composer). While any is shown, the transcript dims with "Last update <age>".

## attention.ts

Tab title prefix `(!)` while a permission waits and `(✓)` when a turn ended while
`document.hidden`; cleared on `visibilitychange` to visible. Desktop notifications for the
holder through `deliverNotifications`-style calls guarded by the opt-in atom
(`web/src/state/notifications.ts`), off by default. Tests with a fake `document.hidden`.

## chat.css

Message text Inter 14px/1.5, chrome 12-13px; no coloured spine; reduced motion stops the
roundel. Tests in `panel.test.tsx` and `state-line.test.tsx`: every state row, collapse rail
badge, sheet below `lg` (mock `matchMedia` as `web/src/lib/use-media-query.test.ts` does).

## Required test cases (coverage audit; named tests)

`panel.test.tsx`: "drag clamps to 20 and 40rem and persists"; "separator arrow keys resize";
"collapse rail badge"; "collapsed flag persists"; "sheet below lg"; "header button shows
attachment count and adding a chip leaves the sheet closed"; "reduced motion stops the roundel".
`state-line.test.tsx`: "every state row" (none, starting, idle, working, permission, ending,
ended, failed, each with its words); "working shows elapsed, newest tool, subagents"; "failed
spawn text"; "failed died text with last activity"; "read-only mode hazard lists problems, no
offers"; "announcements throttled to five seconds". Run header (in `state-line.test.tsx`):
"session total and time left when quota known"; "away line and return"; "holder and watcher count
with names". Connection banner (in `panel.test.tsx`): "lost banner"; "holder line"; "stopping"; "dims transcript with last update". `attention.test.ts`: tab title prefixes;
"notifications only when opted in".

Roundels: use `WorkRoundel` (and `BusyRoundel`) for sessions. `ActivityRoundel` takes a
`StageSummary` and does not fit a session, so it is not reused.

## Fix-round additions (pressure test px-browser-04, -13, -17)

- A refused cookie (401) is plan 2's `Unauthorized` screen, which replaces the whole shell and
  carries the "loom plan restarted" wording; the chat has no restarted row.
- Several pending permissions: the state line reads "Needs a decision: Bash" for one and "Needs
  N decisions: Bash, Edit" for more, oldest first; the rail and header badges show the count.
- Compact header (plan 2's `compact` prop on `HeaderExtras`, below `lg`): the chat sheet button
  always stays visible (glyph, permission count, attachment count); `ChangedSinceControl` shows
  a dot and the count only.
- Tests: "two pending permissions name both tools and count 2"; "compact header keeps the chat
  button and shrinks changed-since".

## Round 2 (pressure test px-browser-R01)

- `dock.tsx` renders `<ChatRegistrations />` and the panel or sheet; the sheet's content may mount
  only while open, so nothing the app needs while it is closed lives inside it.
- Tests: "below lg with the sheet closed, an OfferButton is a button" (mock `matchMedia` below
  `lg`, render a checklist offer, assert a button role); "below lg with the sheet closed, a viewer's
  selection shows Send to author".

