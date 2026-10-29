import { cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import {
  READING_SCENES,
  READING_TIPS,
  ReadingFoldScene,
  ReadingSentenceScene,
} from "@/plan/help/reading-scene";

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

it("exports the two reading scenes in order", () => {
  expect(READING_SCENES.map(({ title, Scene }) => ({ title, Scene }))).toEqual([
    { title: "A summary sentence and its marks", Scene: ReadingSentenceScene },
    { title: "Folding and sources", Scene: ReadingFoldScene },
  ]);
});

it.each(READING_SCENES)(
  "renders $title with every pin target and no live connection",
  ({ Scene, notes }) => {
    const { container } = render(<Scene />);

    expect(container.querySelector("[data-loom-plan][inert]")).not.toBeNull();
    expect(notes.length).toBeGreaterThanOrEqual(3);
    expect(notes.length).toBeLessThanOrEqual(6);
    for (const note of notes) {
      const selectors = typeof note.target === "string" ? [note.target] : note.target;
      expect(selectors.length).toBeGreaterThan(0);
      for (const selector of selectors) {
        expect(container.querySelector(selector), selector).not.toBeNull();
      }
      expect(note.text.trim()).toMatch(/^[^.!?]+[.!?]$/u);
      expect(note.text.trim().split(/\s+/u).length).toBeGreaterThanOrEqual(5);
    }
    expect(globalThis.fetch).not.toHaveBeenCalled();
    expect(globalThis.WebSocket).not.toHaveBeenCalled();
  },
);

it.each(READING_SCENES)("rings exactly one element per note in $title", ({ Scene, notes }) => {
  const { container } = render(<Scene />);
  for (const { target } of notes) {
    const selectors = typeof target === "string" ? [target] : target;
    const matched = new Set(
      selectors.flatMap((selector) => [...container.querySelectorAll(selector)]),
    );
    expect(matched.size, selectors.join(", ")).toBe(1);
  }
});

it("shows both current and out of date document marks on the real summary sentence", () => {
  const { container } = render(<ReadingSentenceScene />);

  expect(container.querySelector(".concept-sentence strong")?.textContent).toBe("save places");
  expect(container.querySelectorAll(".concept-mark")).toHaveLength(2);
  expect(container.querySelector('[data-dashed="true"]')?.getAttribute("aria-label")).toContain(
    "out of date: source changed, rebuild pending",
  );
});

it("renders both fold caps and the loaded source excerpt without requesting a document", () => {
  const { container } = render(<ReadingFoldScene />);

  expect(container.querySelectorAll(".concept-run-cap")).toHaveLength(2);
  expect(container.querySelector(".concept-passage-chip")?.getAttribute("href")).toBe(
    "/files/product/map-view.md?lines=1-3#saved-places",
  );
  expect(container.querySelector(".concept-passage .md")?.textContent).toContain(
    "Save a place from the map to find it again on your next visit.",
  );
  expect(container.querySelector(".concept-muted")).toBeNull();
  expect(globalThis.fetch).not.toHaveBeenCalled();
  expect(globalThis.WebSocket).not.toHaveBeenCalled();
});

it("provides one to three single sentence reading tips", () => {
  expect(READING_TIPS.length).toBeGreaterThanOrEqual(1);
  expect(READING_TIPS.length).toBeLessThanOrEqual(3);
  for (const tip of READING_TIPS) {
    expect(tip.trim()).toMatch(/^[^.!?]+[.!?]$/u);
  }
});

it("seeds the open passage without writing the reader's session storage", () => {
  sessionStorage.clear();
  const { container } = render(<ReadingFoldScene />);

  expect(container.querySelector(".concept-passage-chip")).not.toBeNull();
  const written = Object.keys(sessionStorage).filter((key) => key.startsWith("loom-plan:concept:"));
  expect(written).toEqual([]);
});
