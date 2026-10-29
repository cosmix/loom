import { cleanup, render, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { CHAT_SCENES } from "./chat-scene";
import { FEATURES_SCENES } from "./features-scene";
import { GettingAroundScene } from "./getting-around-scene";
import { PlannerLegend } from "./legend";
import { PRESSURE_SCENES } from "./pressure-scene";
import { READING_SCENES } from "./reading-scene";
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

it("provides the seven topics in order with unique ids and their production content", () => {
  expect(PLANNER_HELP_TOPICS.map(({ id, label }) => [id, label])).toEqual([
    ["getting-around", "Getting around"],
    ["reading", "Reading summaries"],
    ["features", "Features"],
    ["chat", "Chat and sessions"],
    ["pressure", "Pressure tests and runs"],
    ["legend", "Legend"],
    ["shortcuts", "Shortcuts"],
  ]);
  expect(new Set(PLANNER_HELP_TOPICS.map(({ id }) => id)).size).toBe(7);
  const [around, reading, features, chat, pressure, legend, shortcuts] = PLANNER_HELP_TOPICS;
  expect(around!.scenes).toHaveLength(1);
  expect(around!.scenes[0]!.Scene).toBe(GettingAroundScene);
  expect(around!.scenes[0]!.scale).toBe(0.74);
  expect(reading!.scenes).toBe(READING_SCENES);
  expect(features!.scenes).toBe(FEATURES_SCENES);
  expect(chat!.scenes).toBe(CHAT_SCENES);
  expect(pressure!.scenes).toBe(PRESSURE_SCENES);
  for (const topic of [reading!, features!, chat!, pressure!]) {
    expect(topic.scenes.length).toBeGreaterThanOrEqual(2);
    expect(topic.tips?.length ?? 0).toBeGreaterThan(0);
  }
  expect(legend!.Body).toBe(PlannerLegend);
  expect(legend!.scenes).toEqual([]);
  expect(shortcuts!.Body).toBe(PlannerShortcuts);
  expect(shortcuts!.scenes).toEqual([]);
});

it("gives every topic an intro and description and every scene note non-empty text", () => {
  for (const topic of PLANNER_HELP_TOPICS) {
    expect(topic.description.trim().length).toBeGreaterThan(0);
    expect(topic.intro.trim().length).toBeGreaterThan(0);
    expect(topic.hue).toBeGreaterThanOrEqual(1);
    expect(topic.hue).toBeLessThanOrEqual(8);
    for (const scene of topic.scenes) {
      expect(scene.notes.length).toBeGreaterThanOrEqual(3);
      expect(scene.notes.length).toBeLessThanOrEqual(6);
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
        // A note may list a selector per responsive layout; one of them must match here.
        expect(
          targets.some((selector) => container.querySelector(selector) !== null),
          targets.join(" | "),
        ).toBe(true);
      }
      unmount();
    }
  }
});

it("pins the header's logo, switcher, activity pill, settings and help once each", () => {
  const [around] = PLANNER_HELP_TOPICS;
  const notes = around!.scenes[0]!.notes;
  expect(notes).toHaveLength(6);
  expect(notes[0]!.text).toContain("feature switcher");
  const { container } = render(<GettingAroundScene />);

  const pill = notes.find((note) => note.text.startsWith("The activity pill"));
  expect(pill).toBeDefined();
  const matches = container.querySelectorAll(pill!.target as string);
  expect(matches).toHaveLength(1);
  expect(matches[0]!.getAttribute("aria-label")).toBe("1 in progress: Waiting for you");

  const settingsAndHelp = notes[notes.length - 1]!;
  const selectors = ['[aria-label="Settings"]', '[aria-label="Help"]'];
  expect(settingsAndHelp.target).toEqual(selectors);
  for (const selector of selectors) {
    expect(container.querySelectorAll(selector)).toHaveLength(1);
  }
});

it("pins the step rail in the wide header and its popover button in the narrow one", () => {
  const stepNote = PLANNER_HELP_TOPICS[0]!.scenes[0]!.notes[1]!;
  const [rail, button] = stepNote.target as readonly string[];
  const wide = render(<GettingAroundScene />);
  expect(wide.container.querySelectorAll(rail!)).toHaveLength(1);
  wide.unmount();

  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener() {},
    removeEventListener() {},
    addListener() {},
    removeListener() {},
    dispatchEvent: () => false,
  }));
  const narrow = render(<GettingAroundScene />);
  expect(narrow.container.querySelectorAll(rail!)).toHaveLength(0);
  const popover = narrow.container.querySelectorAll(button!);
  expect(popover).toHaveLength(1);
  expect(popover[0]!.textContent).toContain("Step 3 of 7");
});

it("keeps the scene inert without HTTP requests or a live socket", () => {
  const { container } = render(<GettingAroundScene />);

  expect(container.querySelector("[inert]")).not.toBeNull();
  expect(globalThis.fetch).not.toHaveBeenCalled();
  expect(globalThis.WebSocket).not.toHaveBeenCalled();
});
