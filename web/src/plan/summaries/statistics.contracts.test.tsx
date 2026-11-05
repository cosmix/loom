import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { serverInfoSchema } from "@/plan/api/schema";
import {
  summaryPanelSchema,
  type SummaryPanel as SummaryPanelData,
} from "@/plan/api/schema-summaries";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { SummaryPanel } from "@/plan/summaries/summary-panel";

const NOW_SECS = 1793261460;
const ADA = { public_id: "0123456789abcdef", name: "Ada" };

function show(fields: Partial<SummaryPanelData> = {}, holder = ADA) {
  const store = createStore();
  initClient(store);
  store.set(youAtom, ADA);
  store.set(serverInfoAtom, { ...serverInfoSchema.parse(helloFixture.server), read_only: [] });
  store.set(authoringAtom, { holder: { ...holder, away_since: null, frees_at: null } });
  const panel = { ...summaryPanelSchema.parse(panelFixture), ...fields };
  const rendered = render(
    <Provider store={store}>
      <TooltipProvider>
        <SummaryPanel panel={panel} nowSecs={NOW_SECS} />
      </TooltipProvider>
    </Provider>,
  );
  return { ...rendered, store };
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function period(name: string): HTMLElement {
  const totals = screen.getByRole("region", { name: "Token totals" });
  return within(totals).getByRole("group", { name });
}

const segmentLabels = (group: HTMLElement) =>
  within(group)
    .queryAllByRole("img")
    .map((segment) => segment.getAttribute("aria-label"))
    .sort();

it("the token bars draw one labelled segment per recorded component", () => {
  show();
  expect(segmentLabels(period("Today"))).toEqual([
    "cache reads 68,000",
    "cache writes 3,200",
    "fresh 12,400",
    "output 4,800",
  ]);
  expect(segmentLabels(period("7 days"))).toEqual([
    "cache reads 412,000",
    "cache writes 18,200",
    "fresh 74,600",
    "output 29,600",
  ]);
  cleanup();
  show({ today: { fresh_input: 500, output: 0 } } as Partial<SummaryPanelData>);
  expect(segmentLabels(period("Today"))).toEqual(["fresh 500"]);
});

it("reduced motion shows the token totals without counting up", () => {
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("reduce"),
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
  show();
  expect(period("Today").textContent).toContain("88,400");
  expect(period("7 days").textContent).toContain("534,400");
});
