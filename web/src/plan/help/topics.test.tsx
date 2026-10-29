import { cleanup, render, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { GettingAroundScene } from "./getting-around-scene";
import { PlannerLegend } from "./legend";
import { PlannerShortcuts } from "./shortcuts";
import { PLANNER_HELP_TOPICS } from "./topics";

beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new Error("Help must not request live data")));
  vi.stubGlobal("WebSocket", vi.fn());
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === "(min-width: 1024px)",
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("provides the three topics in order with unique ids and their production bodies", () => {
  expect(PLANNER_HELP_TOPICS.map(({ id, label }) => [id, label])).toEqual([
    ["getting-around", "Getting around"],
    ["legend", "Legend"],
    ["shortcuts", "Shortcuts"],
  ]);
  expect(new Set(PLANNER_HELP_TOPICS.map(({ id }) => id)).size).toBe(3);
  expect(PLANNER_HELP_TOPICS[0]!.scenes).toHaveLength(1);
  expect(PLANNER_HELP_TOPICS[0]!.scenes[0]!.Scene).toBe(GettingAroundScene);
  expect(PLANNER_HELP_TOPICS[0]!.scenes[0]!.scale).toBe(0.74);
  expect(PLANNER_HELP_TOPICS[1]!.Body).toBe(PlannerLegend);
  expect(PLANNER_HELP_TOPICS[1]!.scenes).toEqual([]);
  expect(PLANNER_HELP_TOPICS[2]!.Body).toBe(PlannerShortcuts);
  expect(PLANNER_HELP_TOPICS[2]!.scenes).toEqual([]);
});

it("gives every topic an intro and description and every scene note non-empty text", () => {
  for (const topic of PLANNER_HELP_TOPICS) {
    expect(topic.description.trim().length).toBeGreaterThan(0);
    expect(topic.intro.trim().length).toBeGreaterThan(0);
    expect(topic.hue).toBeGreaterThanOrEqual(1);
    expect(topic.hue).toBeLessThanOrEqual(8);
    for (const scene of topic.scenes) {
      expect(scene.notes).toHaveLength(6);
      for (const note of scene.notes) {
        expect(note.text.trim().length).toBeGreaterThan(0);
      }
    }
  }
});

it("renders the sample project and all seven steps without throwing", () => {
  const { container } = render(<GettingAroundScene />);

  expect(
    within(container).getByRole("button", { name: "Switch feature, current: atlas", hidden: true }),
  ).toBeDefined();
  const steps = within(container).getByRole("list", { name: "Steps", hidden: true });
  expect(within(steps).getAllByRole("listitem", { hidden: true })).toHaveLength(7);
  expect(
    within(steps).getByRole("listitem", { current: "step", hidden: true }).textContent,
  ).toContain("Metaplan");
});

it("matches every pin selector inside its rendered production scene", () => {
  for (const topic of PLANNER_HELP_TOPICS) {
    for (const { Scene, notes } of topic.scenes) {
      const { container, unmount } = render(<Scene />);
      for (const note of notes) {
        const targets = typeof note.target === "string" ? [note.target] : note.target;
        expect(targets.length).toBeGreaterThan(0);
        for (const selector of targets) {
          expect(container.querySelector(selector), selector).not.toBeNull();
        }
      }
      unmount();
    }
  }
});

it("keeps the scene inert without HTTP requests or a live socket", () => {
  const { container } = render(<GettingAroundScene />);

  expect(container.querySelector("[inert]")).not.toBeNull();
  expect(globalThis.fetch).not.toHaveBeenCalled();
  expect(globalThis.WebSocket).not.toHaveBeenCalled();
});
