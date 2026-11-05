import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, render, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
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
    // Every min-width query holds, so the header and the spine take their wide layouts.
    matches: query.startsWith("(min-width"),
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
    expect(topic.scenes.length).toBeGreaterThanOrEqual(1);
    expect(topic.tips?.length ?? 0).toBeGreaterThan(0);
  }
  expect(reading!.scenes).toHaveLength(1);
  expect(pressure!.scenes).toHaveLength(1);
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

const SPINE = 'nav[aria-label="Feature documents"]';

it("renders the sample project and its spine without throwing", () => {
  const { container } = render(<GettingAroundScene />);

  expect(
    within(container).getByRole("button", { name: "Switch feature, current: atlas", hidden: true }),
  ).toBeDefined();
  expect(container.querySelectorAll(SPINE)).toHaveLength(1);
  expect(container.querySelectorAll(".plan-spine-next")).toHaveLength(1);
});

it.each(["/", "/files", "/f/other/product"])(
  "keeps the sample feature spine when help opens from %s",
  (pathname) => {
    const router = createMemoryRouter([{ path: "*", element: <GettingAroundScene /> }], {
      initialEntries: [`${pathname}?help=open#topic`],
    });
    const { container } = render(<RouterProvider router={router} />);

    expect(router.state.errors).toBeNull();
    expect(
      within(container).getByRole("button", {
        name: "Switch feature, current: atlas",
        hidden: true,
      }),
    ).toBeDefined();
    expect(container.querySelectorAll(SPINE)).toHaveLength(1);
    expect(router.state.location).toMatchObject({ pathname, search: "?help=open", hash: "#topic" });
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  },
);

/** True when the element sits in a collapsed <details> body, where it takes no space. */
function hiddenByDetails(element: Element): boolean {
  const summary = element.closest("summary");
  let node = element.closest("details");
  while (node) {
    const inOwnSummary = summary !== null && summary.parentElement === node;
    if (!node.open && !inOwnSummary) return true;
    node = node.parentElement?.closest("details") ?? null;
  }
  return false;
}

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
        // Every ringed element is on screen: not hidden, and not inside a closed <details>.
        const matched = targets.flatMap((selector) => [...container.querySelectorAll(selector)]);
        for (const element of matched) {
          expect(element.closest("[hidden]"), targets.join(" | ")).toBeNull();
          expect(hiddenByDetails(element), targets.join(" | ")).toBe(false);
        }
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

it("pins the spine's next step on the third note, not Share", () => {
  const note = PLANNER_HELP_TOPICS[0]!.scenes[0]!.notes[2]!;
  const targets = typeof note.target === "string" ? [note.target] : note.target;
  const { container } = render(<GettingAroundScene />);

  const matched = new Set(targets.flatMap((selector) => [...container.querySelectorAll(selector)]));
  expect(matched.size).toBe(1);
  const next = [...matched][0] as HTMLElement;
  expect(next.classList.contains("plan-spine-next")).toBe(true);
  expect(next.textContent).toMatch(/metaplan/i);
  expect(within(next).queryByRole("button", { name: /share/i, hidden: true })).toBeNull();
  expect(next.contains(container.querySelector('[aria-label="Settings"]'))).toBe(false);
});

it("pins the whole spine, which holds no removed step and cannot scroll the scene", () => {
  const note = PLANNER_HELP_TOPICS[0]!.scenes[0]!.notes[1]!;
  expect(note.target).toBe(SPINE);
  const { container } = render(<GettingAroundScene />);

  const spine = container.querySelector(SPINE)!;
  expect(container.querySelectorAll(SPINE)).toHaveLength(1);
  expect(spine.textContent).not.toContain("Plan review");
  // Five groups: the pressure test is a plan's record, not a row of its own.
  expect(spine.textContent).not.toContain("Pressure test");
  for (const group of ["Vision", "Product spec", "Metaplan", "Plans", "Run"]) {
    expect(spine.textContent, group).toContain(group);
  }
  // The stage clips a header wider than itself instead of growing a scrollbar in the scene.
  const css = readFileSync(join(import.meta.dirname, "pinned-scene.css"), "utf8");
  expect(css).toMatch(/__stage\s*\{[^}]*overflow-x:\s*clip;/);
});

it("keeps the scene inert without HTTP requests or a live socket", () => {
  const { container } = render(<GettingAroundScene />);

  expect(container.querySelector("[inert]")).not.toBeNull();
  expect(globalThis.fetch).not.toHaveBeenCalled();
  expect(globalThis.WebSocket).not.toHaveBeenCalled();
});

it("states the current planner behavior in the notes, tips and intros", () => {
  const [around, , , chat, pressure] = PLANNER_HELP_TOPICS;
  expect(around!.scenes[0]!.notes[1]!.text).toMatch(/five groups.*Not pressure-tested/);
  expect(around!.scenes[0]!.notes[3]!.text).toContain("roundel");
  const aroundTips = (around!.tips ?? []).join(" ");
  expect(aroundTips).toContain("product overview");
  expect(aroundTips).toContain("Claude and Codex limits");
  expect(aroundTips).toContain("rebuild statistics");
  expect(chat!.intro).toMatch(/Chat button.*End chat.*Stop sits beside Send/);
  expect((pressure!.tips ?? []).join(" ")).toContain("Pressure test button");
});
