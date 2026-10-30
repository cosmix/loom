import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { THEME_OPTIONS } from "@/components/theme-picker";
import { PlannerThemeSwatch } from "./planner-swatch";

afterEach(cleanup);

describe("PlannerThemeSwatch", () => {
  it("renders a decorative 120 by 76 planner thumbnail", () => {
    const palette = THEME_OPTIONS[0]!.palette;
    const { container } = render(PlannerThemeSwatch(palette));
    const svg = container.querySelector("svg");

    expect(svg).not.toBeNull();
    expect(svg?.getAttribute("width")).toBe("120");
    expect(svg?.getAttribute("height")).toBe("76");
    expect(svg?.getAttribute("viewBox")).toBe("0 0 120 76");
    expect(svg?.getAttribute("aria-hidden")).toBe("true");
    expect(svg?.style.display).toBe("block");
    expect(svg?.style.borderRadius).toBe("5px");
  });

  it.each(THEME_OPTIONS)(
    "accents only the spine's current row with the $label primary",
    ({ palette }) => {
      const { container } = render(PlannerThemeSwatch(palette));
      const group = container.querySelector('g[data-region="spine"]')!;
      const marks = group.querySelectorAll<SVGCircleElement>(":scope > g > circle");
      const accentMarks = Array.from(marks).filter((mark) => mark.style.fill === palette.primary);

      expect(marks).toHaveLength(6);
      expect(accentMarks).toEqual([marks[1]]);
      expect(Array.from(group.querySelectorAll("circle[data-current]"))).toEqual([marks[1]]);
      expect(
        group.querySelectorAll<SVGCircleElement>(":scope > g > circle[style*='opacity: 0.9']"),
      ).toHaveLength(3);
    },
  );

  it("draws one primary button and the chat card", () => {
    const palette = THEME_OPTIONS[0]!.palette;
    const { container } = render(PlannerThemeSwatch(palette));
    const rects = Array.from(container.querySelectorAll<SVGRectElement>("rect"));
    const buttons = rects.filter(
      (rect) => rect.style.fill === palette.primary && rect.style.opacity === "",
    );

    expect(buttons).toHaveLength(1);
    expect(container.querySelectorAll('rect[x="84"][width="32"]')).toHaveLength(1);
  });

  it("changes its fills when a different palette is supplied", () => {
    const ledger = THEME_OPTIONS[0]!.palette;
    const pacific = THEME_OPTIONS[2]!.palette;
    const { container, rerender } = render(PlannerThemeSwatch(ledger));
    const initialFills = Array.from(
      container.querySelectorAll<SVGRectElement>("rect"),
      (rect) => rect.style.fill,
    );

    rerender(PlannerThemeSwatch(pacific));

    const nextFills = Array.from(
      container.querySelectorAll<SVGRectElement>("rect"),
      (rect) => rect.style.fill,
    );
    expect(nextFills).not.toEqual(initialFills);
    expect(initialFills[0]).toBe(ledger.background);
    expect(nextFills[0]).toBe(pacific.background);
    expect(nextFills).toContain(pacific.primary);
    expect(nextFills).not.toContain(ledger.primary);
  });
});
