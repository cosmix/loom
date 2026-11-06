import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import tokensCss from "@/aurora-ui/shared/styles/tokens.css?raw";
import { ActivityGlyph } from "./activity-glyphs";
import { BusyText } from "./busy-text";
import { LoadState } from "./load-state";

afterEach(cleanup);

/** The `--primary` value of every innermost rule block whose selector names the Graphite theme. */
function graphitePrimaries(css: string): string[] {
  const blocks = [...css.matchAll(/([^{}]*)\{([^{}]*)\}/g)];
  return blocks
    .filter(
      ([, selector, body]) =>
        selector!.includes('[data-dark-theme="gray"]') && /--primary\s*:/.test(body!),
    )
    .map(([, , body]) => /--primary\s*:\s*([^;]+);/.exec(body!)![1]!.trim());
}

it("activity glyphs draw the status dot, busy text the arrow, loading the spinner, never the roundel", () => {
  const glyph = render(<ActivityGlyph kind="summaries" />);
  expect(glyph.container.querySelector(".activity-glyph__led")).not.toBeNull();
  expect(glyph.container.querySelector(".aurora-roundel")).toBeNull();
  cleanup();
  const busy = render(<BusyText>Sending…</BusyText>);
  expect(busy.container.querySelector("svg.plan-arrow-spinner")).not.toBeNull();
  cleanup();
  const loading = render(<LoadState state="loading" title="Loading the feature" />);
  expect(loading.container.querySelector(".aurora-roundel")).toBeNull();
  expect(loading.container.querySelector(".plan-spinner")).not.toBeNull();
  expect(screen.getByRole("status").getAttribute("aria-label")).toBe("Loading the feature");
});

it("the Graphite accent is a bright colour", () => {
  const primaries = graphitePrimaries(tokensCss);
  expect(primaries.length).toBeGreaterThan(0);
  for (const value of primaries) {
    const match = /^oklch\(\s*([\d.]+%?)\s+([\d.]+)\s+([\d.]+)/.exec(value);
    expect(match, `--primary ${value} is not oklch(L C H)`).not.toBeNull();
    expect(Number(match![2])).toBeGreaterThanOrEqual(0.12);
  }
});
