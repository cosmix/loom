import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import docFixture from "@/plan/api/fixtures/files-doc.json";
import { PageHeading } from "@/plan/concept/page-heading";
import { pageWith } from "@/plan/concept/page-heading-kit";
import { refreshDocView } from "@/plan/concept/passages";

import { authorStore, makeDoc, mount } from "./doc-heading-kit";

const PATH = "doc/product/demo/section_01.md";
const SECTIONS = [{ level: 2, title: "Purpose", anchor: "purpose", start_line: 3, end_line: 6 }];
const header = "# Title\n\n## Purpose\n";
const ONE = `${header}We ship it. (My call.)\n`;
const TWO = `${header}We ship it. (My call.)\nPick a store. (My call: Postgres.)\n`;
const NONE = `${header}Nothing is left open.\n`;

const SECOND = "doc/product/demo/section_02.md";

let served: string;
/** Holds the second page's document back until the test releases it. */
let gate: Promise<void> | null;

beforeEach(() => {
  served = ONE;
  gate = null;
  vi.stubGlobal("fetch", async (request: Request | string) => {
    const second = String(request instanceof Request ? request.url : request).includes(
      "section_02",
    );
    if (second) await gate;
    const body = { ...docFixture, path: second ? SECOND : PATH, text: served, sections: SECTIONS };
    return new Response(JSON.stringify(body), { headers: { "content-type": "application/json" } });
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function fileDoc(text: string) {
  return makeDoc({ kind: "section", plan: null, path: PATH, text, sections: SECTIONS });
}

it("file mode chips the My calls as a warning link to the first one", () => {
  mount(fileDoc(TWO), authorStore(), `/files/${PATH.replace("doc/", "")}`);
  const chip = screen.getByRole("link", { name: "2 My calls" });
  expect(chip.getAttribute("href")).toBe("/files/product/demo/section_01.md#purpose");
  expect(chip.getAttribute("data-tone")).toBe("warning");
  expect(chip.querySelector("svg.lucide-flag")).toBeTruthy();
});

it("file mode says 1 My call in the singular", () => {
  mount(fileDoc(ONE));
  expect(screen.getByRole("link", { name: "1 My call" })).toBeTruthy();
});

it("file mode shows no chip when the text holds none", () => {
  mount(fileDoc(NONE));
  expect(screen.queryByText(/My call/)).toBeNull();
});

function showPage(store = authorStore()) {
  const page = pageWith({ key: "demo/product/section_01", path: PATH, feature: "demo" });
  render(
    <Provider store={store}>
      <MemoryRouter>
        <PageHeading page={page} />
      </MemoryRouter>
    </Provider>,
  );
  return store;
}

it("a summary page links its My calls inside the feature, once the text has loaded", async () => {
  served = TWO;
  showPage();
  const chip = await screen.findByRole("link", { name: "2 My calls" });
  expect(chip.getAttribute("href")).toBe("/f/demo/source/product/demo/section_01.md#purpose");
});

it("a summary page follows the document: the chip updates and goes at zero", async () => {
  const store = showPage();
  await screen.findByRole("link", { name: "1 My call" });

  served = TWO;
  act(() => refreshDocView(store, PATH));
  await screen.findByRole("link", { name: "2 My calls" });

  served = NONE;
  act(() => refreshDocView(store, PATH));
  await waitFor(() => expect(screen.queryByText(/My call/)).toBeNull());
});

it("a summary page keeps its heading across a navigation and never shows the previous page's calls", async () => {
  const store = authorStore();
  const first = pageWith({ key: "demo/product/section_01", path: PATH, feature: "demo" });
  const second = pageWith({ key: "demo/product/section_02", path: SECOND, feature: "demo" });
  const view = (page: typeof first) => (
    <Provider store={store}>
      <MemoryRouter>
        <PageHeading page={page} />
      </MemoryRouter>
    </Provider>
  );
  const { rerender } = render(view(first));
  await screen.findByRole("link", { name: "1 My call" });
  const heading = screen.getByRole("heading", { level: 1 });

  let release = () => {};
  gate = new Promise((resolve) => (release = resolve));
  served = TWO;
  rerender(view(second));
  expect(screen.getByRole("heading", { level: 1 })).toBe(heading);
  expect(screen.queryByText(/My call/)).toBeNull();

  release();
  await screen.findByRole("link", { name: "2 My calls" });
  expect(screen.getByRole("heading", { level: 1 })).toBe(heading);
});
