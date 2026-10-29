import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { PlannerShortcuts, PLANNER_SHORTCUT_GROUPS } from "./shortcuts";

afterEach(cleanup);

describe("PlannerShortcuts", () => {
  it("renders all four shortcut groups in order", () => {
    render(<PlannerShortcuts />);

    expect(screen.getAllByRole("heading").map((heading) => heading.textContent)).toEqual([
      "While reading",
      "Editor",
      "Chat",
      "Anywhere",
    ]);
  });

  it.each([
    { title: "Editor", action: "Save", keys: ["Ctrl", "S", "⌘", "S"] },
    { title: "Chat", action: "Allow the pinned permission request", keys: ["y"] },
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

  it("includes the question-mark shortcut for opening or closing Help", () => {
    render(<PlannerShortcuts />);

    const row = screen.getByText("Open or close Help outside text fields").closest("div")!;
    expect(row.querySelector("kbd")?.textContent).toBe("?");
  });
});
