import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { PlannerShortcuts, PLANNER_SHORTCUT_GROUPS } from "./shortcuts";

afterEach(cleanup);

describe("PlannerShortcuts", () => {
  it("renders the three shortcut groups in order", () => {
    render(<PlannerShortcuts />);

    expect(screen.getAllByRole("heading").map((heading) => heading.textContent)).toEqual([
      "While reading",
      "On the coverage map",
      "Anywhere",
    ]);
  });

  it.each([
    { title: "Anywhere", action: "Save in the editor", keys: ["Ctrl", "S", "⌘", "S"] },
    {
      title: "On the coverage map",
      action: "Focus the first card that follows from the focused one",
      keys: ["l"],
    },
    {
      title: "On the coverage map",
      action: "Open the focused card's document or passage",
      keys: ["Enter"],
    },
    { title: "Anywhere", action: "Allow the pinned permission request", keys: ["y"] },
    { title: "Anywhere", action: "Focus the chat composer", keys: ["c"] },
    {
      title: "While reading",
      action: "Attach the focused sentence or passage to the next chat message while authoring",
      keys: ["a"],
    },
  ])("renders the $title shortcut for $action", ({ title, action, keys }) => {
    render(<PlannerShortcuts />);

    const section = screen.getByRole("heading", { name: title }).closest("section")!;
    const row = within(section).getByText(action).closest("div")!;
    expect([...row.querySelectorAll("kbd")].map((cap) => cap.textContent)).toEqual(keys);
  });

  it("renders every row's individual keys inside kbd elements", () => {
    render(<PlannerShortcuts />);

    for (const group of PLANNER_SHORTCUT_GROUPS) {
      const section = screen.getByRole("heading", { name: group.title }).closest("section")!;
      for (const [keys, action] of group.rows) {
        const row = within(section).getByText(action).closest("div")!;
        const caps = [...row.querySelectorAll("kbd")].map((cap) => cap.textContent);
        expect(caps).toEqual(
          keys
            .split(" / ")
            .flatMap((alternative) => alternative.split(" ").filter((key) => key !== "+")),
        );
      }
    }
  });

  it("describes j and k as stopping on openable sentences and fold caps", () => {
    render(<PlannerShortcuts />);

    expect(screen.getByText("Focus the next openable sentence or fold cap")).toBeTruthy();
    expect(screen.getByText("Focus the previous openable sentence or fold cap")).toBeTruthy();
    expect(screen.queryByText(/passage, including opened runs/)).toBeNull();
  });

  it("includes the question-mark shortcut for opening or closing Help", () => {
    render(<PlannerShortcuts />);

    const row = screen.getByText("Open or close Help outside text fields").closest("div")!;
    expect(row.querySelector("kbd")?.textContent).toBe("?");
  });
});
