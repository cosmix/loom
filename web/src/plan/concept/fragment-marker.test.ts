import { expect, it } from "vitest";

import conceptCss from "@/plan/concept/concept.css?raw";

/** The innermost `selector { body }` blocks of a stylesheet (nested at-rules yield their rules). */
function rules(css: string): { selector: string; body: string }[] {
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((match) => ({
    selector: match[1]!.trim(),
    body: match[2]!,
  }));
}

const MARKER = /(box-shadow|background|border-bottom)\s*:[^;]*var\(--sentence-colour\)/;
const WRAP = ".concept-sentence-wrap";
const FRAGMENT = `${WRAP}[data-fragment]`;

it("a fragment's wrap draws a marker in its sentence colour at rest", () => {
  const marked = rules(conceptCss).filter(
    (rule) => rule.selector.includes(FRAGMENT) && MARKER.test(rule.body),
  );

  expect(marked).toHaveLength(1);
  // Quieter than the hover edge (85%) so hover replaces, never doubles, the rest marker.
  const strength = Number(/var\(--sentence-colour\)\s+(\d+)%/.exec(marked[0]!.body)?.[1]);
  expect(strength).toBeGreaterThanOrEqual(30);
  expect(strength).toBeLessThan(85);
});

it("a single sentence's wrap draws no marker at rest", () => {
  const atRest = rules(conceptCss).filter(
    (rule) =>
      rule.selector.includes(WRAP) &&
      !rule.selector.includes("[data-fragment]") &&
      !rule.selector.includes(":hover") &&
      !rule.selector.includes(":has("),
  );

  expect(atRest.length).toBeGreaterThan(0);
  expect(atRest.filter((rule) => MARKER.test(rule.body))).toEqual([]);
});
