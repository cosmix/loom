import { cleanup, render } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { ACTIVITY_KINDS, ACTIVITY_LABELS, ActivityGlyph } from "./activity-glyphs";

const EXPECTED_KINDS = [
  "permission",
  "waiting",
  "running",
  "pressure",
  "codex",
  "drafting",
  "summaries",
  "starting",
] as const;

const css = readFileSync(join(import.meta.dirname, "activity-glyphs.css"), "utf8");

afterEach(cleanup);

describe("activity glyphs", () => {
  it("lists every kind exactly once in urgency order", () => {
    expect(ACTIVITY_KINDS).toEqual(EXPECTED_KINDS);
    expect(new Set(ACTIVITY_KINDS).size).toBe(EXPECTED_KINDS.length);
  });

  it("provides the sentence-case labels", () => {
    expect(ACTIVITY_LABELS).toEqual({
      permission: "Needs permission",
      waiting: "Waiting for you",
      running: "Plan running",
      pressure: "Pressure test",
      codex: "Codex reviewing",
      drafting: "Drafting a document",
      summaries: "Rebuilding summaries",
      starting: "Session starting",
    });
  });

  it.each(EXPECTED_KINDS)("renders a decorative %s glyph with a label", (kind) => {
    const { container } = render(<ActivityGlyph kind={kind} className="size-5" />);
    const glyph = container.firstElementChild;

    expect(glyph?.classList.contains("activity-glyph")).toBe(true);
    expect(glyph?.classList.contains("size-5")).toBe(true);
    expect(glyph?.getAttribute("data-kind")).toBe(kind);
    expect(glyph?.getAttribute("aria-hidden")).toBe("true");
    expect(glyph?.hasAttribute("data-paused")).toBe(false);
    expect(
      glyph?.querySelector(kind === "waiting" ? ".activity-glyph__led" : "svg"),
    ).not.toBeNull();
    expect(ACTIVITY_LABELS[kind].trim().length).toBeGreaterThan(0);
  });

  it.each(EXPECTED_KINDS)("exposes and clears the paused state for %s", (kind) => {
    const { container, rerender } = render(<ActivityGlyph kind={kind} paused />);
    expect(container.firstElementChild?.getAttribute("data-paused")).toBe("true");

    rerender(<ActivityGlyph kind={kind} paused={false} />);
    expect(container.firstElementChild?.hasAttribute("data-paused")).toBe(false);
  });

  it("includes the outgoing ring alongside the running icon", () => {
    const { container } = render(<ActivityGlyph kind="running" />);
    expect(container.querySelector(".activity-glyph > .activity-glyph__pulse")).not.toBeNull();
    expect(container.querySelector(".activity-glyph > svg")).not.toBeNull();
  });

  it.each(EXPECTED_KINDS)("defines and uses CSS keyframes for %s", (kind) => {
    expect(css).toMatch(new RegExp(`@keyframes\\s+activity-glyph-${kind}\\s*\\{`));
    expect(css).toMatch(new RegExp(`animation:\\s*activity-glyph-${kind}\\s`));
  });

  it("pauses nested animation targets through the root attribute", () => {
    expect(css).toMatch(
      /\.activity-glyph\[data-paused="true"\],\s*\.activity-glyph\[data-paused="true"\] \*\s*\{[^}]*animation-play-state:\s*paused\s*!important;/,
    );
  });

  it("disables all glyph animations, including the ring, for reduced motion", () => {
    const reducedMotion = css.match(
      /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([\s\S]*?)\n\}/,
    )?.[1];

    expect(reducedMotion).toBeDefined();
    expect(reducedMotion).toMatch(
      /\.activity-glyph,\s*\.activity-glyph \*\s*\{\s*animation:\s*none;/,
    );
  });
});
