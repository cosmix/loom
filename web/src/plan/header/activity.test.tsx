import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { getDefaultStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import fixture from "@/plan/api/fixtures/run-strip.json";
import { runStripSchema } from "@/plan/api/schema-run";
import { runStripAtom } from "@/plan/run/use-run";
import { useSummaryFeed } from "@/plan/summaries/state";

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

it("keeps the loom run row out of the closed pill and shows it in the popover footer", async () => {
  setRun({ plan: "doc/plans/DONE-PLAN-x.md", daemon_running: false, merged: 0, total: 10 });
  const { container } = render(<ActivityPill compact={false} />);
  expect(container.textContent).not.toContain("daemon stopped");
  expect(document.body.textContent).not.toContain("DONE-PLAN-x.md");

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(within(dialog).getByText("loom run")).toBeTruthy();
  expect(dialog.textContent).toContain("DONE-PLAN-x.md · daemon stopped · 0/10 merged");
  expect(within(dialog).getByRole("link", { name: "Open dashboard" })).toBeTruthy();
  expect(within(dialog).getByRole("button", { name: /^Summary rebuilds:/ })).toBeTruthy();
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

it("sets the popover width, offset and surface from the brief", async () => {
  render(<ActivityPill compact={false} />);

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(dialog.classList.contains("w-[min(360px,86vw)]")).toBe(true);
  expect(dialog.classList.contains("bg-card")).toBe(true);
  expect(dialog.style.boxShadow).toContain("var(--dialog-shadow)");
});
