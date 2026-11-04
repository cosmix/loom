import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { PlanShell } from "@/plan/shell";
import { checklistAtom } from "@/plan/state/atoms";

vi.mock("@/plan/header/header", () => ({ PlanHeader: () => null }));
vi.mock("@/plan/help/help-dialog", () => ({ PlannerHelpDialog: () => null }));
vi.mock("@/plan/settings/settings-dialog", () => ({ SettingsDialog: () => null }));

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** Answers GET /api/summaries/panel with `panel`; every other request gets an empty object. */
function stubPanel(panel: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = new URL(String(input), "http://localhost");
      const body = url.pathname === "/api/summaries/panel" ? panel : {};
      return new Response(JSON.stringify(body), {
        headers: { "content-type": "application/json" },
      });
    }),
  );
}

async function footer() {
  const store = createStore();
  store.set(checklistAtom, checklistViewSchema.parse(checklistFixture.checklists[0]));
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={["/"]}>
        <Routes>
          <Route path="/" element={<PlanShell />}>
            <Route index element={<p>outlet content</p>} />
          </Route>
        </Routes>
      </MemoryRouter>
    </Provider>,
  );
  return within(await screen.findByRole("contentinfo"));
}

it("the footer shows Claude limits and says Codex limits are unavailable", async () => {
  stubPanel({ ...panelFixture, quota: { ...panelFixture.quota, codex: null } });
  const inside = await footer();

  expect(await inside.findByRole("img", { name: /^claude 5h window/ })).toBeTruthy();
  expect(await inside.findByText("Codex limits unavailable")).toBeTruthy();
});

it("the footer shows no unavailable note when both providers report", async () => {
  stubPanel(panelFixture);
  const inside = await footer();

  expect(await inside.findByRole("img", { name: /^claude 5h window/ })).toBeTruthy();
  expect(await inside.findByRole("img", { name: /^codex 7d window/ })).toBeTruthy();
  expect(inside.queryByText(/limits unavailable/)).toBeNull();
});
