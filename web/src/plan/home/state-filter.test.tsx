import { fireEvent, screen, within } from "@testing-library/react";
import { act } from "react";
import { MemoryRouter } from "react-router";
import { expect, it } from "vitest";

import type { StandalonePlanView } from "@/plan/api/schema";
import { harness, installStartChatLifecycle } from "@/plan/chat/start-chat-test-kit";
import { project, standalonePlans } from "@/plan/home/features-contracts-kit";
import { StandaloneShelf } from "@/plan/home/standalone-shelf";
import { projectAtom } from "@/plan/state/atoms";

installStartChatLifecycle();

function plan(slug: string, state: StandalonePlanView["state"]): StandalonePlanView {
  const title = slug[0]!.toUpperCase() + slug.slice(1);
  return { slug, title, state, path: `doc/plans/PLAN-${slug}.md`, last_change: null };
}

const mixed = [plan("cache", "running"), plan("search", "written"), plan("lint", "written")];
const mixedDone = [...mixed, plan("cleanup", "done")];

function mount(plans: StandalonePlanView[]) {
  const h = harness();
  h.store.set(projectAtom, project({ standalone: plans }));
  h.mount(
    <MemoryRouter>
      <StandaloneShelf defaultOpen />
    </MemoryRouter>,
  );
  return h;
}

const group = () => screen.getByRole("group", { name: "Filter by state" });
const toggle = (name: string) => within(group()).getByRole("button", { name });
const titles = () =>
  within(screen.getByRole("table"))
    .queryAllByRole("link")
    .map((link) => link.textContent);

it("offers All and one counted toggle per state the list holds, All pressed", () => {
  mount(mixedDone);

  const buttons = within(group()).getAllByRole("button");
  expect(buttons.map((button) => button.textContent)).toEqual([
    "All 4",
    "Running 1",
    "Written 2",
    "Done 1",
  ]);
  expect(buttons.map((button) => button.getAttribute("aria-pressed"))).toEqual([
    "true",
    "false",
    "false",
    "false",
  ]);
});

it("shows only the pressed states, several at once, and returns to All when none is", () => {
  mount(mixedDone);

  fireEvent.click(toggle("Written 2"));
  expect(titles().sort()).toEqual(["Lint", "Search"]);
  expect(toggle("All 4").getAttribute("aria-pressed")).toBe("false");

  fireEvent.click(toggle("Done 1"));
  expect(titles().sort()).toEqual(["Cleanup", "Lint", "Search"]);

  fireEvent.click(toggle("Written 2"));
  fireEvent.click(toggle("Done 1"));
  expect(titles()).toHaveLength(4);
  expect(toggle("All 4").getAttribute("aria-pressed")).toBe("true");
});

it("All clears every pressed state", () => {
  mount(mixedDone);
  fireEvent.click(toggle("Running 1"));
  fireEvent.click(toggle("All 4"));

  expect(titles()).toHaveLength(4);
  expect(toggle("Running 1").getAttribute("aria-pressed")).toBe("false");
});

it("composes with the text filter and offers the way back when nothing matches", () => {
  mount([...standalonePlans(10), plan("zeta", "running")]);

  fireEvent.change(screen.getByRole("searchbox", { name: "Filter plans" }), {
    target: { value: "plan 3" },
  });
  expect(titles()).toEqual(["Plan 3"]);
  fireEvent.click(toggle("Running 1"));

  expect(screen.getByText("No plans in this state.")).toBeTruthy();
  expect(screen.queryByRole("table")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Show all states" }));
  expect(titles()).toEqual(["Plan 3"]);
});

it("unselects a selected plan the state filter hides", () => {
  mount(mixed);
  fireEvent.click(screen.getByRole("checkbox", { name: "Select Cache" }));
  fireEvent.click(screen.getByRole("checkbox", { name: "Select Search" }));
  expect(screen.getByRole("group", { name: "Selected plans" }).textContent).toContain("2 selected");

  fireEvent.click(toggle("Written 2"));

  expect(screen.getByRole("group", { name: "Selected plans" }).textContent).toContain("1 selected");
  fireEvent.click(toggle("All 3"));
  expect((screen.getByRole("checkbox", { name: "Select Cache" }) as HTMLInputElement).checked).toBe(
    false,
  );
});

it("keeps its counts live and draws nothing with fewer than two states", () => {
  const h = mount(mixed);
  fireEvent.click(toggle("Running 1"));

  act(() => h.store.set(projectAtom, project({ standalone: [...mixed, plan("new", "running")] })));
  expect(toggle("Running 2").getAttribute("aria-pressed")).toBe("true");
  expect(titles().sort()).toEqual(["Cache", "New"]);

  act(() => h.store.set(projectAtom, project({ standalone: [plan("search", "written")] })));
  expect(screen.queryByRole("group", { name: "Filter by state" })).toBeNull();
  expect(titles()).toEqual(["Search"]);
});

it("marks the running state word with the turning two-arrow glyph, never the live dot", () => {
  mount(mixed);

  const row = screen.getByRole("link", { name: "Cache" }).closest("tr")!;
  const marks = [
    row.querySelector(".shelf-table__state"),
    toggle("Running 1"),
    document.querySelector(".shelf__states [data-state='running']"),
  ];
  for (const mark of marks) {
    expect(mark?.querySelector("svg.lucide-refresh-cw.plan-arrow-spinner")).not.toBeNull();
    expect(mark?.querySelector(".activity-glyph")).toBeNull();
  }
});
