import { describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type AgentFlow, type TokenFlow } from "@/api/schema";
import { modelHistoryBars } from "@/lib/model-history";

type Agent = Pick<AgentFlow, "model" | "series">;

/// A one-stage flow whose agents carry `series`; the stage series is their per-bucket sum.
function flowOf(agents: Agent[], bucketSecs = 30): TokenFlow {
  const flow = snapshotSchema.parse(fixture).status.tokens!;
  const stage = flow.stages[0]!;
  const template = stage.agents[0]!;
  const length = Math.max(0, ...agents.map((agent) => agent.series.length));
  stage.agents = agents.map((agent, index) => ({ ...template, id: `a${index}`, ...agent }));
  stage.series = Array.from({ length }, (_, i) =>
    agents.reduce((sum, agent) => sum + (agent.series[i - (length - agent.series.length)] ?? 0), 0),
  );
  flow.stages = [stage];
  flow.bucket_secs = bucketSecs;
  flow.observed_at = 1000 * bucketSecs + 7;
  return flow;
}

describe("modelHistoryBars", () => {
  it("sums to the stage series per bucket, scaled to per minute", () => {
    const flow = flowOf([
      { model: "a", series: [10, 0, 30, 5] },
      { model: "b", series: [2, 4, 0, 5] },
    ]);
    const bars = modelHistoryBars(flow);
    expect(bars).toHaveLength(60);
    const stage = flow.stages[0]!;
    bars.slice(-4).forEach((bar, i) => {
      const total = Object.values(bar.rates).reduce((sum, rate) => sum + rate, 0);
      expect(total).toBeCloseTo((stage.series[i]! * 60) / 30);
    });
    expect(bars.at(-4)?.rates).toEqual({ a: 20, b: 4 });
    expect(bars.at(-3)?.rates).toEqual({ b: 8 });
    expect(bars.slice(0, 56).every((bar) => Object.keys(bar.rates).length === 0)).toBe(true);
  });

  it("left-pads a short series so its last value lands in the newest bar", () => {
    const bars = modelHistoryBars(flowOf([{ model: "a", series: [3] }]));
    expect(bars[59]?.rates.a).toBe(6);
    expect(bars[58]?.rates.a).toBeUndefined();
  });

  it("falls back to 30 s buckets when bucket_secs is not positive", () => {
    const bars = modelHistoryBars(flowOf([{ model: "a", series: [10] }], 0));
    expect(bars[59]?.rates.a).toBe(20);
    expect(bars[59]!.at - bars[58]!.at).toBe(30);
  });

  it("books a null model under unknown and stamps bars by bucket start", () => {
    const flow = flowOf([{ model: null, series: [1] }]);
    const bars = modelHistoryBars(flow);
    expect(bars[59]?.rates).toEqual({ unknown: 2 });
    expect(bars[59]?.at).toBe(1000 * 30);
    expect(bars[0]?.at).toBe(941 * 30);
  });
});
