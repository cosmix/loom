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
  it("draws the five columns in the page's own markup, inert, with one card selected", () => {
    const { container } = render(<CoverageSpecimen />);

    // The map's own element is the inert planner root: it is the container its width steps query.
    const map = container.querySelector("[data-loom-plan][inert].maps-coverage");
    expect(map).not.toBeNull();
    expect(
      [...map!.querySelectorAll(".maps-column")].map((column) =>
        column.getAttribute("data-column"),
      ),
    ).toEqual(["vision", "spec", "metaplan", "plans", "findings"]);
    expect(
      map!.querySelectorAll('.maps-card[aria-current="true"][data-lineage="self"]'),
    ).toHaveLength(1);
    expect(map!.querySelector('.maps-card[data-lineage="self"]')?.textContent).toBe("Stops");
  });

  it("shows a gap, a moved document and an open finding, each as a card state", () => {
    const { container } = render(<CoverageSpecimen />);

    const states = [...container.querySelectorAll(".maps-card")].map((card) =>
      card.getAttribute("data-state"),
    );
    expect(states).toContain("uncovered");
    expect(states).toContain("revised");
    expect(states).toContain("open");
    expect(container.querySelector(".maps-group-title")?.textContent).toBe("Trip planning");
  });
});

describe("the coverage topic body", () => {
  it("names the columns, the marks, the filters and the keys without live data", async () => {
    render(<CoverageHelp />);
    await Promise.resolve();

    for (const title of ["The columns", "Marks on a card", "Filters", COVERAGE_SHORTCUTS.title]) {
      expect(screen.getByRole("heading", { name: title })).toBeTruthy();
    }
    for (const name of ["Vision rows", "Spec headings", "Metaplan entries", "Plans and stages"]) {
      expect(screen.getAllByText(name).length).toBeGreaterThan(0);
    }
    expect(screen.getByText("Not covered")).toBeTruthy();
    expect(screen.getByText("Changed since settled")).toBeTruthy();
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
  it("draws one open finding with its id, status, text, About chips and three quiet actions", () => {
    const { container } = render(<FindingSpecimen />);

    const card = container.querySelector(
      '[data-loom-plan][inert] .maps-finding[data-status="open"]',
    );
    expect(card).not.toBeNull();
    expect(card!.querySelector(".maps-finding-id")?.textContent).toBe("F-3");
    expect(card!.querySelector(".maps-finding-status")?.textContent).toBe("Open");
    expect(card!.querySelectorAll(".maps-about .maps-chip")).toHaveLength(2);
    const actions = [...card!.querySelectorAll(".maps-actions button")];
    expect(actions.map((button) => button.textContent)).toEqual([
      "Plan this",
      "Fold into entry",
      "Dismiss",
    ]);
    expect(actions.every((button) => button.getAttribute("data-look") === "quiet")).toBe(true);
  });
});

describe("the findings topic body", () => {
  it("names the three decisions, Review the feature and the four statuses without live data", async () => {
    render(<FindingsHelp />);
    await Promise.resolve();

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
