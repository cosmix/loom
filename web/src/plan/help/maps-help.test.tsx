import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { COVERAGE_SHORTCUTS } from "./shortcuts";
import {
  COVERAGE_TIPS,
  CoverageHelp,
  CoverageSpecimen,
  FINDINGS_TIPS,
  FindingsHelp,
} from "./maps-help";
import { FindingSpecimen } from "./finding-specimen";

beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Help must not request live data")));
  vi.stubGlobal("WebSocket", vi.fn());
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("the coverage specimen", () => {
  it("draws the tiles, a section and its rows in the page's own markup, inert", () => {
    const { container } = render(<CoverageSpecimen />);

    // The specimen's root is the inert planner root: the container the page's width steps query.
    const page = container.querySelector("[data-loom-plan][inert].coverage");
    expect(page).not.toBeNull();
    expect(
      [...page!.querySelectorAll(".coverage-tile")].map(
        (tile) => tile.querySelector(".coverage-tile__label")?.textContent,
      ),
    ).toEqual(["All headings", "Not covered", "Changed since covered", "With open findings"]);
    expect(page!.querySelectorAll('.coverage-tile[aria-pressed="true"]')).toHaveLength(1);
    expect(page!.querySelector(".coverage-section__title")?.textContent).toBe("Trip planning");
    expect(page!.querySelector(".coverage-section__count")?.textContent).toBe("2 of 3 covered");
  });

  it("shows each row state in words and one row open to Why, How and Findings", () => {
    const { container } = render(<CoverageSpecimen />);

    const states = [...container.querySelectorAll(".coverage-row")].map((row) =>
      row.getAttribute("data-state"),
    );
    expect(states).toEqual(["built", "changed", "uncovered"]);
    const open = container.querySelectorAll('.coverage-row__toggle[aria-expanded="true"]');
    expect(open).toHaveLength(1);
    const detail = container.querySelector(".coverage-detail");
    expect([...detail!.querySelectorAll("h3")].map((title) => title.textContent)).toEqual([
      "Why",
      "How",
      "Findings",
    ]);
    expect(container.querySelector(".coverage-nothing")?.textContent).toBe("Nothing yet");
  });
});

describe("the coverage topic body", () => {
  it("names the tiles, the row parts, the states and the keys without live data", async () => {
    render(<CoverageHelp />);
    await Promise.resolve();

    for (const title of [
      "The tiles",
      "A heading row",
      "States",
      "An open row",
      COVERAGE_SHORTCUTS.title,
    ]) {
      expect(screen.getByRole("heading", { name: title })).toBeTruthy();
    }
    for (const name of ["All headings", "Covered by", "Built", "Planned"]) {
      expect(screen.getAllByText(name).length).toBeGreaterThan(0);
    }
    expect(screen.getAllByText("Changed since covered").length).toBeGreaterThan(0);
    const keys = screen
      .getByRole("heading", { name: COVERAGE_SHORTCUTS.title })
      .closest("section")!;
    for (const [, action] of COVERAGE_SHORTCUTS.rows) {
      expect(within(keys).getByText(action)).toBeTruthy();
    }
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  });
});

describe("the finding specimen", () => {
  it("draws the board's four lanes, one finding in each, inert", () => {
    const { container } = render(<FindingSpecimen />);

    const root = container.querySelector("[data-loom-plan][inert]");
    expect(root).not.toBeNull();
    const lanes = [...root!.querySelectorAll(".findings-board .findings-lane")];
    expect(lanes.map((lane) => lane.getAttribute("data-lane"))).toEqual([
      "verify",
      "decide",
      "addressed",
      "aside",
    ]);
    expect(lanes.map((lane) => lane.querySelectorAll(".findings-card").length)).toEqual([
      1, 1, 1, 1,
    ]);
    expect(container.querySelector(".findings-lane__none")).toBeNull();
  });

  it("draws the open finding as the page's card: id, title, severity, place and source", () => {
    const { container } = render(<FindingSpecimen />);

    const card = container.querySelector('.findings-lane[data-lane="decide"] .findings-card');
    expect(card!.getAttribute("data-state")).toBe("open");
    expect(card!.querySelector(".findings-id")?.textContent).toBe("F-3");
    expect(card!.querySelector(".findings-card__title")?.textContent).toBe(
      "Stops have no time window",
    );
    expect(card!.querySelector(".findings-severity")?.textContent).toMatch(/high/i);
    expect(card!.querySelector(".findings-card__where")?.textContent).toBe("Stops");
  });

  it("shows an addressed card with what changed and a dismissed one with its reason", () => {
    const { container } = render(<FindingSpecimen />);

    const addressed = container.querySelector('.findings-card[data-state="addressed"]');
    expect(addressed!.querySelector(".findings-card__reason")?.textContent).toMatch(
      /Changed 1 place/,
    );
    const dismissed = container.querySelector('.findings-card[data-state="dismissed"]');
    expect(dismissed!.querySelector(".findings-card__detail")?.textContent).toBe("code-level note");
    expect(container.querySelector('.findings-card[data-state="unverified"]')).not.toBeNull();
  });

  it("reads and requests nothing", () => {
    render(<FindingSpecimen />);

    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  });
});

describe("the findings topic body", () => {
  it("names the two sessions, Dismiss and every state without live data", async () => {
    render(<FindingsHelp />);
    await Promise.resolve();

    expect(screen.getByRole("heading", { name: "Groups" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Sessions and actions" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "State" })).toBeTruthy();
    for (const name of ["Address the findings", "Review the feature", "Dismiss"]) {
      expect(screen.getAllByText(name).length).toBeGreaterThan(0);
    }
    for (const name of [
      "Open",
      "Being addressed",
      "Unverified",
      "Stale",
      "Duplicate",
      "Addressed",
      "Could not address",
      "Dismissed",
    ]) {
      expect(screen.getAllByText(name).length).toBeGreaterThan(0);
    }
    expect(screen.queryByText("Plan this")).toBeNull();
    expect(screen.queryByText("Fold into entry")).toBeNull();
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  });
});

it.each([
  ["coverage", COVERAGE_TIPS],
  ["findings", FINDINGS_TIPS],
])("provides one to three single sentence %s tips", (_page, tips) => {
  expect(tips.length).toBeGreaterThanOrEqual(1);
  expect(tips.length).toBeLessThanOrEqual(3);
  for (const tip of tips) {
    expect(tip.trim()).toMatch(/^[^.!?]+[.!?]$/u);
  }
});
