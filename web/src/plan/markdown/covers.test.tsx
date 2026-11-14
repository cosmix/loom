import { cleanup, fireEvent, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { CoverLookup, CoverSection } from "@/plan/markdown/covers";
import { parseCovers } from "@/plan/markdown/covers";
import { linkNamed, PATH, renderMarkdown } from "@/plan/markdown/render-test-utils";

const S01 = "doc/product/f/section_01.md";
const S02 = "doc/product/f/section_02.md";

function sectionOf(
  path: string,
  title: string,
  headings: Record<string, string>,
  complete = true,
): CoverSection {
  return { path, title, headings: new Map(Object.entries(headings)), complete };
}

function lookupOf(sections: Record<number, CoverSection>): CoverLookup {
  return (number) => sections[number] ?? null;
}

const LOOKUP = lookupOf({
  1: sectionOf(S01, "Studio", { "changes-to-the-studio-spec": "Changes to the studio spec" }),
  2: sectionOf(S02, "Asset library", {
    "the-five-kinds": "The five kinds",
    "archive-formats": "Archive formats",
  }),
});

function renderCovers(source: string, covers: CoverLookup | null = LOOKUP) {
  return renderMarkdown(source, { covers, docPaths: new Set([PATH, S01, S02]) });
}

afterEach(cleanup);

describe("Covers lines", () => {
  it("draws each ref as a link to its heading, titled and grouped by section", () => {
    const line =
      "Covers: section_02#the-five-kinds, section_02#archive-formats, section_01#changes-to-the-studio-spec";
    const { container, onNavigate } = renderCovers(`## 1. Entry\n\n${line}\n`);
    const block = container.querySelector(".md-covers");
    expect(block).not.toBeNull();
    expect(block?.getAttribute("role")).toBe("group");
    expect(container.textContent).not.toContain("section_02#");

    const groups = Array.from(container.querySelectorAll(".md-covers__group"));
    expect(groups.map((g) => g.querySelector(".md-covers__number")?.textContent)).toEqual([
      "01",
      "02",
    ]);
    const second = within(groups[1] as HTMLElement);
    expect(second.getByRole("link", { name: "Asset library" }).getAttribute("href")).toBe(
      "/files/product/f/section_02.md",
    );
    expect(second.getAllByRole("listitem").map((item) => item.textContent?.trim())).toEqual([
      "The five kinds ·",
      "Archive formats",
    ]);

    const kinds = linkNamed(container, "The five kinds");
    expect(kinds?.getAttribute("href")).toBe("/files/product/f/section_02.md#the-five-kinds");
    fireEvent.click(kinds as HTMLAnchorElement);
    expect(onNavigate).toHaveBeenCalledWith("/files/product/f/section_02.md#the-five-kinds");
    expect(linkNamed(container, "Changes to the studio spec")?.getAttribute("href")).toBe(
      "/files/product/f/section_01.md#changes-to-the-studio-spec",
    );
  });

  it("names a section covered whole", () => {
    const { container } = renderCovers("Covers: section_01, section_02#the-five-kinds\n");
    const first = container.querySelector(".md-covers__group") as HTMLElement;
    expect(within(first).getByRole("link", { name: "Studio" })).toBeTruthy();
    expect(first.querySelector('[data-state="whole"]')?.textContent).toContain("Whole section");
  });

  it("shows a ref with no matching heading as its raw words, muted, never a link", () => {
    const source = "Covers: section_02#gone, section_09#x, vision#why, section_02#the-five-kinds\n";
    const { container } = renderCovers(source);
    const missing = Array.from(container.querySelectorAll('[data-state="missing"]'));
    expect(missing.map((item) => item.textContent?.replace(/ ·/, "").trim())).toEqual([
      "gone",
      "section_09#x",
      "vision#why",
    ]);
    for (const item of missing) expect(item.querySelector("a")).toBeNull();
    expect(missing[0]?.querySelector(".md-covers__raw")?.getAttribute("title")).toBe(
      "section_02#gone: not found in the spec",
    );
    expect(container.textContent).toContain("No section 09 in the spec");
    expect(container.textContent).toContain("Not a spec section");
    expect(container.querySelectorAll(".md-covers a")).toHaveLength(2);
  });

  it("holds a heading the section has not finished loading as pending, not missing", () => {
    const loading = lookupOf({ 2: sectionOf(S02, "Asset library", {}, false) });
    const { container } = renderCovers("Covers: section_02#unpacking-rules\n", loading);
    const pending = container.querySelector('[data-state="pending"]');
    expect(pending?.textContent?.trim()).toBe("unpacking-rules");
    expect(pending?.querySelector("[title]")).toBeNull();
  });

  it("leaves every other paragraph, and every document without a lookup, as prose", () => {
    const source = [
      "Text before. Covers: section_02#the-five-kinds",
      "",
      "- Covers: section_02#the-five-kinds",
      "",
      "> Covers: section_02#the-five-kinds",
      "",
      "```text",
      "Covers: section_02#the-five-kinds",
      "```",
      "",
      "Covers: section_02#the-five-kinds",
      "and a second line",
      "",
    ].join("\n");
    const { container } = renderCovers(source);
    expect(container.querySelector(".md-covers")).toBeNull();
    expect(container.querySelectorAll("p")).toHaveLength(3);

    cleanup();
    const plain = renderCovers("Covers: section_02#the-five-kinds\n", null);
    expect(plain.container.querySelector(".md-covers")).toBeNull();
    expect(plain.container.querySelector("p")?.textContent).toBe(
      "Covers: section_02#the-five-kinds",
    );
  });
});

describe("parseCovers", () => {
  it("reads section tokens as the trace does and keeps the rest as written", () => {
    expect(
      parseCovers(
        " section_04.md#a, section_4#a, [s](../product/section_05.md#b), section_06, x#Bad, , ",
      ),
    ).toEqual([
      { raw: "section_04.md#a", section: 4, anchor: "a" },
      { raw: "[s](../product/section_05.md#b)", section: 5, anchor: "b" },
      { raw: "section_06", section: 6, anchor: null },
      { raw: "x#Bad", section: null, anchor: null },
    ]);
    expect(parseCovers("section_02#Upper")).toEqual([
      { raw: "section_02#Upper", section: null, anchor: null },
    ]);
  });
});
