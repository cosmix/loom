import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { getDefaultStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
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
const activities: Activity[] = [
  {
    id: "permission",
    kind: "permission",
    title: "Approve document edit",
    subject: "Product requirements",
    since: NOW - 125_000,
  },
  {
    id: "pressure",
    kind: "pressure",
    title: "Reviewing the plan",
    subject: "Planner redesign",
    since: NOW - 3_900_000,
  },
  {
    id: "summaries",
    kind: "summaries",
    title: "Rebuilding summaries",
    subject: null,
    since: null,
  },
];

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
  vi.setSystemTime(NOW);
  vi.mocked(useActivities).mockReturnValue(activities);
  vi.mocked(useSummaryFeed).mockReturnValue({ load: { status: "loading" }, reload: vi.fn() });
});

afterEach(() => {
  cleanup();
  getDefaultStore().set(runStripAtom, null);
  getDefaultStore().set(panelLoadAtom, { status: "loading" });
  vi.useRealTimers();
  vi.clearAllMocks();
});

it("stays mounted as an idle pill with a still check when there are no activities", async () => {
  vi.mocked(useActivities).mockReturnValue([]);

  render(<ActivityPill compact={false} />);

  const trigger = screen.getByRole("button", { name: "Activity: idle" });
  expect(trigger.textContent).toBe("Idle");
  expect(trigger.querySelector("[data-kind]")).toBeNull();
  expect(trigger.querySelector(".activity-pill__idle")).toBeTruthy();

  fireEvent.click(trigger);

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(within(dialog).queryByRole("list", { name: "In progress" })).toBeNull();
  expect(within(dialog).getByText("Nothing in progress.")).toBeTruthy();
  expect(within(dialog).getByRole("button", { name: /^Summary rebuilds:/ })).toBeTruthy();
});

it("shows only the check in compact mode when idle", () => {
  vi.mocked(useActivities).mockReturnValue([]);

  render(<ActivityPill compact />);

  expect(screen.getByRole("button", { name: "Activity: idle" }).textContent).toBe("");
});

it("swaps the chevron while open", () => {
  render(<ActivityPill compact={false} />);
  const trigger = screen.getByRole("button", { name: "3 in progress: Needs permission" });
  expect(trigger.querySelector(".lucide-chevron-down")).toBeTruthy();

  fireEvent.click(trigger);

  expect(trigger.querySelector(".lucide-chevron-up")).toBeTruthy();
  expect(trigger.querySelector(".lucide-chevron-down")).toBeNull();
});

it("shows the number of activities in progress", () => {
  render(<ActivityPill compact={false} />);

  expect(screen.getByRole("button", { name: "3 in progress: Needs permission" }).textContent).toBe(
    "3 in progress",
  );
});

it("shows only the count in compact mode with the full accessible label", () => {
  render(<ActivityPill compact />);

  const trigger = screen.getByRole("button", { name: "3 in progress: Needs permission" });
  expect(trigger.textContent).toBe("3");
  expect(trigger.querySelector('[data-kind="permission"]')).toBeTruthy();
});

it("shows the most urgent activity's glyph on the trigger", () => {
  render(<ActivityPill compact={false} />);

  const trigger = screen.getByRole("button", { name: "3 in progress: Needs permission" });
  expect(trigger.querySelector("[data-kind]")?.getAttribute("data-kind")).toBe("permission");
});

it("opens a named list containing every activity's title, subject and glyph", async () => {
  render(<ActivityPill compact={false} />);

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  const list = within(dialog).getByRole("list", { name: "In progress" });
  const rows = within(list).getAllByRole("listitem");
  expect(rows).toHaveLength(3);
  expect(within(rows[0]!).getByText("Approve document edit")).toBeTruthy();
  expect(within(rows[0]!).getByText("Product requirements")).toBeTruthy();
  expect(rows[0]!.querySelector('[data-kind="permission"]')).toBeTruthy();
  expect(within(rows[1]!).getByText("Reviewing the plan")).toBeTruthy();
  expect(within(rows[1]!).getByText("Planner redesign")).toBeTruthy();
  expect(rows[1]!.querySelector('[data-kind="pressure"]')).toBeTruthy();
  expect(within(rows[2]!).getByText("Rebuilding summaries")).toBeTruthy();
  expect(rows[2]!.querySelector('[data-kind="summaries"]')).toBeTruthy();
});

it("formats elapsed time as m:ss under an hour and whole minutes above it and omits unknown start times", async () => {
  render(<ActivityPill compact={false} />);

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const list = await screen.findByRole("list", { name: "In progress" });
  const rows = within(list).getAllByRole("listitem");
  expect(within(rows[0]!).getByText("2:05")).toBeTruthy();
  expect(within(rows[1]!).getByText("65m")).toBeTruthy();
  expect(rows[2]!.textContent).toBe("Rebuilding summaries");
});

it("updates elapsed time while the popover is open", async () => {
  vi.mocked(useActivities).mockReturnValue([{ ...activities[0]!, since: NOW - 119_000 }]);
  render(<ActivityPill compact={false} />);
  fireEvent.click(screen.getByRole("button", { name: "1 in progress: Needs permission" }));
  const list = await screen.findByRole("list", { name: "In progress" });
  expect(within(list).getByText("1:59")).toBeTruthy();

  act(() => vi.advanceTimersByTime(1000));

  expect(within(list).getByText("2:00")).toBeTruthy();
});

function setRun(patch: object) {
  getDefaultStore().set(runStripAtom, { ...runStripSchema.parse(fixture), ...patch });
}

/** The named tests below drive the pill from the atoms, through the real hook. */
async function useRealActivities() {
  const actual = await vi.importActual<typeof import("./use-activities")>("./use-activities");
  vi.mocked(useActivities).mockImplementation(actual.useActivities);
}

it("keeps a stopped run out of the closed pill and shows it as one footer control", async () => {
  setRun({ plan: "doc/plans/DONE-PLAN-x.md", daemon_running: false, merged: 0, total: 10 });
  const { container } = render(<ActivityPill compact={false} />);
  expect(container.textContent).not.toContain("daemon stopped");
  expect(document.body.textContent).not.toContain("DONE-PLAN-x.md");

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  const foot = dialog.querySelector<HTMLElement>(".activity-foot")!;
  expect(within(foot).getByText("DONE-PLAN-x.md")).toBeTruthy();
  expect(within(foot).getByText("0 of 10 stages merged")).toBeTruthy();
  expect(within(foot).getByRole("link", { name: "Open dashboard" })).toBeTruthy();
  expect(foot.querySelector('[data-kind="running"]')).toBeNull();
  expect(dialog.textContent).not.toContain("daemon stopped");
  expect(within(dialog).queryByText("loom run")).toBeNull();
  expect(within(dialog).queryByRole("progressbar")).toBeNull();
  expect(within(dialog).getByRole("button", { name: /^Summary rebuilds:/ })).toBeTruthy();
});

it("a stopped run keeps one dashboard line and no activity", async () => {
  await useRealActivities();
  setRun({ plan: "doc/plans/PLAN-x.md", daemon_running: true, merged: 2, total: 6, attention: 0 });
  render(<ActivityPill compact={false} />);
  fireEvent.click(screen.getByRole("button", { name: "1 in progress: Plan running" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  const list = within(dialog).getByRole("list", { name: "In progress" });
  expect(within(dialog).getAllByText(/PLAN-x\.md/)).toHaveLength(1);
  expect(within(list).getByText("PLAN-x.md · 2 of 6 stages merged")).toBeTruthy();
  expect(within(dialog).getAllByRole("link", { name: "Open dashboard" })).toHaveLength(1);
  expect(dialog.querySelector(".activity-foot")?.textContent).not.toContain("PLAN-x.md");

  act(() =>
    setRun({
      plan: "doc/plans/PLAN-x.md",
      daemon_running: false,
      merged: 2,
      total: 6,
      attention: 2,
    }),
  );

  expect(screen.getByRole("button", { name: "Activity: idle" })).toBeTruthy();
  const foot = dialog.querySelector<HTMLElement>(".activity-foot")!;
  expect(within(dialog).getAllByText(/PLAN-x\.md/)).toHaveLength(1);
  expect(within(foot).getByText("PLAN-x.md")).toBeTruthy();
  expect(within(foot).getByText("2 need attention")).toBeTruthy();
  expect(within(dialog).getAllByRole("link", { name: "Open dashboard" })).toHaveLength(1);
  expect(within(foot).getByRole("link", { name: "Open dashboard" })).toBeTruthy();
  const remaining = within(dialog).queryByRole("list", { name: "In progress" });
  expect(remaining ? within(remaining).queryAllByRole("listitem") : []).toHaveLength(0);
  expect(dialog.querySelector('[data-kind="running"]')).toBeNull();
  expect(dialog.textContent).not.toContain("daemon stopped");
});

it.each([true, false])(
  "escapes a bidi control in the plan name for a run (daemon running: %s)",
  async (running) => {
    await useRealActivities();
    setRun({ plan: "doc/plans/PLAN-‮exe.md", daemon_running: running, attention: 2 });
    render(<ActivityPill compact={false} />);
    fireEvent.click(screen.getByRole("button", { name: /^(1 in progress|Activity)/ }));

    const dialog = await screen.findByRole("dialog", { name: "In progress" });
    expect(within(dialog).getAllByText(/PLAN-⟨U\+202E⟩exe\.md/).length).toBeGreaterThan(0);
    expect(document.body.textContent).not.toContain("‮");
  },
);

it("renders no run row or footer line without a plan", async () => {
  await useRealActivities();
  setRun({ plan: null, daemon_running: false });
  render(<ActivityPill compact={false} />);
  fireEvent.click(screen.getByRole("button", { name: /^Activity/ }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(within(dialog).queryByRole("link", { name: "Open dashboard" })).toBeNull();
  expect(dialog.querySelector(".activity-foot")?.textContent ?? "").not.toMatch(/stages merged/);
});

function setPanel(patch: object): PanelLoad {
  const load: PanelLoad = {
    status: "ready",
    panel: summaryPanelSchema.parse({ ...panelFixture, ...patch }),
  };
  getDefaultStore().set(panelLoadAtom, load);
  vi.mocked(useSummaryFeed).mockReturnValue({ load, reload: vi.fn() });
  return load;
}

it("a running rebuild's row opens its queue and errors", async () => {
  await useRealActivities();
  const start = { queue: ["demo/product/section_09"], start_error: "Could not start a rebuild" };
  setPanel(start);
  const pill = (
    <TooltipProvider>
      <ActivityPill compact={false} />
    </TooltipProvider>
  );
  const { rerender } = render(pill);
  fireEvent.click(screen.getByRole("button", { name: "1 in progress: Rebuilding summaries" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  const rows = within(dialog).getAllByRole("button", { name: /^Summary rebuilds:/ });
  expect(rows).toHaveLength(1);
  const row = rows[0]!;
  expect(row.getAttribute("aria-label")).toBe("Summary rebuilds: Rebuilding 1 of 3");
  expect(row.closest("ul")?.getAttribute("aria-label")).toBe("In progress");
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(within(dialog).queryByText("demo/product/section_09")).toBeNull();

  fireEvent.click(row);

  expect(row.getAttribute("aria-expanded")).toBe("true");
  expect(within(dialog).getByText("demo/product/section_09")).toBeTruthy();
  expect(within(dialog).getByText("Could not start a rebuild")).toBeTruthy();

  act(() => void setPanel({ ...start, question: questionFixture }));
  rerender(pill);

  expect(within(dialog).getAllByRole("button", { name: /^Summary rebuilds:/ })).toHaveLength(1);
  expect(within(dialog).getByText(/summaries out of date\.\s+Rebuild all\?/)).toBeTruthy();
});

it("an expanded running row stays expanded as the footer row once the rebuild ends", async () => {
  await useRealActivities();
  const start = { queue: ["demo/product/section_09"], start_error: "Could not start a rebuild" };
  setPanel(start);
  const pill = (
    <TooltipProvider>
      <ActivityPill compact={false} />
    </TooltipProvider>
  );
  const { rerender } = render(pill);
  fireEvent.click(screen.getByRole("button", { name: "1 in progress: Rebuilding summaries" }));
  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  const running = within(dialog).getByRole("button", { name: /^Summary rebuilds:/ });
  fireEvent.click(running);
  expect(running.getAttribute("aria-expanded")).toBe("true");
  expect(within(dialog).getByText("demo/product/section_09")).toBeTruthy();

  act(() => void setPanel({ ...start, running: null }));
  rerender(pill);

  expect(within(dialog).queryByRole("list", { name: "In progress" })).toBeNull();
  const foot = dialog.querySelector<HTMLElement>(".activity-foot")!;
  const row = within(foot).getByRole("button", { name: /^Summary rebuilds:/ });
  expect(row).not.toBe(running);
  expect(row.getAttribute("aria-expanded")).toBe("true");
  expect(within(dialog).getByText("demo/product/section_09")).toBeTruthy();
});

it("omits the footer and the summary feed for a sample pill", async () => {
  setRun({});
  render(<ActivityPill compact={false} sample />);

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(within(dialog).getAllByRole("listitem")).toHaveLength(3);
  expect(within(dialog).queryByText("loom run")).toBeNull();
  expect(within(dialog).queryByRole("button", { name: /^Summary rebuilds:/ })).toBeNull();
  expect(useSummaryFeed).not.toHaveBeenCalled();
});

it("sets the popover width and surface", async () => {
  render(<ActivityPill compact={false} />);

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(dialog.classList.contains("w-[min(380px,92vw)]")).toBe(true);
  expect(dialog.classList.contains("bg-card")).toBe(true);
  expect(dialog.style.boxShadow).toContain("var(--plan-shadow)");
});
