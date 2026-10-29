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

  it.each(THEME_OPTIONS)("fills only the fourth step with the $label accent", ({ palette }) => {
    const { container } = render(PlannerThemeSwatch(palette));
    const steps = container.querySelectorAll<SVGRectElement>('g[data-region="steps"] > rect');
    const accentFills = Array.from(container.querySelectorAll<SVGElement>("svg [style]")).filter(
      (element) => element.style.fill === palette.primary,
    );

    expect(steps).toHaveLength(7);
    expect(accentFills).toEqual([steps[3]]);
    expect(steps[3]?.getAttribute("style")).toContain(`fill: ${palette.primary}`);
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
