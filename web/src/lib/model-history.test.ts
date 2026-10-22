import { describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type AgentFlow, type TokenFlow } from "@/api/schema";
import { modelHistoryBars } from "@/lib/model-history";

type Agent = Pick<AgentFlow, "model" | "series">;

/// The newest window at `observed_at` 7 s into its bucket: nine whole buckets plus 8 s.
const NEWEST_SPAN = 9 * 30 + 7 + 1;

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
  it("draws each bar as the five-minute rate ending at it, per model", () => {
    const bars = modelHistoryBars(
      flowOf([
        { model: "a", series: [10, 0, 30, 5] },
        { model: "b", series: [2, 4, 0, 5] },
      ]),
    );
    expect(bars).toHaveLength(60);
    expect(bars[56]?.rates.a).toBeCloseTo((10 * 60) / 300);
    expect(bars[56]?.rates.b).toBeCloseTo((2 * 60) / 300);
    expect(bars[57]?.rates.a).toBeCloseTo((10 * 60) / 300);
    expect(bars[58]?.rates.a).toBeCloseTo((40 * 60) / 300);
    expect(bars[58]?.rates.b).toBeCloseTo((6 * 60) / 300);
    expect(bars.slice(0, 56).every((bar) => Object.keys(bar.rates).length === 0)).toBe(true);
  });

  it("ends the newest window at observed_at, as the backend rate does", () => {
    const bars = modelHistoryBars(
      flowOf([
        { model: "a", series: [10, 0, 30, 5] },
        { model: "b", series: [2, 4, 0, 5] },
      ]),
    );
    expect(bars[59]?.rates.a).toBeCloseTo((45 * 60) / NEWEST_SPAN);
    expect(bars[59]?.rates.b).toBeCloseTo((11 * 60) / NEWEST_SPAN);
  });

  it("feeds the oldest bar from buckets before it and drops a bucket ten bars on", () => {
    const series = [300, ...Array<number>(68).fill(0)];
    const bars = modelHistoryBars(flowOf([{ model: "a", series }]));
    expect(bars[0]?.rates.a).toBeCloseTo(60);
    expect(bars[1]?.rates.a).toBeUndefined();
  });

  it("does not spike on one busy bucket", () => {
    const bars = modelHistoryBars(
      flowOf([{ model: "a", series: [0, 50000, ...Array(10).fill(0)] }]),
    );
    const peak = Math.max(...bars.map((bar) => bar.rates.a ?? 0));
    expect(peak).toBeCloseTo((50000 * 60) / 300);
  });

  it("left-pads a short series so its last value lands in the newest bar", () => {
    const bars = modelHistoryBars(flowOf([{ model: "a", series: [3] }]));
    expect(bars[59]?.rates.a).toBeCloseTo((3 * 60) / NEWEST_SPAN);
    expect(bars[58]?.rates.a).toBeUndefined();
  });

  it("falls back to 30 s buckets when bucket_secs is not positive", () => {
    const bars = modelHistoryBars(flowOf([{ model: "a", series: [10] }], 0));
    expect(bars[59]?.rates.a).toBeCloseTo((10 * 60) / NEWEST_SPAN);
    expect(bars[59]!.at - bars[58]!.at).toBe(30);
  });

  it("books a null model under unknown and stamps bars by bucket start", () => {
    const bars = modelHistoryBars(flowOf([{ model: null, series: [1] }]));
    expect(Object.keys(bars[59]!.rates)).toEqual(["unknown"]);
    expect(bars[59]?.at).toBe(1000 * 30);
    expect(bars[0]?.at).toBe(941 * 30);
  });
});
