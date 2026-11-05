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

const RUNNING = summaryPanelSchema.parse(panelFixture).running!;

it("shows a Codex job's log path in mono, and nothing for a Claude session", () => {
  show();
  expect(screen.queryByText(/^Log:/)).toBeNull();
  cleanup();

  show({ running: { ...RUNNING, log_path: "/home/me/.loom/summary-2.log\u202E" } });
  const path = screen.getByText("/home/me/.loom/summary-2.log⟨U+202E⟩");
  expect(path.closest("code")?.className).toContain("font-mono");
  expect(
    within(screen.getByRole("region", { name: "Running rebuild" })).getByText(/^Log:/),
  ).toBeTruthy();
});

it("spells out control characters in every server string it shows", () => {
  const bad = (text: string) => `${text}\u202E`;
  show({
    running: { ...RUNNING, ids: [bad("run-id")] },
    queue: [bad("queued-id")],
    failed: [{ id: bad("failed-id"), error: bad("failed-error") }],
    start_error: bad("start-error"),
    read_only: bad("read-only"),
    terminal_claims: [bad("claim-id")],
  });
  for (const text of [
    "run-id",
    "queued-id",
    "failed-id",
    "failed-error",
    "start-error",
    "read-only",
    "claim-id",
  ])
    expect(screen.getByText(`${text}⟨U+202E⟩`)).toBeTruthy();
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

/** One span's labelled figures: each figure label's `dt` paired with the `dd` beside it. */
function figuresIn(scope: HTMLElement): Record<string, string | null> {
  const labels = ["fresh", "cache reads", "output"];
  const terms = Array.from(scope.querySelectorAll("dt")).filter((term) =>
    labels.includes(term.textContent ?? ""),
  );
  return Object.fromEntries(
    terms.map((term) => [term.textContent, term.nextElementSibling?.textContent ?? null]),
  );
}

const lastRebuild = () => screen.getByRole("region", { name: "Last rebuild" });
const span = (name: string) => {
  const totals = within(screen.getByRole("region", { name: "Token totals" }));
  const term = totals.getByText(name);
  expect(term.tagName).toBe("DT");
  return term.parentElement!;
};

it("pairs each of the last cost's labels with its figure", () => {
  show();

  expect(figuresIn(lastRebuild())).toEqual({
    fresh: "1,840",
    "cache reads": "9,200",
    output: "760",
  });
});

it("captions the last rebuild with its age", () => {
  show({ last: { ...LAST, time: "2026-10-29T08:01:00Z" } });

  const time = lastRebuild().querySelector("time");
  expect(time?.getAttribute("dateTime")).toBe("2026-10-29T08:01:00Z");
  expect(time?.textContent).toBe("10 min ago");
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

const UNKNOWN = { fresh: "unknown", "cache reads": "unknown", output: "unknown" };

it("shows unknown when there is no last rebuild", () => {
  show({ last: null });

  expect(within(lastRebuild()).getByText("None yet")).toBeTruthy();
  expect(figuresIn(lastRebuild())).toEqual(UNKNOWN);
});

it("shows today and seven-day totals with formatted figures", () => {
  show();

  expect(figuresIn(span("Today"))).toEqual({
    fresh: "12,400",
    "cache reads": "68,000",
    output: "4,800",
  });
  expect(figuresIn(span("7 days"))).toEqual({
    fresh: "74,600",
    "cache reads": "412,000",
    output: "29,600",
  });
});

it("distinguishes absent total fields from recorded zeroes", () => {
  show({ today: {}, week: { fresh_input: 0, cache_read: 0, output: 0 } });

  expect(figuresIn(span("Today"))).toEqual(UNKNOWN);
  expect(figuresIn(span("7 days"))).toEqual({ fresh: "0", "cache reads": "0", output: "0" });
});

it("draws no provider limit meters: the footer is their one place", () => {
  show();

  expect(screen.queryByRole("img", { name: /window .*% used/ })).toBeNull();
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
