import { cleanup, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";

import { clone, keyList, makeStubFetch, readingColumn, renderAt } from "./concept-route-support";

let pages: Record<string, unknown> = {};
const fetched: string[] = [];

beforeEach(() => {
  pages = {
    "overview/demo/product": clone(overviewFixture),
    "demo/product/section_01": clone(summaryFixture),
  };
  fetched.length = 0;
  window.sessionStorage.clear();
  vi.stubGlobal(
    "fetch",
    vi.fn(makeStubFetch(() => ({ pages, docs: {}, failing: false }), fetched)),
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** The text a sighted reader sees in `link`: its content without visually hidden parts. */
function visibleText(link: HTMLElement): string {
  const copy = link.cloneNode(true) as HTMLElement;
  for (const hidden of Array.from(copy.querySelectorAll(".sr-only"))) hidden.remove();
  return copy.textContent ?? "";
}

it("a key chip shows its document title", async () => {
  renderAt("/f/demo/product");
  await readingColumn();
  const chips = overviewFixture.key_chips;
  await waitFor(() => expect(within(keyList()).getAllByRole("link")).toHaveLength(chips.length));
  const links = within(keyList()).getAllByRole("link");
  links.forEach((link, i) => {
    const title = chips[i]!.doc.title;
    expect(visibleText(link), `chip ${i} shows "${title}"`).toContain(title);
  });
});
