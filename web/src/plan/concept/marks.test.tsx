import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import type { Mark } from "@/plan/api/schema-concept";

import { docColour, Marks } from "./marks";

const markOf = (colour: number): Mark => ({
  doc: { id: "vision", title: "Vision", path: "doc/product/vision.md", page: "product" },
  colour,
  out_of_date: null,
});

afterEach(cleanup);

describe("docColour", () => {
  it("draws colour 0, the server's no-colour value, in the muted foreground", () => {
    expect(docColour(0)).toBe("var(--muted-foreground)");
  });

  it.each([
    [1, "var(--doc-1)"],
    [8, "var(--doc-8)"],
    [9, "var(--doc-1)"],
    [16, "var(--doc-8)"],
  ])("colour %i is %s", (colour, expected) => {
    expect(docColour(colour)).toBe(expected);
  });
});

describe("Marks", () => {
  it("a colour 0 mark is muted, not the eighth document colour", () => {
    render(
      <TooltipProvider>
        <Marks marks={[markOf(0), markOf(3)]} onOpen={() => {}} />
      </TooltipProvider>,
    );
    const [none, third] = screen.getAllByRole("button");
    expect(none!.style.background).toContain("--muted-foreground");
    expect(none!.style.background).not.toContain("--doc-8");
    expect(third!.style.background).toContain("--doc-3");
  });

  it("shows a format character in the title as its code point in the accessible name", () => {
    const mark = markOf(2);
    render(
      <TooltipProvider>
        <Marks marks={[{ ...mark, doc: { ...mark.doc, title: "Vis‮ion" } }]} onOpen={() => {}} />
      </TooltipProvider>,
    );
    const name = screen.getByRole("button").getAttribute("aria-label");
    expect(name).toBe("Vis⟨U+202E⟩ion: doc/product/vision.md");
    expect(name).not.toContain("‮");
  });
});
