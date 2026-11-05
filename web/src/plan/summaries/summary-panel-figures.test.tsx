import { act, cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { serverInfoSchema } from "@/plan/api/schema";
import {
  costLineSchema,
  summaryPanelSchema,
  type SummaryPanel as SummaryPanelData,
} from "@/plan/api/schema-summaries";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { stubMotion } from "./motion-stub";
import { SummaryPanel } from "./summary-panel";

const NOW_SECS = 1793261460;
const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const LAST = costLineSchema.parse(panelFixture.last);

function show(fields: Partial<SummaryPanelData> = {}) {
  const store = createStore();
  initClient(store);
  store.set(youAtom, ADA);
  store.set(serverInfoAtom, { ...serverInfoSchema.parse(helloFixture.server), read_only: [] });
  store.set(authoringAtom, { holder: { ...ADA, away_since: null, frees_at: null } });
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

/** Every labelled figure in `scope`: each `dt` with the `dd` beside it. */
function figuresIn(scope: HTMLElement): Record<string, string | null> {
  const terms = Array.from(scope.querySelectorAll(".sp-figure > dt"));
  return Object.fromEntries(
    terms.map((term) => [term.textContent, term.nextElementSibling?.textContent ?? null]),
  );
}

const lastRebuild = () => screen.getByRole("region", { name: "Last rebuild" });
const period = (name: string) =>
  within(screen.getByRole("region", { name: "Token totals" })).getByRole("group", { name });
const total = (name: string) => period(name).querySelector(".sp-total-figure")!.textContent;

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("pairs each of the last cost's labels with its figure", () => {
  show();

  expect(figuresIn(lastRebuild())).toEqual({
    fresh: "1,840",
    "cache reads": "9,200",
    output: "760",
  });
});

it("captions the last rebuild with its age and its absolute time", () => {
  show({ last: { ...LAST, time: "2026-10-29T08:01:00Z" } });

  const time = lastRebuild().querySelector("time");
  expect(time?.getAttribute("dateTime")).toBe("2026-10-29T08:01:00Z");
  expect(time?.textContent).toBe("10 min ago");
  expect(time?.getAttribute("title")).toBe(new Date("2026-10-29T08:01:00Z").toLocaleString());
});

it("marks the last rebuild ok or failed and counts its documents", () => {
  show({ failed: [] });
  expect(within(lastRebuild()).getByText("Rebuilt")).toBeTruthy();
  expect(lastRebuild().querySelector(".sp-dot")?.getAttribute("data-state")).toBe("ok");
  expect(within(lastRebuild()).getByText("1 document")).toBeTruthy();
  cleanup();

  show({ last: { ...LAST, summaries: ["a", "b", "c"] } });
  expect(within(lastRebuild()).getByText("Failed")).toBeTruthy();
  expect(lastRebuild().querySelector(".sp-dot")?.getAttribute("data-state")).toBe("failed");
  expect(within(lastRebuild()).getByText("3 documents")).toBeTruthy();
});

it.each([
  [
    { cache_read: 9200, output: 760 },
    { fresh: "unknown", "cache reads": "9,200", output: "760" },
  ],
  [
    { fresh_input: 1840, output: 760 },
    { fresh: "1,840", "cache reads": "unknown", output: "760" },
  ],
  [
    { fresh_input: 1840, cache_read: 9200 },
    { fresh: "1,840", "cache reads": "9,200", output: "unknown" },
  ],
])("shows unknown for absent cost fields: %s", (tokens, expected) => {
  show({ last: { ...LAST, tokens } });

  expect(figuresIn(lastRebuild())).toEqual(expected);
});

it("shows None yet and no figures when there is no last rebuild", () => {
  show({ last: null });

  expect(within(lastRebuild()).getByText("None yet")).toBeTruthy();
  expect(figuresIn(lastRebuild())).toEqual({});
  expect(lastRebuild().querySelector("time")).toBeNull();
});

it("shows today and seven-day totals with every component in the legend", () => {
  show();

  expect(total("Today")).toBe("88,400");
  expect(figuresIn(period("Today"))).toEqual({
    fresh: "12,400",
    "cache reads": "68,000",
    "cache writes": "3,200",
    output: "4,800",
  });
  expect(total("7 days")).toBe("534,400");
  expect(figuresIn(period("7 days"))).toEqual({
    fresh: "74,600",
    "cache reads": "412,000",
    "cache writes": "18,200",
    output: "29,600",
  });
});

it("distinguishes absent total fields from recorded zeroes", () => {
  show({ today: {}, week: { fresh_input: 0, cache_read: 0, output: 0 } });

  expect(total("Today")).toBe("unknown");
  expect(figuresIn(period("Today"))).toEqual({
    fresh: "unknown",
    "cache reads": "unknown",
    "cache writes": "unknown",
    output: "unknown",
  });
  expect(total("7 days")).toBe("0");
  expect(figuresIn(period("7 days"))).toEqual({
    fresh: "0",
    "cache reads": "0",
    "cache writes": "unknown",
    output: "0",
  });
  expect(within(period("7 days")).queryAllByRole("img")).toEqual([]);
});

it("counts the totals up from 0 while the legend and the last rebuild never animate", () => {
  stubMotion((query) => query.includes("no-preference"));
  vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
  show();

  expect(total("Today")).toBe("0");
  expect(total("7 days")).toBe("0");
  expect(figuresIn(period("Today")).fresh).toBe("12,400");
  expect(figuresIn(lastRebuild()).fresh).toBe("1,840");

  act(() => {
    vi.advanceTimersByTime(400);
  });
  expect(total("Today")).toBe("88,400");
  expect(total("7 days")).toBe("534,400");
});

it("draws no provider limit meters: the footer is their one place", () => {
  show();

  expect(screen.queryByRole("img", { name: /window .*% used/ })).toBeNull();
});
