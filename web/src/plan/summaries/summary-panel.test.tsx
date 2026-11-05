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
const RUNNING = summaryPanelSchema.parse(panelFixture).running!;
const POST = (body: object) => ({
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(body),
  credentials: "same-origin",
});

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
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("lists the running rebuild's progress and ids", () => {
  show();

  const running = within(screen.getByRole("region", { name: "Running rebuild" }));
  expect(running.getByRole("heading", { name: "Rebuilding 1 of 3" })).toBeTruthy();
  expect(running.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
    "demo/product/section_01",
    "demo/product/section_02",
    "overview/demo/product",
  ]);
});

it("draws the running rebuild's progress as a labelled progress line", () => {
  show();

  const running = within(screen.getByRole("region", { name: "Running rebuild" }));
  const bar = running.getByRole("progressbar", { name: "Rebuild progress" });
  expect(bar.getAttribute("aria-valuemin")).toBe("0");
  expect(bar.getAttribute("aria-valuemax")).toBe("3");
  expect(bar.getAttribute("aria-valuenow")).toBe("1");
  expect(screen.queryByText(/^Idle/)).toBeNull();
});

it("states Idle with the last run's age while no rebuild runs", () => {
  show({ running: null, last: { ...LAST, time: "2026-10-29T08:01:00Z" } });
  expect(screen.getByText("Idle · last run 10 min ago")).toBeTruthy();
  cleanup();

  show({ running: null, last: null });
  expect(screen.getByText("Idle · no runs yet")).toBeTruthy();
});

it("shows a Codex job's log path in mono, and nothing for a Claude session", () => {
  show();
  expect(screen.queryByText(/^Log:/)).toBeNull();
  cleanup();

  show({ running: { ...RUNNING, log_path: "/home/me/.loom/summary-2.log‮" } });
  const path = screen.getByText("/home/me/.loom/summary-2.log⟨U+202E⟩");
  expect(path.closest("code")?.className).toContain("sp-mono");
  expect(
    within(screen.getByRole("region", { name: "Running rebuild" })).getByText(/^Log:/),
  ).toBeTruthy();
});

it("spells out control characters in every server string it shows", () => {
  const bad = (text: string) => `${text}‮`;
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

it("pairs each failed id with its error in a row that offers Retry", () => {
  show({
    failed: [
      { id: "demo/product/section_04", error: "Summary failed to seal" },
      { id: "demo/product/section_05", error: "Session stopped" },
    ],
  });

  const failed = within(screen.getByRole("region", { name: "Failed rebuilds" }));
  const rows = failed.getAllByRole("listitem");
  expect(
    rows.map((row) => [
      row.querySelector(".sp-mono")?.textContent,
      row.querySelector(".sp-row-sub")?.textContent,
    ]),
  ).toEqual([
    ["demo/product/section_04", "Summary failed to seal"],
    ["demo/product/section_05", "Session stopped"],
  ]);
  expect(failed.getAllByRole("button", { name: "Retry" })).toHaveLength(2);
});

it("posts only the failed id whose Retry was pressed", async () => {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockResolvedValue(new Response(JSON.stringify(panelFixture), { status: 200 }));
  vi.stubGlobal("fetch", fetchMock);
  const { store } = show({
    failed: [
      { id: "demo/product/section_04", error: "Summary failed to seal" },
      { id: "demo/product/section_05", error: "Session stopped" },
    ],
  });
  const row = screen.getByText("demo/product/section_05").closest("li");
  if (!row) throw new Error("Missing failed row");

  fireEvent.click(within(row).getByRole("button", { name: "Retry" }));

  await waitFor(() => expect(store.get(panelLoadAtom).status).toBe("ready"));
  expect(fetchMock).toHaveBeenCalledExactlyOnceWith(
    "/api/summaries/rebuild",
    POST({ ids: ["demo/product/section_05"] }),
  );
  expect(screen.queryByRole("alert")).toBeNull();
});

it("posts one rebuild request for a double click on Retry", async () => {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockResolvedValue(new Response(JSON.stringify(panelFixture), { status: 200 }));
  vi.stubGlobal("fetch", fetchMock);
  show();
  const retry = screen.getByRole("button", { name: "Retry" });

  fireEvent.click(retry);
  fireEvent.click(retry);

  await waitFor(() => expect(fetchMock).toHaveBeenCalled());
  await new Promise((resolve) => setTimeout(resolve, 0));
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("shows a retry error in an alert under the row", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>().mockResolvedValue(
      new Response(JSON.stringify({ error: "busy", message: "A rebuild is running" }), {
        status: 409,
      }),
    ),
  );
  show();

  fireEvent.click(screen.getByRole("button", { name: "Retry" }));

  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toBe("A rebuild is running");
  expect(alert.closest("li")?.textContent).toContain("plans/loom-planner-ui");
});

it("shows a failed start as an alert", () => {
  show({ start_error: "Could not start the rebuild session" });

  expect(screen.getByRole("alert").textContent).toBe("Could not start the rebuild session");
});

it("shows the panel's read-only reason without action controls", () => {
  show({ read_only: "tmux is not installed" });

  expect(screen.getByText("tmux is not installed")).toBeTruthy();
  expect(screen.getByText("plans/loom-rebuilds")).toBeTruthy();
  expect(screen.getByText("plans/loom-planner-ui")).toBeTruthy();
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
  expect(fetchMock).toHaveBeenCalledExactlyOnceWith(
    "/api/summaries/release",
    POST({ ids: ["plans/loom-rebuilds"] }),
  );
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
