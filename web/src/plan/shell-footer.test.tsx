import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { ActivityPill } from "@/plan/header/activity";
import { PlanShell } from "@/plan/shell";
import { checklistAtom, unauthorizedAtom } from "@/plan/state/atoms";

vi.mock("@/plan/header/header", () => ({
  PlanHeader: () => <ActivityPill compact={false} />,
}));
vi.mock("@/plan/help/help-dialog", () => ({ PlannerHelpDialog: () => null }));
vi.mock("@/plan/settings/settings-dialog", () => ({ SettingsDialog: () => null }));
vi.mock("@/plan/run/dashboard-button", () => ({
  DashboardButton: () => <a href="/dashboard">Open dashboard</a>,
}));

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

/** The pill is named for what runs, or "Activity: idle". */
const PILL = /^(Activity:|\d+ in progress)/;
const PANEL_URL = "/api/summaries/panel";

/** Answers each panel GET with the next of `answers`, the last one repeating; counts the GETs. */
function stubPanel(...answers: Array<Response | (() => Response)>) {
  let served = 0;
  const fetchStub = vi.fn(async (input: RequestInfo | URL) => {
    const url = new URL(String(input), "http://localhost");
    if (url.pathname !== PANEL_URL) return new Response("{}");
    const answer = answers[Math.min(served++, answers.length - 1)]!;
    return typeof answer === "function" ? answer() : answer;
  });
  vi.stubGlobal("fetch", fetchStub);
  const panelRequests = () => fetchStub.mock.calls.filter(([url]) => String(url) === PANEL_URL);
  return panelRequests;
}

const ok = () => new Response(JSON.stringify(panelFixture));
const failed = () => new Response(JSON.stringify({ error: "boom" }), { status: 500 });

function renderShell(store = createStore()) {
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
}

it("one summary feed serves the footer and the pill", async () => {
  const panelRequests = stubPanel(ok);
  renderShell();

  const footer = within(await screen.findByRole("contentinfo"));
  expect(await footer.findByRole("img", { name: /^claude 5h window/ })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: PILL }));
  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(within(dialog).getByRole("button", { name: /^Summary rebuilds:/ })).toBeTruthy();

  expect(panelRequests()).toHaveLength(1);
});

it("the pill's Retry reloads the shell's feed and the footer fills in", async () => {
  const panelRequests = stubPanel(failed, ok);
  renderShell();
  fireEvent.click(screen.getByRole("button", { name: PILL }));
  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  const row = await within(dialog).findByRole("button", {
    name: "Summary rebuilds: Summary rebuild status unavailable",
  });
  fireEvent.click(row);
  expect(screen.queryByRole("contentinfo")).toBeNull();

  fireEvent.click(await within(dialog).findByRole("button", { name: "Retry" }));

  expect(await screen.findByRole("contentinfo")).toBeTruthy();
  expect(panelRequests()).toHaveLength(2);
});

it("the footer is absent while the plan shell shows only the sign-in notice", () => {
  const panelRequests = stubPanel(ok);
  const store = createStore();
  store.set(unauthorizedAtom, true);
  renderShell(store);

  expect(screen.queryByRole("contentinfo")).toBeNull();
  expect(panelRequests()).toHaveLength(0);
});

it("the footer waits for the feed before it says anything about limits", async () => {
  const panelRequests = stubPanel(failed);
  renderShell();

  await waitFor(() => expect(panelRequests()).toHaveLength(1));
  expect(screen.queryByRole("contentinfo")).toBeNull();
  expect(screen.queryByText(/limits unavailable/)).toBeNull();
});
