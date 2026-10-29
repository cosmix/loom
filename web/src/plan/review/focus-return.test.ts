import { afterEach, expect, it } from "vitest";

import { focusReadingRegion, returnFocusTo } from "./focus-return";

afterEach(() => {
  document.body.replaceChildren();
});

// The shell's `<main>` contains the file view's scroll region, as in the real page.
function page({ withRegion }: { withRegion: boolean }) {
  const main = document.createElement("main");
  const region = document.createElement("div");
  region.setAttribute("data-reading-scroll", "");
  if (withRegion) main.append(region);
  document.body.append(main);
  return { main, region };
}

it("focuses the scroll region inside main, not main itself", () => {
  const { main, region } = page({ withRegion: true });

  focusReadingRegion();

  expect(document.activeElement).toBe(region);
  expect(main.hasAttribute("tabindex")).toBe(false);
});

it("falls back to main where there is no scroll region", () => {
  const { main } = page({ withRegion: false });

  focusReadingRegion();

  expect(document.activeElement).toBe(main);
});

it("removes the tabindex it added once the region loses focus", () => {
  const { region } = page({ withRegion: true });
  const other = document.createElement("button");
  document.body.append(other);

  focusReadingRegion();
  expect(region.getAttribute("tabindex")).toBe("-1");
  other.focus();

  expect(region.hasAttribute("tabindex")).toBe(false);
});

it("keeps a tabindex the region already had", () => {
  const { region } = page({ withRegion: true });
  region.tabIndex = 0;
  const other = document.createElement("button");
  document.body.append(other);

  focusReadingRegion();
  other.focus();

  expect(region.getAttribute("tabindex")).toBe("0");
});

it("returns focus to the reading region when the target is gone", () => {
  const { region } = page({ withRegion: true });
  const gone = document.createElement("button");

  returnFocusTo(gone);

  expect(document.activeElement).toBe(region);
});
