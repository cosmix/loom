import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { getDefaultStore } from "jotai/vanilla";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import activityCss from "@/plan/header/activity.css?raw";
import fixture from "@/plan/api/fixtures/run-strip.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { runStripSchema } from "@/plan/api/schema-run";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";
import { runStripAtom } from "@/plan/run/use-run";
import { panelLoadAtom, useSummaryFeed, type PanelLoad } from "@/plan/summaries/state";

import { ActivityPill } from "./activity";
import { useActivities, type Activity } from "./use-activities";

vi.mock("@/plan/run/dashboard-button", () => ({
  DashboardButton: () => <a href="/dashboard">Open dashboard</a>,
}));
vi.mock("@/plan/summaries/state", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/plan/summaries/state")>()),
  useSummaryFeed: vi.fn(),
}));
vi.mock("./use-activities", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./use-activities")>()),
  useActivities: vi.fn(),
}));

const NOW = Date.parse("2026-10-29T10:00:00Z");
const HOUR = 3_600_000;
const PRESSED = "plans/pressed-by-hand";

function setPanel(patch: object): void {
  const load: PanelLoad = {
    status: "ready",
    panel: summaryPanelSchema.parse({ ...panelFixture, ...patch }),
  };
  getDefaultStore().set(panelLoadAtom, load);
  vi.mocked(useSummaryFeed).mockReturnValue({ load, reload: vi.fn() });
}

/**
 * An idle panel whose question names every queued id but one pressed by hand. The fixture's
 * terminal claim names a question id too; a stuck claim is a separate fact with its own Release
 * control, so it is cleared here to count only the question and queue listings.
 */
function setIdleQuestion(): void {
  setPanel({
    running: null,
    failed: [],
    terminal_claims: [],
    queue: [...questionFixture.ids, PRESSED],
    question: questionFixture,
  });
}

async function openIdle(): Promise<HTMLElement> {
  vi.mocked(useActivities).mockReturnValue([]);
  render(
    <TooltipProvider>
      <ActivityPill compact={false} />
    </TooltipProvider>,
  );
  // A pending question opens the popover on its own; otherwise open it from the pill.
  const opened = screen.queryByRole("dialog", { name: "In progress" });
  if (opened) return opened;
  fireEvent.click(screen.getByRole("button", { name: "Activity: idle" }));
  return screen.findByRole("dialog", { name: "In progress" });
}

function expand(dialog: HTMLElement): HTMLElement {
  const row = within(dialog).getByRole("button", { name: /^Summary rebuilds:/ });
  if (row.getAttribute("aria-expanded") !== "true") fireEvent.click(row);
  return row;
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
  vi.setSystemTime(NOW);
  vi.mocked(useSummaryFeed).mockReturnValue({ load: { status: "loading" }, reload: vi.fn() });
});

afterEach(() => {
  cleanup();
  getDefaultStore().set(runStripAtom, null);
  getDefaultStore().set(panelLoadAtom, { status: "loading" });
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("the summary detail", () => {
  it("lists each out-of-date id once, keeping a queued id the question does not name", async () => {
    setIdleQuestion();
    const dialog = await openIdle();
    expand(dialog);

    for (const id of questionFixture.ids) expect(within(dialog).getAllByText(id)).toHaveLength(1);
    const queue = within(within(dialog).getByRole("region", { name: "Queue" }));
    expect(queue.getAllByRole("listitem").map((item) => item.textContent)).toEqual([PRESSED]);
  });

  it("drops the Queue section when the question names every queued id", async () => {
    setPanel({ running: null, failed: [], queue: questionFixture.ids, question: questionFixture });
    const dialog = await openIdle();
    expand(dialog);

    expect(within(dialog).queryByRole("region", { name: "Queue" })).toBeNull();
    expect(within(dialog).getByRole("list", { name: "Out-of-date summaries" })).toBeTruthy();
  });

  it("keeps the idle row's time column free of the out-of-date count", async () => {
    setPanel({ running: null, failed: [], queue: questionFixture.ids, question: null });
    const dialog = await openIdle();

    const row = within(dialog).getByRole("button", { name: "Summary rebuilds: 6 out of date" });
    expect(row.querySelector(".activity-row__sub")?.textContent).toBe("6 out of date");
    expect(row.querySelector(".activity-row__time")?.textContent).toBe("");
  });
});

describe("summary rows, label in name (WCAG 2.5.3)", () => {
  const title = (row: HTMLElement) => row.querySelector(".activity-row__title")?.textContent ?? "";

  it("names the idle row with its visible title", async () => {
    setPanel({ running: null, failed: [], queue: questionFixture.ids, question: null });
    const dialog = await openIdle();

    const row = within(dialog).getByRole("button", { name: /^Summary rebuilds:/ });
    expect(title(row)).toBe("Summary rebuilds");
    expect(row.getAttribute("aria-label")).toContain(title(row));
  });

  it("a running rebuild's row names its visible title", async () => {
    vi.mocked(useActivities).mockReturnValue([
      {
        id: "summaries",
        kind: "summaries",
        title: "3 summaries rebuilding",
        subject: "1 of 3",
        since: NOW,
      },
    ]);
    setPanel({});
    render(
      <TooltipProvider>
        <ActivityPill compact={false} />
      </TooltipProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: /^1 in progress:/ }));
    const dialog = await screen.findByRole("dialog", { name: "In progress" });

    const row = within(dialog).getByRole("button", { name: /^Summary rebuilds:/ });
    expect(row.getAttribute("aria-label")).toBe("Summary rebuilds: 3 summaries rebuilding, 1 of 3");
    expect(row.getAttribute("aria-label")).toContain(title(row));
  });
});

describe("the popover's scrolling region", () => {
  it("scrolls the list and the open detail while the footer stays outside the scroller", async () => {
    getDefaultStore().set(runStripAtom, {
      ...runStripSchema.parse(fixture),
      plan: "doc/plans/PLAN-x.md",
      daemon_running: false,
    });
    setIdleQuestion();
    const dialog = await openIdle();
    expand(dialog);

    expect(dialog.classList.contains("overflow-y-auto")).toBe(false);
    const scroll = dialog.querySelector<HTMLElement>(".activity-scroll")!;
    expect(scroll.textContent).toBe("Nothing in progress.");
    const foot = dialog.querySelector<HTMLElement>(".activity-foot")!;
    expect(scroll.contains(foot)).toBe(false);
    expect(foot.parentElement).toBe(dialog);

    const detail = foot.querySelector<HTMLElement>(".activity-foot__scroll")!;
    expect(within(detail).getByText(PRESSED)).toBeTruthy();
    expect(detail.textContent).not.toContain("PLAN-x.md");
    expect(within(foot).getByText("PLAN-x.md")).toBeTruthy();
    expect(foot.lastElementChild?.contains(detail)).toBe(false);
  });

  it("gives the scrolling region, not the popover, the overflow", () => {
    const rule = (selector: string) =>
      new RegExp(`${selector.replace(/[.>]/g, "\\$&")}\\s*\\{[^}]*overflow-y:\\s*auto`);
    expect(activityCss).toMatch(rule(".activity-scroll"));
    expect(activityCss).toMatch(rule(".activity-foot > .activity-foot__scroll"));
  });
});

describe("elapsed time", () => {
  const at = (since: number): Activity[] => [
    { id: "pressure", kind: "pressure", title: "Reviewing the plan", subject: null, since },
  ];

  it.each([
    [24 * HOUR - 60_000, "1439m"],
    [24 * HOUR, "1d"],
    [5 * 24 * HOUR + 10 * HOUR, "5d"],
  ])("reads %i ms as %s", async (elapsed, label) => {
    vi.mocked(useActivities).mockReturnValue(at(NOW - elapsed));
    render(<ActivityPill compact={false} />);
    fireEvent.click(screen.getByRole("button", { name: "1 in progress: Pressure test" }));

    const list = await screen.findByRole("list", { name: "In progress" });
    expect(within(list).getByText(label)).toBeTruthy();
  });
});
