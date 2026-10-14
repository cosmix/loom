import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { HeatStrip, Sparkline } from "./sparkline";

afterEach(cleanup);

describe("Sparkline", () => {
  it("labels the chart and preserves dimensions and classes", () => {
    render(
      <Sparkline values={[1, 2]} width={80} height={20} label="Token flow" className="sample" />,
    );

    const chart = screen.getByRole("img", { name: "Token flow" });
    expect(chart.tagName.toLowerCase()).toBe("svg");
    expect(chart.getAttribute("viewBox")).toBe("0 0 80 20");
    expect(chart.classList.contains("sample")).toBe(true);
  });

  it("scales one point per value in both paths and closes the area at the baseline", () => {
    const { container } = render(
      <Sparkline values={[0, 5, 10]} width={80} height={20} label="Flow" />,
    );

    const paths = container.querySelectorAll("path");
    expect(paths).toHaveLength(2);
    expect(paths[0]?.getAttribute("d")).toBe("M 0 20 L 40 10 L 80 0 L 80 20 L 0 20 Z");
    expect(paths[0]?.getAttribute("fill")).toBe("currentColor");
    expect(paths[0]?.getAttribute("fill-opacity")).toBe("0.2");
    expect(paths[1]?.getAttribute("d")).toBe("M 0 20 L 40 10 L 80 0");
    expect(paths[1]?.getAttribute("stroke")).toBe("currentColor");
    expect(paths[1]?.getAttribute("stroke-width")).toBe("1.25");
    expect(paths[1]?.getAttribute("fill")).toBe("none");
    const dot = container.querySelector("circle");
    expect(dot?.getAttribute("cx")).toBe("80");
    expect(dot?.getAttribute("cy")).toBe("0");
    expect(dot?.getAttribute("r")).toBe("1.75");
  });

  it.each([
    { values: [], path: "M 0 20 L 80 20" },
    { values: [0, 0, 0], path: "M 0 20 L 40 20 L 80 20" },
    { values: [0], path: "M 80 20" },
    { values: [5], path: "M 80 0" },
  ])("renders finite geometry for $values", ({ values, path }) => {
    const { container } = render(<Sparkline values={values} width={80} height={20} label="Flow" />);

    expect(container.innerHTML).not.toMatch(/NaN|Infinity/);
    expect(container.querySelector('path[fill="none"]')?.getAttribute("d")).toBe(path);
  });
});

describe("HeatStrip", () => {
  it("labels a column grid with one cell per value and quantizes its levels", () => {
    render(
      <HeatStrip
        values={[0, 1, 25, 50, 75, 100]}
        max={100}
        label="Token heat"
        className="sample"
      />,
    );

    const strip = screen.getByRole("img", { name: "Token heat" });
    expect(strip.classList.contains("sample")).toBe(true);
    expect(strip.style.gridAutoFlow).toBe("column");
    expect(strip.querySelectorAll("span")).toHaveLength(6);
    expect(Array.from(strip.children, (cell) => cell.getAttribute("data-level"))).toEqual([
      "0",
      "1",
      "1",
      "2",
      "3",
      "4",
    ]);
    expect(strip.querySelector("span")?.style.background).toBe("transparent");
    expect(strip.children[3]?.getAttribute("style")).toContain(
      "color-mix(in oklch, var(--tone-executing) 59%, transparent)",
    );
    expect(strip.lastElementChild?.getAttribute("style")).toContain(
      "color-mix(in oklch, var(--tone-executing) 100%, transparent)",
    );
  });

  it("renders transparent zero cells with a zero maximum", () => {
    const { container } = render(<HeatStrip values={[0, 0]} max={0} label="Idle" />);

    expect(container.innerHTML).not.toMatch(/NaN|Infinity/);
    expect(
      Array.from(container.querySelectorAll("span"), (cell) => [
        cell.dataset.level,
        cell.style.background,
      ]),
    ).toEqual([
      ["0", "transparent"],
      ["0", "transparent"],
    ]);
  });

  it("renders an empty grid for an empty series", () => {
    render(<HeatStrip values={[]} max={0} label="No flow" />);

    expect(screen.getByRole("img", { name: "No flow" }).children).toHaveLength(0);
  });
});
