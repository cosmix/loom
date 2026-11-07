import { cleanup, render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import type { FindingsSource, Instrument } from "@/plan/feature/spine-instruments";

import { InstrumentWell } from "./spine-instrument";

afterEach(cleanup);

function renderWell(instrument: Instrument, current = false) {
  const router = createMemoryRouter(
    [
      {
        path: "/",
        element: (
          <ul>
            <InstrumentWell instrument={instrument} to="/f/atlas/coverage" current={current} />
          </ul>
        ),
      },
    ],
    { initialEntries: ["/"] },
  );
  render(<RouterProvider router={router} />);
}

/** The well's one link, found by its accessible name and (where it has one) its description. */
const link = (name: string, description?: string) =>
  screen.getByRole("link", { name, description });

const dots = (well: HTMLElement) =>
  [...well.querySelectorAll(".plan-spine__instrument-dot")].map((dot) =>
    dot.getAttribute("data-dot"),
  );

describe("the Coverage well", () => {
  it("names the link by its label and figure, and describes it by the footnote", () => {
    renderWell({
      id: "coverage",
      reading: {
        covered: 18,
        total: 22,
        sections: [
          { number: "01", headings: [true, true, true] },
          { number: "03", headings: [true, false] },
          { number: "04", headings: [false, true, false, false] },
        ],
        gaps: ["03", "04"],
      },
    });
    const well = link("Coverage 18 of 22 headings covered", "4 headings open in 03 and 04").closest(
      "li",
    )!;
    expect(well.getAttribute("data-instrument")).toBe("coverage");
    // One dot per heading, in section order; the strip is decoration beside the figure.
    expect(dots(well)).toEqual([
      ...Array<string>(3).fill("covered"),
      "covered",
      "uncovered",
      "uncovered",
      "covered",
      "uncovered",
      "uncovered",
    ]);
    expect(well.querySelectorAll(".plan-spine__instrument-section")).toHaveLength(3);
    expect(well.querySelector(".plan-spine__instrument-strip")?.getAttribute("aria-hidden")).toBe(
      "true",
    );
  });

  it("names one gap's section alone, and reads complete coverage as every heading planned", () => {
    renderWell({
      id: "coverage",
      reading: {
        covered: 1,
        total: 2,
        sections: [{ number: "02", headings: [false, true] }],
        gaps: ["02"],
      },
    });
    link("Coverage 1 of 2 headings covered", "1 heading open in 02");
    cleanup();

    renderWell({
      id: "coverage",
      reading: {
        covered: 2,
        total: 2,
        sections: [{ number: "02", headings: [true, true] }],
        gaps: [],
      },
    });
    link("Coverage 2 of 2 headings covered", "every heading has a plan");
    cleanup();

    renderWell({ id: "coverage", reading: { covered: 0, total: 0, sections: [], gaps: [] } });
    const none = link("Coverage", "no heading to cover yet");
    expect(dots(none.closest("li")!)).toEqual([]);
  });

  it("lists three gapped sections with commas and a closing and", () => {
    renderWell({
      id: "coverage",
      reading: {
        covered: 5,
        total: 8,
        sections: [
          { number: "01", headings: [true, true] },
          { number: "02", headings: [false, true] },
          { number: "03", headings: [false, true] },
          { number: "04", headings: [false, true] },
        ],
        gaps: ["02", "03", "04"],
      },
    });
    link("Coverage 5 of 8 headings covered", "3 headings open in 02, 03 and 04");
  });

  it("counts the gapped sections past three, and says every section when none is whole", () => {
    const section = (number: string, covered: boolean) => ({ number, headings: [covered, true] });
    const numbers = ["01", "02", "03", "04", "05", "06"];
    renderWell({
      id: "coverage",
      reading: {
        covered: 8,
        total: 12,
        sections: numbers.map((number, index) => section(number, index >= 4)),
        gaps: numbers.slice(0, 4),
      },
    });
    link("Coverage 8 of 12 headings covered", "4 headings open in 4 of 6 sections");
    cleanup();

    renderWell({
      id: "coverage",
      reading: {
        covered: 6,
        total: 12,
        sections: numbers.map((number) => section(number, false)),
        gaps: numbers,
      },
    });
    link("Coverage 6 of 12 headings covered", "6 headings open in every section");
  });

  it("is the label alone before the trace loads, and current on its page", () => {
    renderWell({ id: "coverage", reading: null }, true);
    const bare = link("Coverage");
    expect(bare.getAttribute("aria-current")).toBe("page");
    expect(bare.hasAttribute("aria-describedby")).toBe(false);
    const well = bare.closest("li")!;
    expect(well.querySelector(".plan-spine__instrument-foot")).toBeNull();
    expect(well.querySelector(".plan-spine__instrument-mark svg")).not.toBeNull();
  });
});

describe("the Findings well", () => {
  it("reads the open count in the warning tone, open pips before decided ones, and its source", () => {
    renderWell({ id: "findings", reading: { open: 3, decided: 4, sources: [{ run: "03-run" }] } });
    const well = link("Findings 3 open", "from the 03-run review · 4 decided").closest("li")!;
    expect(well.querySelector(".plan-spine__instrument-mark")?.getAttribute("data-tone")).toBe(
      "warning",
    );
    expect(well.querySelector(".plan-spine__instrument-figure")?.getAttribute("data-tone")).toBe(
      "warning",
    );
    expect(dots(well)).toEqual([
      ...Array<string>(3).fill("open"),
      ...Array<string>(4).fill("decided"),
    ]);
  });

  it("reads all clear once every finding is decided, with no tone on the well", () => {
    renderWell({
      id: "findings",
      reading: { open: 0, decided: 5, sources: [{ run: "03-run" }, { run: null }] },
    });
    const well = link(
      "Findings all clear",
      "from the 03-run review and your reviews · 5 decided",
    ).closest("li")!;
    expect(well.querySelector("[data-tone]")).toBeNull();
    expect(dots(well)).toEqual(Array<string>(5).fill("decided"));
  });

  it("words the source by the kinds of review that filed, not by the number of groups", () => {
    const wording = (sources: FindingsSource[]) => {
      renderWell({ id: "findings", reading: { open: 1, decided: 0, sources } });
      const description = link("Findings 1 open").getAttribute("aria-describedby")!;
      const text = document.getElementById(description)?.textContent;
      cleanup();
      return text;
    };
    const run = (name: string) => ({ run: name });
    expect(wording([run("03-run")])).toBe("from the 03-run review");
    expect(wording([run("03-run"), run("02-run")])).toBe("from 2 run reviews");
    expect(wording([{ run: null }])).toBe("from your reviews");
    expect(wording([run("03-run"), { run: null }])).toBe("from the 03-run review and your reviews");
    expect(wording([run("03-run"), run("02-run"), { run: null }])).toBe(
      "from 2 run reviews and your reviews",
    );
  });

  it("describes the sources by name, the separators beside the spans", () => {
    renderWell({
      id: "findings",
      reading: {
        open: 1,
        decided: 0,
        sources: [{ run: "03-run" }, { run: "02-run" }, { run: null }],
      },
    });
    link("Findings 1 open", "from 2 run reviews and your reviews");
  });

  it("draws at most 40 pips, the open ones first, and counts the rest in a trailing +N", () => {
    renderWell({
      id: "findings",
      reading: { open: 30, decided: 20, sources: [{ run: "03-run" }] },
    });
    const well = link("Findings 30 open").closest("li")!;
    const drawn = dots(well);
    expect(drawn).toHaveLength(40);
    expect(drawn.filter((dot) => dot === "open")).toHaveLength(30);
    const strip = well.querySelector(".plan-spine__instrument-strip")!;
    expect(strip.lastElementChild?.textContent).toBe("+10");
    expect(strip.getAttribute("aria-hidden")).toBe("true");
    cleanup();

    renderWell({ id: "findings", reading: { open: 45, decided: 5, sources: [{ run: "03-run" }] } });
    const open = link("Findings 45 open").closest("li")!;
    expect(dots(open)).toEqual(Array<string>(40).fill("open"));
    expect(open.querySelector(".plan-spine__instrument-strip")?.lastElementChild?.textContent).toBe(
      "+10",
    );
    cleanup();

    renderWell({
      id: "findings",
      reading: { open: 20, decided: 20, sources: [{ run: "03-run" }] },
    });
    const exact = link("Findings 20 open").closest("li")!;
    expect(dots(exact)).toHaveLength(40);
    expect(exact.querySelector(".plan-spine__instrument-strip")?.textContent).not.toContain("+");
  });

  it("reads an empty ledger as all clear with nothing filed", () => {
    renderWell({ id: "findings", reading: { open: 0, decided: 0, sources: [] } });
    const well = link("Findings all clear", "no run review has filed any yet").closest("li")!;
    expect(dots(well)).toEqual([]);
  });
});
