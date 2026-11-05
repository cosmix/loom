import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import css from "./roundel-glyph.css?raw";
import { RoundelGlyph } from "./roundel-glyph";

afterEach(cleanup);

describe("RoundelGlyph", () => {
  it("draws the turning roundel inside an aria-hidden root with no announcement of its own", () => {
    const { container } = render(<RoundelGlyph />);

    const root = container.firstElementChild!;
    expect(root.classList.contains("plan-roundel-glyph")).toBe(true);
    expect(root.getAttribute("aria-hidden")).toBe("true");
    expect(root.querySelector(".aurora-roundel")?.getAttribute("data-busy")).toBe("true");
    expect(container.querySelector("svg")).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
    expect(document.body.textContent).toBe("");
  });

  it("rests when not busy", () => {
    const { container } = render(<RoundelGlyph busy={false} />);

    expect(container.querySelector(".aurora-roundel")?.hasAttribute("data-busy")).toBe(false);
  });

  it("takes the props a glyph site passes an icon: a class, a size and data attributes", () => {
    const { container } = render(
      <RoundelGlyph className="size-4 tone-completed" size="1.05em" data-kind="running" />,
    );

    const root = container.firstElementChild as HTMLElement;
    expect(root.className).toBe("plan-roundel-glyph size-4 tone-completed");
    expect(root.style.width).toBe("1.05em");
    expect(root.style.height).toBe("1.05em");
    expect(root.getAttribute("data-kind")).toBe("running");
  });

  it("sizes the box at zero specificity and the disc to the glyph weight", () => {
    expect(css).toMatch(
      /:where\(\.plan-roundel-glyph\)\s*\{[^}]*width:\s*14px;[^}]*height:\s*14px;/,
    );
    expect(css).toMatch(
      /\.plan-roundel-glyph > \.aurora-roundel-wrap\s*\{[^}]*width:\s*86%;[^}]*height:\s*86%;/,
    );
  });
});
