import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import css from "./spinner.css?raw";
import { ArrowSpinner, Spinner } from "./spinner";

afterEach(cleanup);

/** The `@media (prefers-reduced-motion: reduce)` block's body. */
function reducedMotion(): string {
  const start = css.indexOf("@media (prefers-reduced-motion: reduce)");
  expect(start).toBeGreaterThan(-1);
  return css.slice(start);
}

describe("Spinner", () => {
  it("is a decorative ring and arc in currentColor, hidden from assistive technology", () => {
    const { container } = render(<Spinner />);
    const svg = container.querySelector("svg")!;
    expect(svg.classList.contains("plan-spinner")).toBe(true);
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.getAttribute("focusable")).toBe("false");
    expect(svg.querySelector(".plan-spinner__track")).not.toBeNull();
    expect(svg.querySelector(".plan-spinner__arc")).not.toBeNull();
    expect(container.textContent).toBe("");
    expect(css).toMatch(/stroke:\s*currentColor/);
  });

  it("is the 20px block glyph", () => {
    expect(css).toMatch(/\.plan-spinner\s*\{[^}]*width:\s*20px;[^}]*height:\s*20px/);
  });

  it("turns, and under reduced motion stops turning and only eases its opacity", () => {
    expect(css).toMatch(/\.plan-spinner\s*\{[^}]*animation:\s*plan-spinner-turn/);
    const reduced = reducedMotion();
    expect(reduced).toMatch(/\.plan-spinner\s*\{\s*animation:\s*none/);
    expect(reduced).toMatch(/\.plan-spinner__arc\s*\{[^}]*animation:\s*plan-spinner-rest/);
    expect(css).toMatch(/@keyframes plan-spinner-rest\s*\{[^@]*opacity/);
  });
});

describe("ArrowSpinner", () => {
  it("is a lucide-weight circular arrow in currentColor, always hidden from assistive technology", () => {
    const { container } = render(<ArrowSpinner aria-hidden="false" data-kind="summaries" />);
    const svg = container.querySelector("svg")!;
    expect(svg.classList.contains("plan-arrow-spinner")).toBe(true);
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.getAttribute("stroke")).toBe("currentColor");
    expect(svg.getAttribute("stroke-width")).toBe("2");
    expect(svg.getAttribute("data-kind")).toBe("summaries");
    expect(svg.querySelectorAll("path")).toHaveLength(2);
    expect(svg.getAttribute("data-busy")).toBe("true");
  });

  it("takes a glyph site's class and size, and rests when not busy", () => {
    const { container } = render(<ArrowSpinner className="size-4" size={12} busy={false} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("class")).toBe("plan-arrow-spinner size-4");
    expect(svg.style.width).toBe("12px");
    expect(svg.style.height).toBe("12px");
    expect(svg.getAttribute("data-busy")).toBe("false");
  });

  it("turns only while busy, defaults to the 14px glyph box in the components layer, and stops under reduced motion", () => {
    expect(css).toMatch(
      /\.plan-arrow-spinner\[data-busy="true"\]\s*\{\s*animation:\s*plan-spinner-turn/,
    );
    expect(css).toMatch(
      /@layer components\s*\{\s*:where\(\.plan-arrow-spinner\)\s*\{\s*width:\s*14px/,
    );
    expect(reducedMotion()).toMatch(
      /\.plan-arrow-spinner\[data-busy="true"\]\s*\{\s*animation:\s*none/,
    );
  });
});
