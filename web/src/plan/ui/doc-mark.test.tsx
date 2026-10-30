import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { DOC_KIND_COLOUR, DocMark } from "./doc-mark";

afterEach(cleanup);

describe("DocMark", () => {
  it.each([
    ["sm", "size-5", "size-3"],
    ["lg", "size-[30px]", "size-4"],
  ] as const)("renders the %s square and glyph sizes", (size, squareClass, glyphClass) => {
    const { container } = render(<DocMark kind="vision" size={size} />);
    const mark = container.firstElementChild;

    expect(mark?.classList.contains(squareClass)).toBe(true);
    expect(mark?.classList.contains(size === "lg" ? "rounded-[8px]" : "rounded-[5px]")).toBe(true);
    expect(mark?.querySelector("svg")?.classList.contains(glyphClass)).toBe(true);
  });

  it("defaults to the small size", () => {
    const { container } = render(<DocMark kind="plan" />);

    expect(container.firstElementChild?.classList.contains("size-5")).toBe(true);
    expect(container.querySelector("svg")?.classList.contains("size-3")).toBe(true);
  });

  it.each([
    ["vision", 1],
    ["section", 2],
    ["note", 3],
    ["metaplan", 4],
    ["plan", 5],
    ["brief", 6],
    ["knowledge", 7],
    ["review", 8],
  ] as const)("maps %s to document colour %i", (kind, index) => {
    const { container } = render(<DocMark kind={kind} />);
    const mark = container.firstElementChild as HTMLElement;
    const colour = `var(--doc-${index})`;

    expect(DOC_KIND_COLOUR[kind]).toBe(index);
    expect(mark.style.color).toBe(colour);
    expect(mark.style.getPropertyValue("--doc-mark")).toBe(colour);
    expect(
      mark.classList.contains("bg-[color-mix(in_oklch,var(--doc-mark)_16%,transparent)]"),
    ).toBe(true);
    expect(
      mark.classList.contains("dark:bg-[color-mix(in_oklch,var(--doc-mark)_20%,transparent)]"),
    ).toBe(true);
  });

  it.each([
    ["sm", "size-3"],
    ["lg", "size-4"],
  ] as const)(
    "swaps the %s kind glyph for the summaries activity while rebuilding",
    (size, glyphClass) => {
      const { container, rerender } = render(<DocMark kind="vision" size={size} />);
      const kindGlyph = container.querySelector("svg");
      const kindDrawing = kindGlyph?.innerHTML;

      rerender(<DocMark kind="vision" size={size} rebuilding />);

      const activity = container.querySelector('[data-kind="summaries"]');
      expect(activity).not.toBeNull();
      expect(activity?.classList.contains(glyphClass)).toBe(true);
      expect(container.querySelectorAll("svg")).toHaveLength(1);
      expect(container.contains(kindGlyph)).toBe(false);

      rerender(<DocMark kind="vision" size={size} />);

      expect(container.querySelector(".lucide-refresh-cw")).toBeNull();
      expect(container.querySelector("svg")?.innerHTML).toBe(kindDrawing);
    },
  );

  it.each([false, true])(
    "hides the whole mark from assistive technology when rebuilding=%s",
    (rebuilding) => {
      const { container } = render(<DocMark kind="review" rebuilding={rebuilding} />);

      expect(container.firstElementChild?.getAttribute("aria-hidden")).toBe("true");
    },
  );

  it("preserves a supplied class name", () => {
    const { container } = render(<DocMark kind="note" className="align-middle" />);

    expect(container.firstElementChild?.classList.contains("align-middle")).toBe(true);
  });
});
