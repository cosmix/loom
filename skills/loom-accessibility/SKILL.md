---
name: loom-accessibility
description: "Use when implementing accessible UI components, auditing WCAG compliance, adding keyboard navigation, screen reader support, focus management, semantic HTML, ARIA patterns, or color contrast checks."
allowed-tools: Read, Grep, Glob, Edit, Write, Bash
trigger-keywords: accessibility, a11y, WCAG, ARIA, screen reader, keyboard navigation, focus, tab order, tabindex, alt text, color contrast, semantic HTML, landmark, role, aria-label, aria-labelledby, aria-describedby, aria-live, aria-expanded, aria-selected, aria-hidden, focus trap, roving tabindex, skip link, assistive technology
---

# Accessibility

## Overview

Web accessibility ensures websites and applications are usable by everyone, including people with disabilities. This skill covers WCAG 2.1 compliance, semantic HTML, ARIA patterns, keyboard navigation, screen reader support, visual accessibility, focus management, and automated testing.

## Instructions

### 1. WCAG 2.1 Compliance

WCAG is organized around four principles (POUR):

| Principle | Description | Key Guidelines |
|-----------|-------------|----------------|
| **Perceivable** | Information must be presentable to users | Text alternatives, captions, adaptable content |
| **Operable** | Interface must be operable | Keyboard accessible, enough time, no seizures |
| **Understandable** | Information and operation must be understandable | Readable, predictable, input assistance |
| **Robust** | Content must work with assistive technologies | Compatible with current and future tools |

#### Conformance Levels

- **Level A**: Minimum accessibility (must have)
- **Level AA**: Addresses major barriers (legal requirement in many jurisdictions)
- **Level AAA**: Highest level (nice to have for specific audiences)

#### Level A Essentials

- All images have alt text (1.1.1)
- Videos have captions (1.2.2)
- Color is not the only means of conveying information (1.4.1)
- All functionality available from keyboard (2.1.1)
- Users can pause, stop, or hide moving content (2.2.2)
- Page has a title (2.4.2); focus order preserves meaning (2.4.3)
- Link purpose clear from text or context (2.4.4)
- Forms have labels or instructions (3.3.2)
- Name, role, value available for all UI components (4.1.2)

#### Level AA Standard

- Contrast ratio at least 4.5:1 for normal text, 3:1 for large text (1.4.3)
- Text resizable to 200% without loss of functionality (1.4.4)
- Multiple ways to find pages (2.4.5); focus visible (2.4.7)
- Language of page identified (3.1.1)
- Input errors suggested (3.3.3); error prevention for legal/financial (3.3.4)

### 2. Semantic HTML

Always prefer semantic elements over generic divs with ARIA roles.

```html
<!-- Bad: div soup -->
<div class="header"><div class="nav"><div class="nav-item">Home</div></div></div>
<div class="main-content"><div class="article"><div class="title">Title</div></div></div>

<!-- Good: semantic structure -->
<header>
  <nav aria-label="Main navigation">
    <ul><li><a href="/">Home</a></li></ul>
  </nav>
</header>
<main>
  <article><h1>Title</h1><p>Content here...</p></article>
</main>
<footer>Footer content</footer>
```

Key semantic elements:

- **Sectioning**: `header`, `nav`, `main` (one per page), `article`, `section`, `aside`, `footer`
- **Text**: `h1`-`h6` (maintain hierarchy), `p`, `ul`/`ol`, `blockquote`, `figure`/`figcaption`
- **Interactive**: `button` (actions), `a` (links), `details`/`summary`, `dialog`
- **Forms**: `form`, `label` (always pair with inputs), `fieldset`/`legend`

### 3. ARIA Attributes

**First rule of ARIA**: Do not use ARIA if semantic HTML can do it. ARIA fixes what HTML cannot express.

**Landmark roles**: `banner`, `navigation`, `main`, `complementary`, `contentinfo`, `search`, `form`, `region`

**Common attributes**:

- **Labels**: `aria-label`, `aria-labelledby`, `aria-describedby`
- **States**: `aria-expanded`, `aria-selected`, `aria-checked`, `aria-pressed`, `aria-disabled`, `aria-hidden`
- **Live regions**: `aria-live` (`polite` or `assertive`), `aria-atomic`
- **Relationships**: `aria-controls`, `aria-owns`, `aria-haspopup`
- **Other**: `aria-current`, `aria-invalid`, `aria-required`

#### ARIA Pattern: Accessible Modal

```tsx
function Modal({ isOpen, onClose, title, children }) {
  const titleId = useId();
  useEffect(() => {
    if (isOpen) {
      const focusable = modalRef.current.querySelectorAll(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
      );
      focusable[0]?.focus();
      const handleKey = (e: KeyboardEvent) => {
        if (e.key === "Tab") {
          if (e.shiftKey && document.activeElement === focusable[0]) {
            e.preventDefault(); focusable[focusable.length - 1]?.focus();
          } else if (!e.shiftKey && document.activeElement === focusable[focusable.length - 1]) {
            e.preventDefault(); focusable[0]?.focus();
          }
        }
        if (e.key === "Escape") onClose();
      };
      document.addEventListener("keydown", handleKey);
      return () => document.removeEventListener("keydown", handleKey);
    }
  }, [isOpen, onClose]);
  if (!isOpen) return null;
  return (
    <div role="dialog" aria-modal="true" aria-labelledby={titleId} ref={modalRef}>
      <h2 id={titleId}>{title}</h2>
      {children}
      <button onClick={onClose} aria-label="Close dialog">Close</button>
    </div>
  );
}
```

### 4. Keyboard Navigation

#### Standard Patterns

| Pattern | Keys | Behavior |
|---------|------|----------|
| **Tab navigation** | Tab / Shift+Tab | Move focus forward/backward through interactive elements |
| **Arrow navigation** | Arrow keys | Navigate within composite widgets (menus, tabs, lists) |
| **Action keys** | Enter / Space | Activate buttons and links |
| **Escape** | Esc | Close dialogs, cancel operations |
| **Home/End** | Home / End | Move to first/last item in a group |

#### Widget Keyboard Patterns

- **Tabs**: Arrow Left/Right between tabs, Home/End for first/last, Tab exits to panel
- **Menu**: Arrow Up/Down for items, Arrow Right opens submenu, Escape closes
- **Dialog**: Tab/Shift+Tab cycles (focus trap), Escape closes, focus returns to trigger
- **Accordion**: Tab between headers, Enter/Space toggles panel
- **Combobox**: Arrow Down opens listbox, Enter selects, Escape closes

#### Focus Management Essentials

```typescript
// Focus trap pattern
const previousFocus = document.activeElement;  // 1. Save previous focus
modal.querySelector('button, [href], input')?.focus();  // 2. Focus first element
// 3. On Tab, cycle within trap (see Modal example above)
previousFocus?.focus();  // 4. On close, restore focus

// Roving tabindex: only active item has tabindex="0", rest have "-1"
<div role="toolbar">
  <button tabindex="0">Cut</button>
  <button tabindex="-1">Copy</button>
  <button tabindex="-1">Paste</button>
</div>

// Skip link (first focusable element on page)
<a href="#main-content" className="skip-link">Skip to main content</a>
<main id="main-content" tabindex="-1">...</main>
```

### 5. Screen Reader Testing

| Reader | Platform | Shortcuts |
|--------|----------|-----------|
| **NVDA** | Windows (Free) | Start: NVDA+Down, Next heading: H, Landmarks: D, Elements list: NVDA+F7 |
| **VoiceOver** | macOS (Built-in) | Toggle: Cmd+F5, Next: VO+Right, Rotor: VO+U, Activate: VO+Space |
| **TalkBack** | Android (Built-in) | Next: Swipe right, Activate: Double tap |

**Screen reader checklist**:

- Page title announces on load; headings create logical h1-h6 hierarchy
- Landmark regions are labeled; skip links bypass repetitive content
- All images have appropriate alt text; links have descriptive text
- Dynamic updates announced via `aria-live` regions
- Form labels associated with inputs; errors linked via `aria-describedby`
- Modal focus trapped; custom widgets announce state changes

### 6. Color Contrast and Visual Accessibility

**WCAG contrast requirements**:

| Level | Normal text | Large text (18pt+ or 14pt bold) |
|-------|------------|--------------------------------|
| AA | 4.5:1 | 3:1 |
| AAA | 7:1 | 4.5:1 |

**Essential CSS for visual accessibility**:

```css
/* Visible focus for keyboard users */
:focus-visible {
  outline: 2px solid #005fcc;
  outline-offset: 2px;
}
/* High contrast mode */
@media (prefers-contrast: high) {
  :focus-visible { outline: 3px solid currentColor; outline-offset: 3px; }
}
/* Reduced motion */
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 0.01ms !important;
    transition-duration: 0.01ms !important;
  }
}
```

### 7. Automated Testing

```typescript
// Jest + axe-core
import { render } from '@testing-library/react';
import { axe, toHaveNoViolations } from 'jest-axe';
expect.extend(toHaveNoViolations);

it('has no a11y violations', async () => {
  const { container } = render(<MyComponent />);
  expect(await axe(container)).toHaveNoViolations();
});

// Playwright + axe
import AxeBuilder from '@axe-core/playwright';
test('no a11y issues', async ({ page }) => {
  await page.goto('/');
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21aa']).analyze();
  expect(results.violations).toEqual([]);
});
```

## Best Practices

1. **Start with semantic HTML** — proper elements provide built-in accessibility
2. **First rule of ARIA** — do not use ARIA if semantic HTML can do it
3. **Maintain focus management** — logical focus order, visible indicators, focus trapping in modals
4. **Keyboard first** — all functionality must work without a mouse
5. **Test with real tools** — screen readers (NVDA, VoiceOver), keyboard only, axe, Lighthouse
6. **Provide text alternatives** — all images need alt text (empty `alt=""` for decorative)
7. **Design for inclusion** — consider color blindness, low vision, motor and cognitive disabilities
8. **Test early and often** — include accessibility checks in every code review

## Quick Reference: Common ARIA Patterns

```html
<!-- Toggle button -->
<button aria-pressed="true">Mute</button>

<!-- Icon button (always needs label) -->
<button aria-label="Close">x</button>

<!-- Expandable section -->
<button aria-expanded="false" aria-controls="panel-id">Toggle</button>
<div id="panel-id" hidden>Content</div>

<!-- Alert (announces immediately) -->
<div role="alert">Error: Form submission failed</div>

<!-- Status (announces politely) -->
<div role="status" aria-live="polite">5 new messages</div>

<!-- Tabs -->
<div role="tablist">
  <button role="tab" aria-selected="true" aria-controls="p1">Tab 1</button>
  <button role="tab" aria-selected="false" aria-controls="p2">Tab 2</button>
</div>
<div id="p1" role="tabpanel">Panel 1</div>
<div id="p2" role="tabpanel" hidden>Panel 2</div>

<!-- Form field with error -->
<label for="email">Email</label>
<input id="email" type="email" aria-describedby="hint err" aria-invalid="true" aria-required="true" />
<span id="hint">We will never share your email</span>
<span id="err" role="alert">Please enter a valid email</span>

<!-- Visually hidden (screen reader only) -->
<span class="sr-only">Screen reader text</span>
<!-- CSS: .sr-only { position:absolute; width:1px; height:1px; overflow:hidden; clip:rect(0,0,0,0); } -->

<!-- Hidden from screen readers -->
<div aria-hidden="true">Decorative content</div>
```

## Alt Text Guidelines

- **Informative images**: describe the information (`alt="Sales increased 25% Q1 to Q2"`)
- **Functional images**: describe the action (`alt="Print this page"`)
- **Decorative images**: empty alt (`alt=""`)
- **Complex images**: use `aria-describedby` pointing to a detailed description
- **Images of text**: avoid when possible; otherwise reproduce the text in alt

## Testing Checklist

**Automated (every PR)**: axe-core, Lighthouse accessibility score, eslint-plugin-jsx-a11y

**Manual (before release)**: keyboard-only navigation, screen reader testing, 200% browser zoom, color contrast check, focus indicator visibility, reduced motion and high contrast mode
