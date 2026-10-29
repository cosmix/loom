import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import filesDocFixture from "@/plan/api/fixtures/files-doc.json";
import { routes } from "@/plan/router";

const TEXT =
  "# Planning\n\n## Intent\n\nA plan begins with **intent** written down.\n\n" +
  "## Stages\n\nStages list their files.\n";
const SECTIONS = [
  { level: 2, title: "Intent", anchor: "intent", start_line: 3, end_line: 6 },
  { level: 2, title: "Stages", anchor: "stages", start_line: 7, end_line: 10 },
];

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

// Serves the concept fixtures and one source document; anything else is a 404.
function stubFetch(input: RequestInfo | URL): Promise<Response> {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  const url = new URL(raw, "http://localhost");
  const pages: Record<string, unknown> = {
    "/api/concept/overview/demo/product": overviewFixture,
    "/api/concept/demo/product/section_01": summaryFixture,
    "/api/concept/demo/product/section_02": { ...summaryFixture, key: "demo/product/section_02" },
  };
  if (pages[url.pathname]) return Promise.resolve(json(pages[url.pathname]));
  if (
    url.pathname === "/api/doc" &&
    url.searchParams.get("path") === "product/demo/section_01.md"
  ) {
    const doc = { ...structuredClone(filesDocFixture), path: "doc/product/demo/section_01.md" };
    return Promise.resolve(json({ ...doc, title: "Planning", text: TEXT, sections: SECTIONS }));
  }
  return Promise.resolve(json({ error: "not_found", message: "not found" }, 404));
}

beforeEach(() => {
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(stubFetch));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("expansion chain from overview to passage to file mode", async () => {
  const store = createStore();
  initClient(store);
  const router = createMemoryRouter(routes, { initialEntries: ["/f/demo/product"] });
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  const column = await screen.findByRole("document", { name: "Reading column" });
  fireEvent.click(within(column).getByRole("button", { name: /^Teams plan work together/ }));
  const run = await within(column).findByRole("group", {
    name: "Detail of Teams plan work together and",
  });
  fireEvent.click(within(run).getByRole("button", { name: /^A plan starts from/ }));
  const header = await within(column).findByRole("link", { name: /section_01\.md.*Intent/ });
  expect(await within(column).findByText("intent", { selector: "strong" })).toBeTruthy();
  fireEvent.click(header);
  await waitFor(() =>
    expect(router.state.location.pathname).toBe("/files/product/demo/section_01.md"),
  );
  expect(router.state.location.hash).toBe("#intent");
});
