---
name: loom-i18n
description: Internationalization and localization patterns for multi-language applications. Use when implementing translation systems, locale-specific formatting (dates, numbers, currency), RTL layouts, pluralization, or language switching with libraries like i18next, react-intl, FormatJS, or gettext.
triggers:
  - i18n
  - internationalization
  - l10n
  - localization
  - translation
  - translate
  - locale
  - language
  - multilingual
  - multi-language
  - RTL
  - right-to-left
  - LTR
  - bidirectional
  - pluralization
  - plural forms
  - date format
  - time format
  - number format
  - currency format
  - timezone
  - i18next
  - react-intl
  - FormatJS
  - gettext
  - ICU MessageFormat
  - message format
  - language detection
  - language switching
  - Accept-Language
  - locale fallback
  - translation keys
  - translation files
  - JSON translations
  - PO files
  - YAML translations
  - react i18n
  - React localization
  - format date
  - format number
  - format currency
  - format relative time
  - Intl API
  - NumberFormat
  - DateTimeFormat
  - RTL CSS
  - logical properties
  - direction-aware
  - language code
  - region code
  - locale identifier
  - BCP47
  - ISO 639
  - translation extraction
  - pseudo-localization
  - namespace
  - translation namespace
  - Intl.Collator
  - Intl.PluralRules
---

# Internationalization (i18n)

## Overview

Designing software to adapt to languages/regions without code changes (i18n), then adapting it per-locale (l10n). Covers translation architecture, ICU pluralization, `Intl`-based formatting, RTL/bidi, and libraries (i18next, react-intl/FormatJS, gettext).

## The Rules That Prevent Rework

1. **Never concatenate translated fragments.** Word order, gender agreement, and grammar differ per language. Use one full-sentence key with named placeholders; the translator controls order.

   ```javascript
   // Wrong — impossible to translate; order is baked into code
   t("You have") + " " + count + " " + t("new messages");
   // Right — one message, interpolation + plural inside it
   t("inbox.newMessages", { count }); // "{count, plural, one {# new message} other {# new messages}}"
   ```

2. **Pluralize with CLDR categories, never `if (count === 1)`.** English has 2 forms; Russian/Polish 3–4; Arabic 6 (`zero one two few many other`). Which categories a language uses is defined by CLDR — use ICU MessageFormat / `Intl.PluralRules`, not hand-rolled logic.
3. **Format with `Intl`, never by hand.** Decimal/grouping separators, currency placement, date order, and calendars are locale data you will get wrong manually (`1,234.56` en-US vs `1.234,56` de-DE vs `1 234,56` fr-FR).
4. **A locale is language + region.** `en-US` vs `en-GB`: `12/25/2024` vs `25/12/2024`, `color` vs `colour`, different currency. Store and resolve full BCP-47 tags; fall back language-only → default.
5. **Store timestamps in UTC, format at display time** in the user's timezone/locale. Never store locale-formatted strings.
6. **RTL is not just `direction`.** Use CSS logical properties, isolate bidirectional runs, and mirror directional icons.

## Architecture

```text
src/locales/{en,de,ar}/{common,products,errors}.json   # one dir per locale, split by namespace
src/i18n/config.ts
```

- Externalize every user-facing string. Hardcoded text is the #1 bug — catch it with pseudolocalization (below) and lint rules.
- **Namespaces** split catalogs by feature/route so you lazy-load only what a view needs (huge for bundle size on large apps).
- **Fallback chain**: `de-AT → de → en`. Configure it; missing keys should degrade, not render blank or crash.
- Keys are **descriptive and hierarchical** (`products.card.addToCart`), never the source English text (brittle: fixing a typo breaks every locale's lookup). Add context when a word is ambiguous (`button.close` vs `proximity.close`).

```typescript
interface LocaleConfig {
  code: string;                 // "en-US" (BCP-47)
  direction: "ltr" | "rtl";
  currency: string;             // ISO 4217, "USD"
}
```

## Translation File Formats

**JSON (i18next / react-intl)** — most common. i18next plural keys use a suffix per CLDR category:

```json
{
  "welcome": "Welcome, {{name}}!",
  "items_one": "{{count}} item",
  "items_other": "{{count}} items",
  "price": "Price: {{price, currency}}"
}
```

⚠ i18next v4+ derives the suffix (`_one`, `_other`, `_few`, `_many`, `_zero`, `_two`) from `Intl.PluralRules` for the active language — you must provide every category the language needs (Arabic needs all six), and you must pass `{ count }` for suffix selection to fire.

**ICU MessageFormat** (react-intl/FormatJS, and i18next via a plugin) puts logic inside the string:

```text
{count, plural, =0 {No items} one {# item} other {# items}}
{gender, select, male {He} female {She} other {They}} liked your post.
{place, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}
```

- `=0` is an *exact* match (checked before category); `one`/`other`/etc. are CLDR categories; `#` prints the number.
- Nest plural inside select for gender-correct counts. Don't over-nest — hand it to translators as one message.

**PO/gettext** (Python/PHP/Ruby): source string is the key, plurals via `msgid_plural` + `Plural-Forms` header. `xgettext` extracts, translators use Poedit.

## Formatting with `Intl`

One class over the standard `Intl` objects covers most needs. **Cache formatter instances** — constructing `Intl.*Format` is expensive; reuse per (locale, options).

```typescript
const nf = new Intl.NumberFormat("de-DE");
nf.format(1234567.89);                                              // "1.234.567,89"
new Intl.NumberFormat("de-DE", { style: "currency", currency: "EUR" }).format(99.9); // "99,90 €"
new Intl.NumberFormat("en", { notation: "compact" }).format(12000);// "12K"
new Intl.DateTimeFormat("ja-JP", { dateStyle: "long" }).format(d); // "2024年12月19日"
new Intl.RelativeTimeFormat("de", { numeric: "auto" }).format(-1, "day"); // "gestern"
new Intl.ListFormat("en", { type: "conjunction" }).format(["A","B","C"]); // "A, B, and C"
new Intl.PluralRules("ar").select(3);                              // "few"  → pick the message key
```

⚠ Gotchas:

- `dateStyle`/`timeStyle` can't be combined with individual component options (`year`, `month`, …) in one `DateTimeFormat` — pick one style. Pass `timeZone` for server-consistent output; default is the runtime's zone.
- Currency: pass an explicit ISO 4217 code; the locale controls *placement/symbol*, not *which* currency. `narrowSymbol` for "$" over "US$".
- Prefer `dateStyle` presets over custom component lists — presets already encode locale order (MDY vs DMY vs YMD); custom lists you assemble often leak English order.

## RTL & Bidirectional Text

**Use logical properties** so one stylesheet serves both directions — no `[dir=rtl]` overrides:

```css
.card {
  margin-inline-start: 1rem;   /* left in LTR, right in RTL */
  padding-inline: 1rem;
  border-inline-start: 3px solid;
  text-align: start;
}
.icon:dir(rtl) { transform: scaleX(-1); } /* mirror only directional icons: arrows, chevrons, back — NOT logos/checkmarks */
```

- Set `<html dir="rtl" lang="ar">`; flexbox/grid flow follows `dir` automatically.
- **Bidi isolation**: user-generated or opposite-direction text embedded in a sentence (an Arabic name in English UI, a phone number) can reorder surrounding punctuation. Wrap it in `<bdi>…</bdi>`, or `unicode-bidi: isolate`, or the Unicode isolates `⁨…⁩` (FSI/PDI) in plain strings. Numbers next to RTL text are the classic breakage.
- RTL testing must use *real* translated RTL content, not mirrored Lorem Ipsum — bidi bugs only surface with genuine strings.

## Locale Detection & Switching

Precedence: explicit URL/param → cookie/stored pref → `Accept-Language` (parse `q` weights, match against supported set) → default. Validate against your supported list before use.

```typescript
function parseAcceptLanguage(header: string): string[] {
  return header.split(",")
    .map(part => { const [code, q] = part.trim().split(";q="); return { code, q: parseFloat(q) || 1 }; })
    .sort((a, b) => b.q - a.q)
    .map(x => x.code);
}
```

On switch, update three things: the i18n instance, `document.documentElement.lang`, and `.dir`. Persist the choice. SPA switch should not require a reload.

## Sorting & Search

String comparison is locale-dependent — **never `.sort()` raw** for user-visible lists (default sorts by code point: `Z` < `a`, `ä` after `z`). Use `Intl.Collator`:

```typescript
const collator = new Intl.Collator("de", { sensitivity: "base", numeric: true });
list.sort(collator.compare); // locale-correct; numeric:true gives "file2" < "file10"
```

`sensitivity: "base"` ignores case/accents (good for search/dedup); `numeric: true` for natural number ordering. Reuse the collator instance.

## Libraries

**i18next** (framework-agnostic, plugin-rich):

```typescript
i18n.use(Backend).use(LanguageDetector).use(initReactI18next).init({
  fallbackLng: "en",
  supportedLngs: ["en", "de", "fr", "ar"],
  ns: ["common", "products"], defaultNS: "common",
  backend: { loadPath: "/locales/{{lng}}/{{ns}}.json" },   // lazy per-namespace
  detection: { order: ["querystring", "cookie", "navigator"], caches: ["cookie"] },
  interpolation: { escapeValue: false }, // React already escapes; escapeValue:true double-encodes
});
```

```tsx
const { t, i18n } = useTranslation(["products", "common"]);
t("products:price", { price });
t("common:items", { count });                 // plural via count
// Interpolation inside markup — use <Trans>, do NOT build strings from JSX children
<Trans i18nKey="products:promo" values={{ name }}>Check out <strong>{{ name }}</strong> today!</Trans>
```

⚠ `escapeValue: false` in React is correct (React escapes); leaving it `true` double-encodes. Outside React, keep escaping on to avoid XSS from interpolated values.

**react-intl / FormatJS** (ICU-native, standards-aligned):

```tsx
<IntlProvider locale={locale} messages={messages[locale]} defaultLocale="en">…</IntlProvider>
const intl = useIntl();
intl.formatMessage({ id: "app.items" }, { count: 5 });     // ICU plural in the message
<FormattedMessage id="app.greeting" values={{ name }} />
intl.formatNumber(1234.56, { style: "currency", currency: "EUR" });
```

Ships `@formatjs/cli` to extract messages and precompile ICU ASTs (faster runtime). Wire `onError` so missing IDs are caught in CI, not shipped blank.

**Python gettext:**

```python
t = gettext.translation("messages", localedir, languages=["de"], fallback=True)
_ = t.gettext; ngettext = t.ngettext
_("Welcome, %(name)s!") % {"name": user}
ngettext("%(count)d item", "%(count)d items", count) % {"count": count}
```

## Testing & QA

**Pseudolocalization** — the highest-ROI check. Transform the default locale into accented, expanded text to catch two bug classes at once:

```text
"Add to Cart"  →  "[!! Àdd tö Çårt ~~~~ ]"
```

- Untransformed on screen ⇒ a **hardcoded string** bypassing i18n.
- Padding (+30–40%, mimicking German/Finnish) ⇒ **truncation/overflow** before real translations exist.

Also test: longest language (German/Finnish) for overflow; a real RTL locale for layout+bidi; that number/date/currency render per locale (not just English); missing-key fallback path.

**Extraction tooling** (don't hand-roll AST walkers): `i18next-parser` for i18next catalogs, `@formatjs/cli extract` for react-intl, `xgettext`/Babel for gettext. Run in CI to fail on new untranslated keys and prune dead ones.

## Verify Before Done

- [ ] Zero hardcoded user-facing strings (pseudoloc pass is clean)
- [ ] No concatenated sentence fragments; each message is one key with named placeholders
- [ ] Plurals use ICU/`PluralRules` categories (every category the language needs), not `count === 1`
- [ ] All numbers/dates/currencies/lists via `Intl` (formatters cached), not manual formatting
- [ ] Timestamps stored UTC; formatted at display in user tz/locale
- [ ] Fallback chain configured; missing keys degrade gracefully (no blanks/crashes)
- [ ] RTL: logical CSS properties, `dir`+`lang` on `<html>`, bidi isolation on embedded runs, directional icons mirrored
- [ ] User-visible lists sorted via `Intl.Collator`, not raw `.sort()`
- [ ] Locales lazy-loaded by namespace; switch updates i18n + `lang` + `dir` without reload
- [ ] Interpolation escaping correct for the runtime (React: `escapeValue:false`; server: escape on)
- [ ] Verified in ≥2 locales incl. one long-word and one RTL, with real translated content
