import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { TokenLine } from "@/components/header-lines";
import { TokenFlowPanel } from "@/components/token-flow-panel";
import { applySnapshot } from "@/state/apply";

function renderWith(raw: unknown, ui: React.ReactNode) {
  const store = createStore();
  applySnapshot(store, snapshotSchema.parse(raw));
  return render(<Provider store={store}>{ui}</Provider>);
}

function withoutTokens(): unknown {
  const raw = structuredClone(fixture) as unknown as { status: Record<string, unknown> };
  delete raw.status.tokens;
  return raw;
}

afterEach(cleanup);

describe("TokenFlowPanel", () => {
  it("shows the plan rate, the mix and the per-model rates from the fixture", () => {
    renderWith(fixture, <TokenFlowPanel />);

    const panel = screen.getByRole("region", { name: "Token flow" });
    expect(panel.textContent).toContain("43.2k");
    expect(panel.textContent).toContain("5 min avg");
    for (const label of [
      "uncached input",
      "cache write",
      "output",
      "cache read",
      "cache hit (5 min)",
    ]) {
      expect(within(panel).getByText(label)).toBeTruthy();
    }
    expect(within(panel).getByText("94%")).toBeTruthy();
    expect(within(panel).getByText("claude-opus-5-5")).toBeTruthy();
    expect(within(panel).getByText("server")).toBeTruthy();
    for (const label of ["−30m", "−15m", "now"]) {
      expect(panel.textContent).toContain(label);
    }
  });

  it("keeps the raw input row apart from the fresh headline", () => {
    renderWith(fixture, <TokenFlowPanel />);

    const panel = screen.getByRole("region", { name: "Token flow" });
    expect(within(panel).queryByText("fresh input")).toBeNull();
    expect(within(panel).getByText("fresh / min")).toBeTruthy();
  });

  it("captions the chart axis as per 30 s buckets", () => {
    renderWith(fixture, <TokenFlowPanel />);

    const panel = screen.getByRole("region", { name: "Token flow" });
    expect(within(panel).getByText("per 30 s")).toBeTruthy();
  });

  it("draws the chart at a fixed pixel height with HTML axis labels", () => {
    renderWith(fixture, <TokenFlowPanel />);

    const panel = screen.getByRole("region", { name: "Token flow" });
    const chart = within(panel).getByRole("img", { name: "fresh tokens per stage, last 30 min" });
    expect(chart.tagName.toLowerCase()).toBe("svg");
    expect(chart.getAttribute("height")).toBe("96");
    expect(chart.getAttribute("preserveAspectRatio")).toBe("none");
    expect(chart.querySelectorAll("text")).toHaveLength(0);
    for (const polygon of Array.from(chart.querySelectorAll("polygon"))) {
      expect(polygon.getAttribute("vector-effect")).toBe("non-scaling-stroke");
    }

    for (const label of ["per 30 s", "−30m", "−15m", "now", "21.6k"]) {
      const matches = within(panel).getAllByText(label);
      expect(matches.length).toBeGreaterThan(0);
      for (const match of matches) expect(match.closest("svg")).toBeNull();
    }
  });

  it("renders nothing without token data", () => {
    renderWith(withoutTokens(), <TokenFlowPanel />);
    expect(screen.queryByRole("region", { name: "Token flow" })).toBeNull();
  });
});

describe("TokenLine", () => {
  it("shows the plan rate, total and cache hit", () => {
    renderWith(fixture, <TokenLine />);

    const rail = screen.getByRole("region", { name: "tokens" });
    expect(rail.textContent).toContain("43.2k");
    expect(rail.textContent).toContain("2.23M");
    expect(rail.textContent).toContain("93%");
  });

  it("is absent without token data", () => {
    renderWith(withoutTokens(), <TokenLine />);
    expect(screen.queryByRole("region", { name: "tokens" })).toBeNull();
  });
});
