# W3 — the dashboard section, and mounting it in the settings dialog

Lane: codex `gpt-5.6-terra`, effort `xhigh`. Three units, run in order, one forward each. Your task
text names ONE unit: do only the section headed with that unit id and stop at its own "Done when" line.

You run in WAVE 2, after W1 and W2 have returned. Their files exist on disk but your source-graph
lookups (`loom map …`) answer from the published base layer and cannot see them, so everything you
need from them is pinned verbatim below. Trust these signatures over anything a lookup tells you.

You own these files and no others:

```text
web/src/components/settings-webui.tsx      (new)
web/src/components/settings-webui.test.tsx (new)
web/src/components/settings-dialog.tsx     (edit — the mount and one empty-state guard, nothing else)
```

Read-only, for context: `web/src/components/settings-cards.tsx` (section heading markup at `:100-103`),
`web/src/components/settings-lanes.tsx` (`:150-185`), `web/src/components/settings-control.tsx`
(the `BoolSwitch` idiom at `:47-64`), `web/src/test/settings-kit.tsx`, `web/src/api/config.ts`,
`web/src/index.css` (the `eyebrow` utility at `:108-110`).

Do not run `git` at all. Do not edit `settings-dialog.test.tsx`, any other `settings-*` file,
`web/src/index.css`, `settings.css`, `settings-cards.css`, or anything under `web/dist`.

## Pinned contracts from wave 1

From W1, `web/src/state/notifications.ts`:

```ts
export const NOTIFICATIONS_STORAGE_KEY = "loom:notifications";
export const notificationsEnabledAtom; // jotai atomWithStorage<boolean>, default false
export type NotificationSupport = "unsupported" | "insecure" | "default" | "granted" | "denied";
export function notificationSupport(): NotificationSupport;
export async function requestNotificationPermission(): Promise<NotificationSupport>;
export function deliverNotifications(store: Store, previous: Snapshot | null, next: Snapshot): void;
```

From W2, `web/src/components/theme-picker.tsx`:

```tsx
export type ThemeId = "ledger" | "aubergine" | "pacific" | "graphite";
export interface ThemePalette { background: string; card: string; foreground: string; mutedForeground: string; border: string; primary: string; primaryForeground: string; }
export interface ThemeOption { id: ThemeId; label: string; caption: string; scheme: ColorScheme; lightVariant: LightVariant | null; darkVariant: DarkVariant | null; palette: ThemePalette; }
export const THEME_OPTIONS: readonly ThemeOption[];
export function ThemePicker(): ReactElement;
```

You use `notificationsEnabledAtom`, `notificationSupport`, `requestNotificationPermission` and
`ThemePicker`. You never call `deliverNotifications` — W1 already calls it from
`web/src/state/apply.ts`.

## Unit w3a-webui-section — `web/src/components/settings-webui.tsx`

### The public surface

```tsx
export function webuiMatches(query: string): boolean;
export function SettingsWebui({ query }: { query: string }): ReactElement | null;
```

`webuiMatches(query)`: `true` when `query.trim()` is empty; otherwise true when the lower-cased,
trimmed query is a substring of this haystack:

```text
dashboard theme appearance colour color light dark blue gray grey ledger aubergine pacific graphite notifications desktop alerts browser localstorage
```

This mirrors what `filterSections` (`web/src/components/settings-model.ts:231-240`) does for the
server-driven sections: substring, case-insensitive. Without it, typing a filter would leave an
unrelated section on screen while everything else disappeared.

`SettingsWebui({ query })` returns `null` when `!webuiMatches(query)`. Otherwise it renders a
`<section className="settings-webui">` holding, in order:

1. A heading block reusing the existing classes, exactly as `settings-cards.tsx:100-103` does:
   `<span className="eyebrow">dashboard</span>` followed by
   `<span className="settings-help">this browser only — kept in localStorage, never written to loom's config</span>`.
2. A theme row: the label `theme`, the caption
   `Each swatch draws the stage graph in that theme's own palette.`, and `<ThemePicker />`.
3. A notifications row: the label `desktop notifications`, the caption `Raised when a stage needs you.`,
   the switch, and a status line.

FORBIDDEN CLASS: do not use `settings-key-name` anywhere in this component.
`settings-dialog.test.tsx:288` collects every `.settings-key-name` in the whole document and compares
that set against the server-config rows `filterSections` yields; one of yours in the document makes
that assertion fail. Use your own class names under a `settings-webui` prefix, plus `eyebrow` and
`settings-help`, and Tailwind utilities for everything else. Do not add a CSS file.

### The notifications switch

```tsx
<button type="button" role="switch" aria-checked={on} aria-label="desktop notifications" disabled={…} onClick={…}>
```

`BoolSwitch` (`settings-control.tsx:47-64`) is an `<input type="checkbox" role="switch">`; yours is a
`<button>` because it has no form value, only an action. Both answer to `getByRole("switch", …)`,
which is how the dialog's own tests already find switches. Every existing switch query in the suite
passes a `name`, so yours collides with none of them.

State:

```tsx
const [enabled, setEnabled] = useAtom(notificationsEnabledAtom);
const [support, setSupport] = useState<NotificationSupport>(() => notificationSupport());
const on = enabled && support === "granted";
```

The lazy initializer is safe: this is a browser-only SPA with no SSR, so `window` always exists. `on`,
not `enabled`, drives `aria-checked` — the switch shows what is actually delivered.

Reconcile a stale preference. The user can opt in and later revoke or reset the permission in browser
settings, leaving the persisted atom `true` while nothing is delivered. One effect turns it back off:

```tsx
useEffect(() => {
  if (enabled && support !== "granted") setEnabled(false);
}, [enabled, support, setEnabled]);
```

Without this the row would render an ON switch that is disabled and cannot be turned off.

Click behaviour:

- when `on` is true → `setEnabled(false)`, nothing else
- when `on` is false and support is `"default"` → `const result = await requestNotificationPermission()`;
  `setSupport(result)`; `setEnabled(result === "granted")`
- when `on` is false and support is `"granted"` → `setEnabled(true)`
- the switch is `disabled` when support is `"unsupported"`, `"insecure"` or `"denied"` — after the
  reconcile effect it always reads off in those states

The permission request must happen inside this click handler and nowhere else: browsers only grant
the prompt in response to a user gesture.

Status line, one per support state:

| support | text |
| --- | --- |
| `unsupported` | `this browser has no notification support` |
| `insecure` | `notifications need localhost or HTTPS; this page is plain HTTP on a LAN address` |
| `denied` | `the browser has blocked notifications for this page` |
| `granted` | `on` when `on` is true, `off` when not |
| `default` | `the browser will ask when you turn this on` |

The `insecure` line is not hypothetical: `loom status --web --host <lan-ip>` serves plain HTTP off
loopback on purpose, and a plain-HTTP LAN origin is not a secure context, so the API is unavailable
there. Say so plainly rather than letting the switch look broken.

Below the status line, one fixed explanatory line:
`Fires when a stage needs you (a question, a block, a failed check, a merge conflict or a review), when one needs a handoff, and when the whole run finishes.`

Typography: Inter at 12-13px, sentence case, `text-muted-foreground` for captions. No all-caps
monospace chips — that is reserved for identifiers here
(`doc/loom/knowledge/conventions/web-dashboard-typography.md`). Keep the file under 400 lines.

### Done when (w3a)

The file matches this section. It has no test of its own; w3c covers it.

## Unit w3b-dialog-mount — `web/src/components/settings-dialog.tsx`

Two changes, both in the module-private `Body` component. First, inside the returned JSX, the scroll
region reads:

```tsx
<div className="settings-scroll">
  <LoadNotice load={load} onRetry={retry} />
  …
```

Insert `<SettingsWebui query={query} />` as the FIRST child of that `<div className="settings-scroll">`,
above `<LoadNotice …>`. Add the import (`SettingsWebui` and `webuiMatches`).

Second, the empty-state guard. `SettingsCards` and `SettingsLanes` render `nothing matches "<query>"`
whenever the server `sections` array is empty (`settings-cards.tsx:47-48`, `settings-lanes.tsx:100-107`)
and know nothing about your section. A query such as `theme` matches your section and no server row,
so the dialog would show the theme picker AND "nothing matches theme". In `Body`, add:

```tsx
const onlyDashboardMatches = query.trim() !== "" && sections.length === 0 && webuiMatches(query);
```

and change the guard in front of the cards/lanes ternary from `load.phase === "ready" &&` to
`load.phase === "ready" && !onlyDashboardMatches &&`. An empty query, or a query that matches neither,
behaves exactly as today.

Change nothing else in the file — not `Body`'s props, not `useSettingsWrites`, not the breakpoint swap.

Anchor on the `className="settings-scroll"` string and the `Body` function, not on a line number.

It goes ABOVE `LoadNotice`, and outside the `load.phase === "ready"` guard that wraps the
lanes/cards, on purpose: this section holds browser-local state and must still render when
`/api/config` is unreachable, which is exactly when the rest of the dialog shows an error notice.

### Done when (w3b)

Both changes are in `Body`. Its proof is w3c's tests plus the existing `settings-dialog.test.tsx`,
which the orchestrator runs.

## Unit w3c-section-test — `web/src/components/settings-webui.test.tsx`

Use `renderAt` and `fakeClient` from `@/test/settings-kit` so the section is exercised through the
REAL dialog at `?settings=1`, not in isolation — that is what proves the mount. `afterEach(() => cleanup())`.
Clear `localStorage` between cases. jsdom 30 implements neither `Notification` nor
`window.isSecureContext`, so stub both in every case that needs a supported state: `vi.stubGlobal`
a `Notification` class with a static `permission` and a static `requestPermission`, and
`Object.defineProperty(window, "isSecureContext", { configurable: true, value: true })`. Restore with
`vi.unstubAllGlobals()` in `afterEach`.

`renderAt(search, client)` builds its own jotai store and does not return it, so you cannot
`store.set` the preference. To start a case with the preference already on, call
`localStorage.setItem("loom:notifications", "true")` BEFORE `renderAt`: `atomWithStorage` re-reads
storage when `useAtom` mounts the atom.

The filter input is `screen.findByRole("searchbox", { name: "filter settings" })`; drive it with
`fireEvent.change(filter, { target: { value: "…" } })`, as `settings-dialog.test.tsx:277-294` does.

Cases:

1. the `dashboard` eyebrow and the four theme buttons are present once the dialog opens
2. the switch (`getByRole("switch", { name: "desktop notifications" })`) is present
3. with support `"default"`, clicking the switch calls `Notification.requestPermission` once; on
   `"granted"` the switch ends `aria-checked="true"`, and on `"denied"` it stays `"false"`
4. with `window.isSecureContext` false the switch is disabled and the LAN/HTTPS line is shown
5. the section still renders when the config load fails — pass an inline client
   `{ load: () => Promise.reject(new Error("offline")), write: async () => ({ ok: false, status: null, message: "offline" }) }`
   and assert the `dashboard` eyebrow is on screen
6. filtering hides and shows it: typing `zzzzznotfound` removes the section and shows the
   `nothing matches` line; typing `theme` brings the section back and `screen.queryByText(/^nothing matches /)`
   is `null`; typing `dark` also shows the section
7. a stale preference is reconciled: with `localStorage` holding `"true"` for `loom:notifications` and
   the stubbed permission `"denied"`, the switch ends `aria-checked="false"` (use `waitFor`), is
   disabled, the blocked-notifications line is shown, and `localStorage.getItem("loom:notifications")`
   ends as `"false"`
8. with the preference on and permission `"granted"`, the switch is `aria-checked="true"` and one click
   turns it `"false"`
9. `both breakpoints` (the name must contain those exact words — a plan gate greps for them): an
   `it.each` over `["wide", "narrow"]`. For `"narrow"`, override `window.matchMedia` so that
   `(max-width: 699px)` matches, exactly as `web/src/components/settings-cards.test.tsx:17-36` does,
   and restore the original in `afterEach`; assert `.settings-cards` is present so the case really
   ran the phone layout. For `"wide"` keep the setup default (nothing matches). Then drive the filter
   through three queries and assert each outcome:
   - `graphite` (dashboard only): the `dashboard` eyebrow is shown, and
     `screen.queryByText(/^nothing matches /)` is `null`
   - `sonnet` (server rows only — the query `settings-dialog.test.tsx:279` already uses, and not a
     substring of the section's haystack): the `dashboard` eyebrow is absent, at least one
     `.settings-key-name` is shown, and there is no `nothing matches` line
   - `zzzzznotfound` (neither): the eyebrow is absent and EXACTLY ONE element matches
     `/^nothing matches /` (`screen.getAllByText(...)` has length 1)

### Done when (w3c)

`bun run --cwd web test src/components/settings-webui.test.tsx` passes. Run it once if you want.

## Report

The orchestrator runs typecheck, lint, format and the full suite — including
`settings-dialog.test.tsx`, which your two `Body` changes must leave green. Report files changed,
anything you assumed, anything unresolved.
