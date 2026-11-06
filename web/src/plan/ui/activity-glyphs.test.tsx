import { cleanup, render } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import {
  ACTIVITY_KINDS,
  ACTIVITY_LABELS,
  ACTIVITY_TONES,
  ActivityGlyph,
  ActivityTile,
  isProgressKind,
  LiveDot,
  PROGRESS_KINDS,
} from "./activity-glyphs";

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

const WORKING = ["running", "pressure", "codex", "drafting", "summaries", "starting"] as const;

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

  it("names the working kinds and leaves the your-turn kinds out", () => {
    expect([...PROGRESS_KINDS].sort()).toEqual([...WORKING].sort());
    expect(isProgressKind("waiting")).toBe(false);
    expect(isProgressKind("permission")).toBe(false);
  });

  it.each(EXPECTED_KINDS)("renders a decorative %s glyph with a label", (kind) => {
    const { container } = render(<ActivityGlyph kind={kind} className="size-5" />);
    const glyph = container.firstElementChild;

    expect(glyph?.classList.contains("activity-glyph")).toBe(true);
    expect(glyph?.classList.contains("size-5")).toBe(true);
    expect(glyph?.getAttribute("data-kind")).toBe(kind);
    expect(glyph?.getAttribute("aria-hidden")).toBe("true");
    expect(glyph?.hasAttribute("data-paused")).toBe(false);
    const shape = kind === "permission" ? "svg" : ".activity-glyph__led";
    expect(glyph?.querySelector(shape)).not.toBeNull();
    expect(ACTIVITY_LABELS[kind].trim().length).toBeGreaterThan(0);
  });

  it.each(EXPECTED_KINDS)("exposes and clears the paused state for %s", (kind) => {
    const { container, rerender } = render(<ActivityGlyph kind={kind} paused />);
    expect(container.firstElementChild?.getAttribute("data-paused")).toBe("true");

    rerender(<ActivityGlyph kind={kind} paused={false} />);
    expect(container.firstElementChild?.hasAttribute("data-paused")).toBe(false);
  });

  it.each(WORKING)(
    "draws %s as the live status dot: no spinner, no roundel, no announcement",
    (kind) => {
      const { container } = render(<ActivityGlyph kind={kind} />);

      const glyph = container.querySelector('.activity-glyph[aria-hidden="true"]');
      expect(glyph?.querySelector(".activity-glyph__led")).not.toBeNull();
      expect(container.querySelector("svg")).toBeNull();
      expect(container.querySelector(".aurora-roundel")).toBeNull();
      expect(container.textContent).toBe("");
    },
  );

  it("draws the live dot for a running plan's state glyph, sized like an icon", () => {
    const { container } = render(<LiveDot className="size-4" size={14} />);
    const glyph = container.firstElementChild as HTMLElement;
    expect(glyph.classList.contains("activity-glyph")).toBe(true);
    expect(glyph.classList.contains("size-4")).toBe(true);
    expect(glyph.getAttribute("data-kind")).toBe("running");
    expect(glyph.getAttribute("aria-hidden")).toBe("true");
    expect(glyph.style.width).toBe("14px");
    expect(glyph.querySelector(".activity-glyph__led")).not.toBeNull();
  });

  it("greens a live dot, grays a paused one, and pulses it only with motion allowed", () => {
    expect(css).toMatch(
      /\.activity-glyph:not\(\[data-kind="permission"\]\)\s*\{\s*color:\s*var\(--tone-completed\)/,
    );
    expect(css).toMatch(
      /\.activity-glyph\[data-paused="true"\]\s*\{\s*color:\s*var\(--muted-foreground\)/,
    );
    expect(css).toMatch(/\.activity-glyph__led\s*\{[^}]*animation:\s*activity-glyph-waiting/);
  });

  it("sizes the header pill's roundel to the glyph box", () => {
    expect(css).toMatch(
      /\.activity-glyph > \.aurora-roundel-wrap\s*\{[^}]*width:\s*86%;[^}]*height:\s*86%;/,
    );
  });

  it("keeps one LED pattern and gives the bell its pivot", () => {
    expect(css).not.toMatch(/activity-glyph__pulse/);
    expect(css).toMatch(/transform-origin:\s*50% 10%/);
  });

  it.each([
    ["running", "executing"],
    ["summaries", "executing"],
    ["codex", "executing"],
    ["starting", "executing"],
    ["pressure", "ring"],
    ["drafting", "ring"],
    ["waiting", "completed"],
    ["permission", "warning"],
  ] as const)("tiles the %s glyph in the %s tone", (kind, tone) => {
    expect(ACTIVITY_TONES[kind]).toBe(tone);
    const { container } = render(<ActivityTile kind={kind} />);

    expect(container.firstElementChild?.getAttribute("data-tone")).toBe(tone);
    expect(container.querySelector(`[data-kind="${kind}"]`)).not.toBeNull();
  });

  it("draws children in a neutral tile", () => {
    const { container } = render(
      <ActivityTile>
        <svg />
      </ActivityTile>,
    );

    expect(container.firstElementChild?.getAttribute("data-tone")).toBe("neutral");
    expect(container.querySelector("[data-kind]")).toBeNull();
  });

  it.each(["waiting", "permission"] as const)("defines and uses CSS keyframes for %s", (kind) => {
    expect(css).toMatch(new RegExp(`@keyframes\\s+activity-glyph-${kind}\\s*\\{`));
    expect(css).toMatch(new RegExp(`animation:\\s*activity-glyph-${kind}\\s`));
  });

  it("keeps no keyframes of their own for the kinds the status dot draws", () => {
    for (const kind of WORKING) {
      expect(css).not.toMatch(new RegExp(`activity-glyph-${kind}\\b`));
    }
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
