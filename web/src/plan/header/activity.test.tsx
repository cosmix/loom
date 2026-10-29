import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { ActivityPill } from "./activity";
import { useActivities, type Activity } from "./use-activities";

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
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});

it("renders nothing when there are no activities", () => {
  vi.mocked(useActivities).mockReturnValue([]);

  const { container } = render(<ActivityPill compact={false} />);

  expect(container.innerHTML).toBe("");
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

it("formats elapsed minutes and hours and omits unknown start times", async () => {
  render(<ActivityPill compact={false} />);

  fireEvent.click(screen.getByRole("button", { name: "3 in progress: Needs permission" }));

  const list = await screen.findByRole("list", { name: "In progress" });
  const rows = within(list).getAllByRole("listitem");
  expect(within(rows[0]!).getByText("2m")).toBeTruthy();
  expect(within(rows[1]!).getByText("1h 05m")).toBeTruthy();
  expect(rows[2]!.textContent).toBe("Rebuilding summaries");
});

it("updates elapsed time while the popover is open", async () => {
  vi.mocked(useActivities).mockReturnValue([{ ...activities[0]!, since: NOW - 119_000 }]);
  render(<ActivityPill compact={false} />);
  fireEvent.click(screen.getByRole("button", { name: "1 in progress: Needs permission" }));
  const list = await screen.findByRole("list", { name: "In progress" });
  expect(within(list).getByText("1m")).toBeTruthy();

  act(() => vi.advanceTimersByTime(1000));

  expect(within(list).getByText("2m")).toBeTruthy();
});
