import { expect, it } from "vitest";

import railCss from "./rail.css?raw";
import treeCss from "./tree.css?raw";

/** The declarations of the first top-level rule whose selector is exactly `selector`. */
function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(?:^|\\n)${escaped} \\{([^}]*)\\}`).exec(css)?.[1] ?? "";
}

it.each([
  [treeCss, ".plan-tree__scroll"],
  [treeCss, ".plan-tree__row"],
  [railCss, ".plan-rail__item"],
])("%#: %s is the containing block of the sr-only words inside it", (css, selector) => {
  // Without it a row's sr-only status words (position: absolute) are placed against an ancestor
  // outside the tree's scroller, sit below the viewport, and the whole page scrolls.
  expect(ruleBody(css, selector)).toMatch(/\n\s*position: relative;/);
});
