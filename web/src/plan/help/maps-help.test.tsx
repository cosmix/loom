import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { COVERAGE_SHORTCUTS } from "./shortcuts";
import {
  COVERAGE_TIPS,
  CoverageHelp,
  CoverageSpecimen,
  FINDINGS_TIPS,
  FindingsHelp,
  FindingSpecimen,
} from "./maps-help";

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
  it("draws an open finding with its id, status, text, About chips and three secondary actions", () => {
    const { container } = render(<FindingSpecimen />);

    const card = container.querySelector(
      '[data-loom-plan][inert] .maps-finding[data-status="open"]',
    );
    expect(card).not.toBeNull();
    expect(card!.querySelector(".maps-finding-id")?.textContent).toBe("F-3");
    expect(card!.querySelector("h3.maps-finding-title")?.textContent).toBe(
      "Stops have no time window",
    );
    expect(card!.querySelector(".maps-finding-status")?.textContent).toBe("Open");
    expect(card!.querySelectorAll(".maps-about .maps-chip")).toHaveLength(2);
    const actions = [...card!.querySelectorAll(".maps-actions button")];
    expect(actions.map((button) => button.textContent)).toEqual([
      "Plan this",
      "Fold into entry",
      "Dismiss",
    ]);
    expect(actions.every((button) => button.getAttribute("data-look") === "secondary")).toBe(true);
  });

  it("folds a decided finding to its heading and the outcome in words", () => {
    const { container } = render(<FindingSpecimen />);

    const decided = container.querySelector('.maps-finding[data-status="dismissed"]');
    expect(decided!.querySelector(".maps-finding-status")?.textContent).toBe(
      "Dismissed: not needed",
    );
    expect(decided!.querySelector("button")).toBeNull();
  });
});

describe("the findings topic body", () => {
  it("names the three decisions, Review the feature and the four statuses without live data", async () => {
    render(<FindingsHelp />);
    await Promise.resolve();

    expect(screen.getByRole("heading", { name: "Groups" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Deciding a finding" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Status" })).toBeTruthy();
    for (const name of ["Plan this", "Fold into entry", "Dismiss", "Review the feature"]) {
      expect(screen.getAllByText(name).length).toBeGreaterThan(0);
    }
    for (const name of ["Planned", "Folded", "Dismissed"]) {
      expect(screen.getByText(name)).toBeTruthy();
    }
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
