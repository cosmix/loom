---
name: loom-accessibility
description: Web accessibility patterns, WCAG compliance, and inclusive design. Use when implementing accessible UI, keyboard navigation, screen reader support, focus management, semantic HTML, ARIA patterns, or auditing for compliance.
triggers:
  - accessibility
  - a11y
  - WCAG
  - ARIA
  - screen reader
  - keyboard navigation
  - focus
  - tab order
  - tabindex
  - alt text
  - color contrast
  - semantic HTML
  - landmark
  - role
  - aria-label
  - aria-labelledby
  - aria-describedby
  - aria-live
  - aria-expanded
  - aria-selected
  - aria-hidden
  - focus trap
  - roving tabindex
  - skip link
  - assistive technology
  - prefers-reduced-motion
  - accessible name
  - inert
---

# Accessibility

## Overview

Making web UIs usable by everyone, including keyboard-only and assistive-technology users. Covers WCAG 2.1/2.2 AA, semantic HTML, ARIA, keyboard/focus management, screen readers, contrast, and testing. Optimize for AA — that is the legal bar (ADA, EN 301 549, AODA) in most jurisdictions.

## Core Rules (internalize these first)

1. **First rule of ARIA: don't use ARIA.** A native `<button>`/`<a>`/`<input>`/`<nav>` ships focus, keyboard, role, and state for free. ARIA only *describes*; it never adds behavior. Bad ARIA is worse than none.
2. **Second rule: don't change native semantics.** `<button role="heading">` is a footgun. Don't override roles of interactive elements.
3. **`aria-hidden="true"` on a focusable element is a trap** — it hides the element from the a11y tree while leaving it in the Tab order, so SR users land on "nothing." Hide the whole subtree and remove it from tab order (`inert`), never one or the other.
4. **Never `outline: none` without a replacement.** Removing the focus ring with no `:focus-visible` style is the single most common WCAG 2.4.7 failure.
5. **Automated tools catch ~30–40% of issues.** axe/Lighthouse find contrast, missing alt/labels, dup IDs — never "is the focus order sane," "does the SR announcement make sense," "is this keyboard-operable." Manual keyboard + SR passes are mandatory.

## Accessible Name Computation (the thing people get wrong)

The accessible name is what a screen reader announces. Resolution order (first non-empty wins):

1. `aria-labelledby` (space-separated IDs; concatenated text of targets) — **highest priority, overrides visible content**
2. `aria-label`
3. Native labeling: `<label for>`/wrapping `<label>`, `<img alt>`, `<fieldset><legend>`, `<figcaption>`, wrapped text content of a button/link
4. `title` / `placeholder` — **fallback only; never rely on these** (placeholder vanishes on input, `title` is not exposed on touch/keyboard reliably)

⚠ Gotchas:

- `aria-labelledby` referencing a *hidden* (`display:none`) element still contributes its text — intentional, useful for SR-only names.
- `aria-label`/`aria-labelledby` are ignored on non-interactive, non-landmark elements (`<div>`, `<span>`, `<p>`) unless they have a role. Don't expect `<span aria-label>` to announce.
- Icon-only controls MUST have a name: `<button aria-label="Close">✕</button>`. A bare glyph/SVG announces nothing or reads the character.
- `aria-describedby` adds *supplementary* description (hint, error) read after the name — it does not replace the name.

## WCAG: what actually gets flagged (AA)

Grouped by how you catch it. Full spec at w3.org/WAI/WCAG21/quickref; these are the high-frequency failures.

| Area | Criterion | Rule |
| ---- | --------- | ---- |
| Text alt | 1.1.1 (A) | Every `<img>` has `alt`; decorative → `alt=""` (not missing). Functional image → describe the action. |
| Contrast | 1.4.3 (AA) | Body text ≥ 4.5:1; large text (≥18.66px bold / ≥24px) ≥ 3:1. |
| Non-text contrast | 1.4.11 (AA) | UI components, focus indicators, and meaningful graphics ≥ 3:1 vs adjacent colors. |
| Color alone | 1.4.1 (A) | Never convey info by color only (add icon/text/underline). |
| Reflow | 1.4.10 (AA) | Usable at 320px width / 400% zoom, no horizontal scroll. |
| Keyboard | 2.1.1 (A) | All functionality via keyboard. |
| No trap | 2.1.2 (A) | Focus can leave any component (except intentional modal traps with Esc). |
| Focus order | 2.4.3 (A) | Tab order matches visual/reading order. |
| Focus visible | 2.4.7 (AA) | Visible focus indicator on every focusable element. |
| Name/role/value | 4.1.2 (A) | Custom widgets expose correct role + current state. |
| Labels | 3.3.2 (A) | Every input has a programmatic label. |
| Error id | 3.3.1 (A) | Errors identified in text (not color) and tied to the field. |
| Lang | 3.1.1 (A) | `<html lang="…">` set (and `lang` on inline language switches). |

**WCAG 2.2 adds** (AA): 2.4.11 Focus Not Obscured (sticky headers must not fully cover the focused element), 2.5.8 Target Size ≥ 24×24px, 3.3.7 Redundant Entry, 3.3.8 Accessible Authentication (no cognitive-test-only auth, allow paste into OTP).

## Semantic HTML

Reach for the native element before any `<div role>`:

```html
<!-- Good: roles, focus, keyboard all free -->
<header>
  <nav aria-label="Main">
    <ul><li><a href="/">Home</a></li><li><a href="/about">About</a></li></ul>
  </nav>
</header>
<main id="main">
  <article><h1>Title</h1><p>…</p></article>
</main>
<footer>…</footer>
```

- One `<main>` and one `<h1>` per page; never skip heading levels (h2→h4 breaks the SR outline).
- Landmarks (`header`/`nav`/`main`/`aside`/`footer`) let SR users jump by region — don't replace them with `<div class="header">`.
- Two same-type landmarks on one page (e.g. two `<nav>`) MUST be disambiguated with `aria-label` ("Main"/"Footer").
- `<button>` for actions in-page, `<a href>` for navigation. A clickable `<div>` needs role + `tabindex="0"` + Enter/Space handlers + focus style — i.e. reinventing `<button>` badly.
- Group radios/checkboxes in `<fieldset><legend>`; `<legend>` names the group for SR.

## ARIA Attributes Reference

```typescript
// Labels/descriptions
"aria-label" | "aria-labelledby" | "aria-describedby"  // see name computation above
// States (keep in sync with reality on every change)
"aria-expanded": boolean          // disclosure/accordion/combobox trigger
"aria-selected": boolean          // tab/option
"aria-checked": boolean | "mixed" // checkbox/switch/radio
"aria-pressed": boolean | "mixed" // toggle button
"aria-current": "page" | "step" | "location" | "date" | "time" | true // active item in a set
"aria-disabled": boolean          // disabled but still focusable (vs native `disabled`)
"aria-invalid": boolean | "grammar" | "spelling"
"aria-required": boolean
// Live regions
"aria-live": "off" | "polite" | "assertive"
"aria-atomic": boolean            // re-read whole region vs only changed node
// Relationships
"aria-controls" | "aria-owns" | "aria-haspopup": "menu" | "dialog" | "listbox" | true
```

Landmark roles (prefer the native element that implies them): `banner`(header), `navigation`(nav), `main`, `complementary`(aside), `contentinfo`(footer), `search`, `region`(needs a name), `form`.

### Live regions — the #1 subtle bug

**The container must exist in the DOM before you change its content.** Injecting a populated `aria-live` node in the same tick is often NOT announced — SRs only announce *mutations* to a region they already observe.

```tsx
// Wrong: node created+filled together → frequently silent
container.innerHTML = '<div aria-live="polite">Saved</div>';

// Right: region is present and empty on mount; update its text later
<div aria-live="polite" aria-atomic="true" className="sr-only" />  // rendered once
// ...later:  regionEl.textContent = "Saved";
```

- `polite` = wait for a pause (status, "5 results"). `assertive`/`role="alert"` = interrupt immediately (errors only). Overusing assertive is hostile.
- `role="alert"` implies `aria-live="assertive"` + `aria-atomic="true"`; `role="status"` implies polite.
- One shared visually-hidden live region toggled via a helper is more reliable than many.

## Focus Management

The highest-value, least-automatable a11y work.

**Modal open:** save `document.activeElement`, move focus into the dialog (the dialog or its first control), trap Tab inside, close on Esc, **restore focus to the trigger on close.** Make the background non-interactive with `inert` (native `inert` disables focus + pointer + hides from a11y tree in one attribute; falls back to `aria-hidden`+removing tabbables on older browsers).

**SPA route change:** browsers reset focus on full loads but NOT on client-side navigation — focus is stranded, and the SR announces nothing. On route change, move focus to the new page's `<h1>` (give it `tabindex="-1"`) or announce the new title via a live region. This is the most-missed SPA a11y bug.

**Roving tabindex** (toolbar/menu/tablist/grid): exactly one item has `tabindex="0"`, the rest `-1`; arrow keys move focus and shift the `0`. Keeps composite widgets to a single Tab stop.

```tsx
// Focus trap core — cycle Tab within a container, Esc to close
function trapFocus(container: HTMLElement, onEscape: () => void) {
  const sel =
    'a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])';
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") return onEscape();
    if (e.key !== "Tab") return;
    const els = [...container.querySelectorAll<HTMLElement>(sel)];
    const first = els[0], last = els[els.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus(); }
  };
  container.addEventListener("keydown", onKey);
  return () => container.removeEventListener("keydown", onKey);
}
```

⚠ `disabled` buttons are skipped by Tab and not announced as present — if users must discover a disabled action (e.g. "why can't I submit?"), use `aria-disabled="true"` + intercept activation instead, so it stays focusable. Prefer native `disabled` for simple forms.

⚠ Prefer the native `<dialog>` element (`.showModal()`): it provides the focus trap, Esc-to-close, backdrop, and top-layer stacking for free — far less to get wrong than a `role="dialog"` div.

## Keyboard Patterns (WAI-ARIA APG)

| Widget | Keys |
| ------ | ---- |
| Button | Enter **and** Space activate |
| Link | Enter only |
| Tabs | ←/→ move between tabs, Home/End first/last, Tab moves to panel (roving tabindex; auto- vs manual-activation) |
| Menu / Menubar | ↑/↓ items, →/← submenu, Esc closes + returns focus to trigger, type-ahead |
| Combobox | ↓ opens, ↑/↓ options, Enter selects, Esc closes, type filters |
| Accordion | Tab between headers, Enter/Space toggles panel |
| Dialog | Tab/Shift+Tab trapped, Esc closes, focus restored to trigger |
| Radio group | ↑/↓/←/→ move AND select (single Tab stop) |

Implement composite widgets against the WAI-ARIA Authoring Practices patterns rather than inventing key handling. Don't hijack browser/SR shortcuts (single-key handlers can collide with SR quick-nav — gate them behind a modifier or a focused control, per WCAG 2.1.4).

## Screen Readers

| Reader | Platform | Notes |
| ------ | -------- | ----- |
| NVDA | Windows | Free, dominant for testing. H=next heading, K=link, F=form field, D=landmark, NVDA+F7=elements list |
| JAWS | Windows | Paid, enterprise. H=heading, Insert+F6=headings list |
| VoiceOver | macOS/iOS | Built-in (Cmd+F5). VO=Ctrl+Opt; VO+U=rotor, VO+→/← navigate |
| TalkBack | Android | Built-in. Swipe →/← navigate, double-tap activate |

Test matrix that matters: **NVDA+Firefox and VoiceOver+Safari** cover most real usage. Chrome+NVDA second. Behavior differs across pairs — a name that reads on one may not on another.

SR test checklist:

- [ ] Page title announces on load / route change
- [ ] Headings form a logical outline (no skipped levels)
- [ ] Landmarks present and uniquely labeled; skip link is the first Tab stop
- [ ] Images: informative have meaningful alt, decorative have `alt=""`
- [ ] Links/buttons have descriptive names (no bare "click here"/icon-only without label)
- [ ] Form fields announce label, required, hint, and error
- [ ] Custom widgets announce role + state, and state updates are heard
- [ ] Dynamic changes announce via live regions (and only the right ones)

## Contrast & Visual

- Body ≥ 4.5:1, large text ≥ 3:1, UI/graphics/focus ring ≥ 3:1 (1.4.11).
- Contrast ratio = (L_lighter + 0.05) / (L_darker + 0.05), L = relative luminance. Don't hand-roll — use axe, the browser DevTools contrast picker, or a lib; verify against the *actual* rendered background (gradients/overlays included).
- `:focus-visible` (not `:focus`) shows the ring for keyboard users without flashing it on mouse click. Never remove the ring without a ≥3:1 replacement.
- Respect `prefers-reduced-motion` — vestibular disorders. Also `prefers-contrast`, `forced-colors` (Windows High Contrast: don't set colors via background-image; use `currentColor` and system color keywords).

```css
:focus-visible { outline: 2px solid #005fcc; outline-offset: 2px; }
@media (prefers-contrast: more) { :focus-visible { outline-width: 3px; } }
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: .01ms !important; animation-iteration-count: 1 !important;
    transition-duration: .01ms !important; scroll-behavior: auto !important;
  }
}
/* Screen-reader-only text — visible to SR, off-screen visually. NOT display:none (that hides from SR too) */
.sr-only {
  position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
  overflow: hidden; clip: rect(0 0 0 0); clip-path: inset(50%); white-space: nowrap; border: 0;
}
```

## Forms

Wire label, hint, error, and validity so a SR reads them as one unit:

```tsx
<label htmlFor="email">Email <span className="sr-only">(required)</span></label>
<input
  id="email" type="email" autoComplete="email" required
  aria-invalid={!!error}
  aria-describedby={`email-hint${error ? " email-error" : ""}`}
/>
<span id="email-hint" className="hint">We never share your email</span>
{error && <span id="email-error" role="alert">{error}</span>}
```

- Associate every input with a `<label for>` (or wrap it). Placeholder is not a label.
- `aria-describedby` may list multiple IDs (hint + error) — read in order after the name.
- `aria-invalid` toggles with validity; don't leave it hard-coded `true`.
- On submit failure: move focus to an error summary (`tabIndex={-1}` + `role="alert"`) with in-page links to each bad field. Don't rely on red borders alone (1.4.1).
- Use `autocomplete` tokens (1.3.5) and correct `type`/`inputmode` for mobile keyboards.

## Testing

**Automated (every PR) — necessary, not sufficient (~30–40% coverage):**

```typescript
// Unit — jest-axe
import { axe, toHaveNoViolations } from "jest-axe";
expect.extend(toHaveNoViolations);
it("no violations", async () => {
  const { container } = render(<MyComponent />);
  expect(await axe(container)).toHaveNoViolations();
});

// E2E — @axe-core/playwright
import AxeBuilder from "@axe-core/playwright";
const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
expect(results.violations).toEqual([]);
```

- Lint: `eslint-plugin-jsx-a11y`. Query by role/name in tests (`getByRole("button", { name: "Save" })`) — it fails when the accessible name is missing, doubling as an a11y assertion.
- CI gates: Lighthouse `categories:accessibility` ≥ 0.9, or Pa11y with `WCAG2AA`.

**Manual (before release) — the load-bearing part:**

- [ ] Unplug the mouse: reach and operate every control by keyboard; focus never lost or trapped
- [ ] Focus ring visible on every stop; Tab order matches visual order
- [ ] One SR pass (NVDA or VoiceOver): names, roles, states, and live announcements make sense
- [ ] Zoom to 400% / 320px width: no loss of content or horizontal scroll
- [ ] `prefers-reduced-motion` and forced-colors modes usable
- [ ] Color-only info has a non-color cue

## Verify Before Done

- [ ] Native element used wherever it fits; ARIA only where HTML can't express it
- [ ] Every interactive element has a correct accessible name (icon-only buttons labeled)
- [ ] No `outline:none` without `:focus-visible`; focus visible everywhere
- [ ] Modals: focus trapped, Esc closes, focus restored, background `inert`/`aria-hidden`
- [ ] SPA route changes move focus and/or announce
- [ ] Live regions exist in DOM before updating; polite vs assertive chosen correctly
- [ ] No `aria-hidden` on a focusable element; custom widget states kept in sync
- [ ] Form fields wired: label + `aria-describedby` (hint/error) + `aria-invalid`
- [ ] Contrast: text ≥4.5:1, large/UI ≥3:1; info not conveyed by color alone
- [ ] axe/lint green AND one manual keyboard + SR pass done

## Quick Reference: ARIA Patterns

```html
<button aria-pressed="true">Mute</button>                      <!-- toggle -->
<button aria-label="Close">✕</button>                          <!-- icon-only -->
<button aria-expanded="false" aria-controls="p1">Toggle</button><div id="p1" hidden>…</div>
<button aria-busy="true">Saving…</button>                       <!-- loading -->
<div role="dialog" aria-modal="true" aria-labelledby="t"><h2 id="t">Title</h2></div>
<div role="alert">Submission failed</div>                       <!-- interrupts -->
<div role="status" aria-live="polite">5 new messages</div>      <!-- polite -->
<a href="#page2" aria-current="page">2</a>                      <!-- active item -->
<span class="sr-only">, opens in new tab</span>                 <!-- SR-only suffix -->
<div aria-hidden="true">★★★</div>                               <!-- decorative, non-focusable -->
```

### Alt text

```html
<img src="chart.png" alt="Sales rose 25% Q1→Q2" />   <!-- informative: describe the info -->
<img src="print.svg" alt="Print this page" />         <!-- functional: describe the action -->
<img src="swoosh.png" alt="" />                        <!-- decorative: empty, not missing -->
<img src="logo.png" alt="Acme Corporation" />          <!-- image of text: transcribe -->
<img src="diagram.png" alt="Architecture overview" aria-describedby="d" /><p id="d">Full description…</p>
```
