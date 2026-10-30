import { cleanup, render, within } from "@testing-library/react";
import { getDefaultStore } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import {
  FEATURES_SCENES,
  FEATURES_TIPS,
  FeaturesHomeScene,
  FeaturesKickoffScene,
} from "@/plan/help/features-scene";
import { SAMPLE_SERVER } from "@/plan/help/sample";
import { serverInfoAtom } from "@/plan/state/atoms";

const SHELF_BUTTONS = "section[aria-labelledby] button";

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

it("provides the two Features scenes in order", () => {
  expect(FEATURES_SCENES.map(({ title, Scene }) => [title, Scene])).toEqual([
    ["The project home", FeaturesHomeScene],
    ["Kicking off a feature", FeaturesKickoffScene],
  ]);
});

it.each(FEATURES_SCENES)(
  "renders $title with matching pins and no live data",
  ({ Scene, notes }) => {
    const { container } = render(<Scene />);

    expect(container.querySelector("[data-loom-plan][inert]")).not.toBeNull();
    expect(notes.length).toBeGreaterThanOrEqual(3);
    expect(notes.length).toBeLessThanOrEqual(6);
    for (const note of notes) {
      expect(note.text.trim()).toMatch(/^[^.!?\r\n]+[.!?]$/u);
      const selectors = typeof note.target === "string" ? [note.target] : note.target;
      expect(selectors.length).toBeGreaterThan(0);
      for (const selector of selectors) {
        expect(container.querySelector(selector), selector).not.toBeNull();
      }
    }
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  },
);

it("rings one element per note, except the two shelf buttons that share a note", () => {
  for (const { Scene, notes } of FEATURES_SCENES) {
    const { container, unmount } = render(<Scene />);
    for (const { target } of notes) {
      const selectors = typeof target === "string" ? [target] : target;
      const count = selectors.reduce(
        (sum, selector) => sum + container.querySelectorAll(selector).length,
        0,
      );
      expect(count, selectors.join(", ")).toBe(selectors[0] === SHELF_BUTTONS ? 2 : 1);
    }
    unmount();
  }
});

it("anchors each kickoff note to the section its text names", () => {
  const { container } = render(<FeaturesKickoffScene />);
  const headings = (FEATURES_SCENES[1]?.notes ?? []).map(({ target }) => {
    const selector = typeof target === "string" ? target : target.join(", ");
    return container.querySelector(selector)?.querySelector(":scope > h3")?.textContent;
  });

  expect(headings).toEqual([
    "Size",
    "Name",
    "Problem",
    "Settled decisions",
    "Open questions",
    "Sections",
  ]);
});

it("draws the invitation beside the feature cards and pins it first", () => {
  const { container } = render(<FeaturesHomeScene />);
  const scene = within(container);

  expect(scene.getByRole("region", { name: "Start something new", hidden: true })).toBeDefined();
  expect(
    scene.getByRole("link", { name: "Start a conversation", hidden: true }).getAttribute("href"),
  ).toBe("/new");
  const [invitation] = FEATURES_SCENES[0]!.notes;
  expect(invitation?.text).toBe(
    "The invitation starts a conversation about a new feature or a standalone plan.",
  );
  expect(container.querySelectorAll(invitation?.target as string)).toHaveLength(1);
  expect(container.querySelector(".card-grid__lead .invitation")).not.toBeNull();
});

it("shows nothing of the host's server state in the kickoff scene", () => {
  const store = getDefaultStore();
  store.set(serverInfoAtom, {
    ...SAMPLE_SERVER,
    read_only: [{ what: "claude_login", message: "Claude Code is not logged in", fix: "login" }],
  });
  try {
    const { container } = render(<FeaturesKickoffScene />);

    expect(container.textContent).not.toContain("read-only mode");
    expect(container.textContent).not.toContain("not logged in");
  } finally {
    store.set(serverInfoAtom, null);
  }
});

it("renders the feature progress and standalone shelf", () => {
  const { container } = render(<FeaturesHomeScene />);
  const scene = within(container);

  expect(scene.getByRole("link", { name: "atlas", hidden: true }).getAttribute("href")).toBe(
    "/f/atlas",
  );
  const progress = scene.getByRole("list", { name: "Progress", hidden: true });
  expect(within(progress).getAllByRole("listitem", { hidden: true })).toHaveLength(6);
  expect(scene.getByRole("region", { name: "Standalone plans", hidden: true })).toBeDefined();
  expect(
    scene.getByRole("button", { name: "Add plan", hidden: true }).getAttribute("aria-disabled"),
  ).not.toBe("true");
});

it("renders the kickoff summary with decisions, questions, and all section states", () => {
  const { container } = render(<FeaturesKickoffScene />);
  const panel = within(container).getByRole("complementary", {
    name: "Taking shape",
    hidden: true,
  });
  const shape = within(panel);

  expect(shape.getByText("Feature")).toBeDefined();
  expect(shape.getByText("atlas")).toBeDefined();
  expect(shape.getByText("Keep the project map beside the documents it describes.")).toBeDefined();
  expect(shape.getByText("How should archived work appear in the map?")).toBeDefined();
  expect(shape.getByText("Settled")).toBeDefined();
  expect(shape.getByText("Being worded")).toBeDefined();
  expect(shape.getByText("Open")).toBeDefined();
});

it("provides one to three single sentence tips", () => {
  expect(FEATURES_TIPS.length).toBeGreaterThanOrEqual(1);
  expect(FEATURES_TIPS.length).toBeLessThanOrEqual(3);
  for (const tip of FEATURES_TIPS) {
    expect(tip.trim()).toMatch(/^[^.!?\r\n]+[.!?]$/u);
  }
});
