import { render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";

import { TooltipProvider } from "@/components/ui/tooltip";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import { pageHref } from "@/plan/api/page-key";
import { conceptPageSchema, type KeyChip } from "@/plan/api/schema-concept";

import { DocumentKey } from "./document-key";
import { cite, pageOf, S1, sent, src } from "./reading-column-fixtures";
import { loadPage, mark, mount, pages } from "./reading-column-harness";

/** Setup for `reading.contracts.test.tsx`; assertions stay in the contract file. */

/** An overview chip for document `n` (1-based), titled "NN — Document n". */
function chipOf(n: number): KeyChip {
  const label = String(n).padStart(2, "0");
  return {
    doc: {
      id: `section_${label}`,
      title: `${label} — Document ${n}`,
      path: `doc/product/demo/section_${label}.md`,
      page: `demo/product/section_${label}`,
    },
    colour: n % 8,
    label,
    out_of_date: null,
    no_plan: false,
  };
}

/** Renders the document key of a product overview drawing on `count` documents. */
export function showKey(count: number) {
  const chips = Array.from({ length: count }, (_, index) => chipOf(index + 1));
  const page = conceptPageSchema.parse({ ...overviewFixture, key_chips: chips });
  render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <TooltipProvider>
          <DocumentKey page={page} pageKey={page.key} />
        </TooltipProvider>
      </MemoryRouter>
    </Provider>,
  );
  return chips.map((chip) => pageHref(chip.doc.page));
}

/** The links among `hrefs` that the screen shows. */
export const chipLinks = (hrefs: string[]) =>
  screen.queryAllByRole("link").filter((link) => hrefs.includes(link.getAttribute("href") ?? ""));

const PLANNING = [mark("section_01", "Planning", 1)];

/**
 * A product overview whose first sentence opens a run over a placed section page; the second
 * sentence has nothing to open.
 */
export function openableOverview() {
  const overview = pageOf("overview/demo/product", [
    sent("o", "Overview opens.", [cite("section_01", "x")], PLANNING),
    sent("p", "Plain closing line."),
  ]);
  const detail = pageOf("demo/product/section_01", [
    sent("x", "Detail opens.", [src(S1, "intro")]),
  ]);
  pages["demo/product/section_01"] = detail;
  const view = mount("overview/demo/product", overview);
  loadPage(view.store, "demo/product/section_01", detail);
  return view;
}
