# V1: a fragment reads as one unit at rest (R8, user decision, wave 2)

Tier: fable (`loom-senior-software-engineer`, model override fable). First invoke `frontend-design:frontend-design`, then read this brief, `common.md` and `web/src/plan/ui/DESIGN.md` in full. DESIGN.md and the targets below win over the skill's general advice.

## Context

W1 (wave 1) merges adjacent summary sentences that open to the same content into one **fragment**: one `role="button"` span inside one `.concept-sentence-wrap`, holding one inner span per sentence, then one set of marks and one chevron. The wrap of a fragment of two or more sentences carries `data-fragment="<count>"`; a single sentence's wrap carries none. Today's hover and focus treatment (`web/src/plan/concept/concept.css`, `.concept-sentence-wrap:has(> .concept-sentence:not(.concept-sentence-plain)):is(:hover, :has(:focus-visible))`: a 20% wash in `--sentence-colour`, a 2px inset bottom edge at 85%, marks scaled 1.35, the chevron faded in) already covers the whole fragment. Read `concept.css` as planner-visual left it: keep its chevron treatment.

The user's decision: a multi-sentence fragment is visibly one unit **before** hover; hover gives the full wash and one chevron.

## Fixed targets

1. At rest, `.concept-sentence-wrap[data-fragment]` shows a quiet marker that runs unbroken under or across all of its sentences, the spaces between them included (`box-decoration-break: clone` already set on the wrap keeps it continuous across line wraps), in `--sentence-colour` at low strength: for example a 1px bottom edge at about 35% (`color-mix(in oklch, var(--sentence-colour) 35%, transparent)`) or a tint at about 6%. Choose one; it must be clearly weaker than the hover wash and must not read as a link underline or as selection.
2. A single sentence (no `data-fragment`) looks exactly as it does today at rest.
3. Hover and keyboard focus on a fragment give today's full treatment over the whole fragment; the marker does not stack visibly on top of it (the hover edge replaces it).
4. Opened text, flashes and the fold caps are unchanged. Reduced motion: no new transition beyond the existing 120ms ones, all off under `prefers-reduced-motion`.
5. Every theme (Ledger, Walnut, Graphite): the marker is visible on `--background` (at least 1.5:1 against it for a line, so the unit reads) and theme tokens only, no colour literal.

## Build

Edit only `web/src/plan/concept/concept.css`: one rule for `.concept-sentence-wrap[data-fragment]` at rest, and its interplay with the existing hover rule. If DESIGN.md needs a line for the marker, report the text instead of editing DESIGN.md.

## Test

Add to a test file W1 wrote or extended for fragments (W1 reports its name; else a new `web/src/plan/concept/fragment-marker.test.ts`): read `@/plan/concept/concept.css?raw` and assert a rule whose selector contains `.concept-sentence-wrap[data-fragment]` declares a property drawing the marker (`box-shadow`, `background` or `border-bottom`) with `var(--sentence-colour)`, and that no rule for `.concept-sentence-wrap` without `[data-fragment]` and without `:hover`/`:has(` declares one. W1's DOM test covers the attribute itself.

## Proof

You cannot build the bundle. The stage's main agent shoots a real summary page with a fragment at rest and on hover in Ledger and Walnut (1440x900 and 390x844) and judges targets 1-5; a miss returns to a fresh fable worker with the shots.

## Run once at the end

`cd web && node_modules/.bin/vitest run src/plan/concept`

## Report

The rule you wrote with its values, why they meet targets 1-5 in each theme, the test, and any DESIGN.md line you propose.
