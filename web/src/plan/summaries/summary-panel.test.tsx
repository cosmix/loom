import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
import { panelLoadAtom } from "./state";
import { SummaryPanel } from "./summary-panel";

const NOW_SECS = 1793261460;
const ADA = { public_id: "0123456789abcdef", name: "Ada" };
const DANA = { public_id: "bbbbbbbbbbbbbbbb", name: "Dana" };
const LAST = costLineSchema.parse(panelFixture.last);

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
  vi.restoreAllMocks();
});

it("lists the running rebuild's progress and ids", () => {
  show();

  const running = within(screen.getByRole("region", { name: "Running rebuild" }));
  expect(running.getByText("Rebuilding 1 of 3")).toBeTruthy();
  expect(running.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
    "demo/product/section_01",
    "demo/product/section_02",
    "overview/demo/product",
  ]);
});

it("lists queued ids", () => {
  show();

  const queue = within(screen.getByRole("region", { name: "Queue" }));
  expect(queue.getByRole("listitem").textContent).toBe("demo/product/section_03");
});

it("pairs each failed id with its error", () => {
  show({
    failed: [
      { id: "demo/product/section_04", error: "Summary failed to seal" },
      { id: "demo/product/section_05", error: "Session stopped" },
    ],
  });

  const failed = within(screen.getByRole("region", { name: "Failed rebuilds" }));
  expect(failed.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
    "demo/product/section_04Summary failed to seal",
    "demo/product/section_05Session stopped",
  ]);
});

it("shows a failed start as an alert", () => {
  show({ start_error: "Could not start the rebuild session" });

  expect(screen.getByRole("alert").textContent).toBe("Could not start the rebuild session");
});

it("keeps the last cost's three labelled figures in one element", () => {
  show();

  const last = within(screen.getByRole("region", { name: "Last rebuild" }));
  expect(last.getByText("fresh 1,840 · cache reads 9,200 · output 760")).toBeTruthy();
});

it.each([
  [{ cache_read: 9200, output: 760 }, "fresh unknown · cache reads 9,200 · output 760"],
  [{ fresh_input: 1840, output: 760 }, "fresh 1,840 · cache reads unknown · output 760"],
  [{ fresh_input: 1840, cache_read: 9200 }, "fresh 1,840 · cache reads 9,200 · output unknown"],
])("shows unknown for absent cost fields: %s", (tokens, expected) => {
  show({ last: { ...LAST, tokens } });

  const last = within(screen.getByRole("region", { name: "Last rebuild" }));
  expect(last.getByText(expected)).toBeTruthy();
});

it("shows unknown when there is no last rebuild", () => {
  show({ last: null });

  const last = within(screen.getByRole("region", { name: "Last rebuild" }));
  expect(last.getByText("fresh unknown · cache reads unknown · output unknown")).toBeTruthy();
});

it("shows today and seven-day totals with formatted figures", () => {
  show();

  const totals = within(screen.getByRole("region", { name: "Token totals" }));
  expect(totals.getByText("Today").parentElement?.textContent).toBe(
    "Todayfresh 12,400 · cache reads 68,000 · output 4,800",
  );
  expect(totals.getByText("7 days").parentElement?.textContent).toBe(
    "7 daysfresh 74,600 · cache reads 412,000 · output 29,600",
  );
});

it("distinguishes absent total fields from recorded zeroes", () => {
  show({ today: {}, week: { fresh_input: 0, cache_read: 0, output: 0 } });

  const totals = within(screen.getByRole("region", { name: "Token totals" }));
  expect(totals.getByText("fresh unknown · cache reads unknown · output unknown")).toBeTruthy();
  expect(totals.getByText("fresh 0 · cache reads 0 · output 0")).toBeTruthy();
});

it("renders the shared quota meters", () => {
  show();

  expect(screen.getByRole("img", { name: /^claude 5h window 48% used/ })).toBeTruthy();
  expect(screen.getByRole("img", { name: /^codex 7d window 63% used/ })).toBeTruthy();
});

it("shows the panel's read-only reason without action controls", () => {
  show({ read_only: "tmux is not installed" });

  expect(screen.getByText("tmux is not installed")).toBeTruthy();
  expect(screen.getByText("plans/loom-rebuilds")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
});

it("blocks Release with the current authoring reason", () => {
  const fetchMock = vi.fn<typeof fetch>();
  vi.stubGlobal("fetch", fetchMock);
  show({}, DANA);

  const button = screen.getByRole("button", { name: "Release" });
  fireEvent.click(button);

  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(button.getAttribute("aria-describedby") ?? "")?.textContent).toBe(
    "Dana holds authoring",
  );
  expect(fetchMock).not.toHaveBeenCalled();
});

it("posts only the selected terminal claim to the release endpoint", async () => {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockResolvedValue(new Response(JSON.stringify(panelFixture), { status: 200 }));
  vi.stubGlobal("fetch", fetchMock);
  const { store } = show({ terminal_claims: ["plans/loom-rebuilds", "overview/demo/product"] });
  const claims = within(screen.getByRole("region", { name: "Terminal claims" }));
  const selected = claims.getByText("plans/loom-rebuilds").closest("li");
  if (!selected) throw new Error("Missing terminal claim row");

  fireEvent.click(within(selected).getByRole("button", { name: "Release" }));

  await waitFor(() => expect(store.get(panelLoadAtom).status).toBe("ready"));
  expect(fetchMock).toHaveBeenCalledExactlyOnceWith("/api/summaries/release", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ids: ["plans/loom-rebuilds"] }),
    credentials: "same-origin",
  });
  expect(screen.queryByRole("alert")).toBeNull();
});

it("shows a release error in an alert", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(
      new Response(JSON.stringify({ error: "conflict", message: "Claim is still running" }), {
        status: 409,
      }),
    ),
  );
  show();

  fireEvent.click(screen.getByRole("button", { name: "Release" }));

  expect((await screen.findByRole("alert")).textContent).toBe("Claim is still running");
});
