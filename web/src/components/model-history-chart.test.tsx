import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { ModelHistoryChart } from "@/components/model-history-chart";

afterEach(cleanup);

const NAME = "Fresh tokens per minute by model, stacked bars, last 30 min";

function flow() {
  return snapshotSchema.parse(fixture).status.tokens!;
}

describe("model history chart", () => {
  it("stacks colored models bottom-up, then uncolored ones dimmed", () => {
    const tokens = flow();
    const agents = tokens.stages[0]!.agents;
    const colors = new Map([[agents[0]!.model!, "red"]]);
    render(<ModelHistoryChart flow={tokens} colors={colors} />);
    const chart = screen.getByRole("img", { name: NAME });
    expect(chart.querySelectorAll("[data-time]")).toHaveLength(60);
    expect(screen.queryByText("No fresh tokens in the last 30 min")).toBeNull();
    const fills = new Set<string | null>();
    for (const bar of chart.querySelectorAll("[data-time]")) {
      let below = 100;
      const rects = [...bar.querySelectorAll("rect")];
      rects.forEach((rect, index) => {
        if (index === 0) expect(rect.getAttribute("fill")).toBe("red");
        fills.add(rect.getAttribute("fill"));
        const y = Number(rect.getAttribute("y"));
        // Segments above the first leave a 1.5-unit gap below them.
        const height = Number(rect.getAttribute("height"));
        if (height > 0) expect(y + height).toBeCloseTo(below - (index === 0 ? 0 : 1.5));
        below = y;
      });
      expect(below).toBeGreaterThanOrEqual(-0.0001);
    }
    expect(fills.has("var(--tone-dimmed)")).toBe(true);
  });

  it("reads 0 min ago on the newest bar and says so when no bar has tokens", () => {
    const tokens = flow();
    render(<ModelHistoryChart flow={tokens} colors={new Map()} />);
    const bars = screen.getByRole("img", { name: NAME }).querySelectorAll("[data-time]");
    expect(bars[59]?.querySelector("title")?.textContent).toMatch(/^0 min ago/);
    cleanup();
    for (const stage of tokens.stages) for (const agent of stage.agents) agent.series = [];
    render(<ModelHistoryChart flow={tokens} colors={new Map()} />);
    expect(screen.getByText("No fresh tokens in the last 30 min")).toBeTruthy();
    expect(screen.getByRole("img", { name: NAME }).querySelectorAll("rect")).toHaveLength(0);
  });
});
