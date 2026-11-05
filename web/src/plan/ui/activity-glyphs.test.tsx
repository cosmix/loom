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
    const shape =
      kind === "waiting"
        ? ".activity-glyph__led"
        : kind === "permission"
          ? "svg"
          : ".aurora-roundel";
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
    "draws %s as the one spinning roundel, with no icon and no announcement",
    (kind) => {
      const { container } = render(<ActivityGlyph kind={kind} />);

      expect(container.querySelector(".aurora-roundel")?.getAttribute("data-busy")).toBe("true");
      expect(container.querySelector("svg")).toBeNull();
      expect(container.textContent).toBe("");
      expect(
        container.querySelector('.activity-glyph[aria-hidden="true"] .aurora-roundel'),
      ).not.toBeNull();
    },
  );

  it("rests the roundel while paused", () => {
    const { container } = render(<ActivityGlyph kind="running" paused />);
    expect(container.querySelector(".aurora-roundel")?.hasAttribute("data-busy")).toBe(false);
  });

  it("sizes the roundel to the glyph box it replaces", () => {
    expect(css).toMatch(
      /\.activity-glyph > \.aurora-roundel-wrap\s*\{[^}]*width:\s*86%;[^}]*height:\s*86%;/,
    );
  });

  it("gives the waiting LED the completed tone and the bell its pivot", () => {
    expect(css).toMatch(/\[data-kind="waiting"\]\s*\{\s*color:\s*var\(--tone-completed\)/);
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

  it("keeps no keyframes for the kinds the roundel draws", () => {
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
