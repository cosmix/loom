import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import { LoomDoodle } from "./doodle";

const TITLE = "A thread leaves a spool, loops through a thought and weaves into cloth on a loom";
const css = readFileSync(join(import.meta.dirname, "doodle.css"), "utf8");

afterEach(cleanup);

it("renders an accessible SVG with its descriptive title and caller class", () => {
  render(<LoomDoodle className="invitation-art" />);
  const svg = screen.getByRole("img", { name: TITLE });
  expect(svg.tagName.toLowerCase()).toBe("svg");
  expect(svg.querySelector("title")?.textContent).toBe(TITLE);
  expect(svg.classList.contains("loom-doodle")).toBe(true);
  expect(svg.classList.contains("invitation-art")).toBe(true);
});

it("normalizes every thread path for the redraw animation", () => {
  render(<LoomDoodle />);
  const threads = screen
    .getByRole("img", { name: TITLE })
    .querySelectorAll("path.loom-doodle__thread");
  expect(threads.length).toBeGreaterThan(0);
  for (const thread of threads) {
    expect(thread.getAttribute("pathLength")).toBe("1");
  }
});

it("draws a spool, a six-warp loom frame, one weft ending at a dot and three sparks", () => {
  render(<LoomDoodle />);
  const svg = screen.getByRole("img", { name: TITLE });
  expect(svg.getAttribute("viewBox")).toBe("0 0 270 128");
  expect(svg.querySelectorAll("rect.loom-doodle__ink")).toHaveLength(1);
  expect(svg.querySelectorAll("path.loom-doodle__thread")).toHaveLength(1);
  expect(svg.querySelectorAll("path.loom-doodle__spark")).toHaveLength(3);
  const dot = svg.querySelector("circle.loom-doodle__dot");
  expect([dot?.getAttribute("cx"), dot?.getAttribute("cy"), dot?.getAttribute("r")]).toEqual([
    "204",
    "92",
    "2.6",
  ]);
  const warps = [...svg.querySelectorAll("path.loom-doodle__ink")].filter((path) =>
    /^M\d+ 52V112$/.test(path.getAttribute("d") ?? ""),
  );
  expect(warps).toHaveLength(6);
});

it("keeps threads drawn at rest and supports local and host interaction", () => {
  expect(css).toMatch(/stroke-dasharray:\s*1\s*;/);
  expect(css).toMatch(/stroke-dashoffset:\s*0\s*;/);
  for (const state of ["hover", "focus-within"]) {
    for (const part of ["thread", "spark", "dot"]) {
      expect(css).toContain(`.loom-doodle:${state} .loom-doodle__${part}`);
      expect(css).toContain(`[data-doodle-host]:${state} .loom-doodle .loom-doodle__${part}`);
    }
  }
});

it("disables animations and leaves the drawing complete for reduced motion", () => {
  const reducedMotion = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce)"));
  expect(reducedMotion).toContain("@media (prefers-reduced-motion: reduce)");
  expect(reducedMotion).toMatch(
    /\.loom-doodle\s*,\s*\.loom-doodle\s*\*\s*\{\s*animation:\s*none\s*!important;/,
  );
  expect(reducedMotion).toMatch(/stroke-dashoffset:\s*0\s*;/);
  expect(reducedMotion).toMatch(/opacity:\s*1\s*;/);
  expect(reducedMotion).toMatch(/transform:\s*none\s*;/);
});
