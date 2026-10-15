import { expect, it } from "vitest";

import type { AgentFlow, ModelUsage, StageFlow, TokenFlow, TokenUsage } from "@/api/schema";
import {
  agentLabel,
  cacheHitPercent,
  carryForward,
  compactTokens,
  cumulativeSeries,
  fresh,
  lastValues,
  liveCounts,
  maxOf,
  modelGroups,
  modelRates,
  modelShares,
  padLeft,
  quotaLeftText,
  rollingRate,
  stageFlow,
  wholePercents,
} from "@/lib/tokens";

function usage(input: number, cache_write: number, cache_read: number, output: number): TokenUsage {
  return { input, cache_write, cache_read, output };
}

const main: AgentFlow = {
  id: "main-01ed9f4c",
  kind: "main",
  agent_type: null,
  model: "claude-opus-5-5",
  active: true,
  total: usage(40000, 600000, 9000000, 100000),
  rate: usage(600, 12000, 200000, 1600),
  series: Array(120).fill(7100),
};
const subagent: AgentFlow = {
  id: "a04baa183f888b773",
  kind: "subagent",
  agent_type: "loom-software-engineer",
  model: "claude-sonnet-5-5",
  active: true,
  total: usage(50000, 900000, 13000000, 160000),
  rate: usage(1200, 21000, 350000, 3000),
  series: Array(120).fill(12600),
};
const codex: AgentFlow = {
  id: "codex-01a1396b",
  kind: "codex",
  agent_type: null,
  model: "gpt-6.1-sol",
  active: true,
  total: usage(30000, 300000, 4000000, 50000),
  rate: usage(200, 3000, 50000, 600),
  series: Array(120).fill(1900),
};
const server: StageFlow = {
  stage_id: "server",
  total: usage(120000, 1800000, 26000000, 310000),
  rate: usage(2000, 36000, 600000, 5200),
  series: Array(120).fill(21600),
  hit_series: Array(120).fill(94),
  agents: [main, subagent, codex],
};
const flow: TokenFlow = {
  observed_at: 1788523200,
  bucket_secs: 30,
  plan: { total: server.total, rate: server.rate, series: Array(60).fill(21600) },
  stages: [server],
  models: [],
  quota_left: [{ provider: "claude", minutes_left: 127, lasts_to_reset: false }],
  unreadable_lines: 0,
};

it("fresh excludes cache reads from plan totals and rates", () => {
  expect([fresh(flow.plan.total), fresh(flow.plan.rate)]).toEqual([2230000, 43200]);
});

it.each([
  [0, "0"],
  [980, "980"],
  [999, "999"],
  [1000, "1.0k"],
  [43200, "43.2k"],
  [99999, "99.9k"],
  [100000, "100k"],
  [312000, "312k"],
  [999999, "999k"],
  [1000000, "1.00M"],
  [2230000, "2.23M"],
  [2239999, "2.23M"],
])("compactTokens truncates %i to %s", (value, expected) => {
  expect(compactTokens(value)).toBe(expected);
});

it("cacheHitPercent gives the pinned plan percentage", () => {
  expect(cacheHitPercent(flow.plan.total)?.toFixed(1)).toBe("93.1");
});

it("cacheHitPercent is undefined when only output tokens exist", () => {
  expect(cacheHitPercent(usage(0, 0, 0, 100))).toBeNull();
});

it.each([
  [127, "~2h07m left", "warning"],
  [60, "~1h00m left", "warning"],
  [43, "~43m left", "warning"],
  [30, "~30m left", "warning"],
  [29, "~29m left", "blocked"],
  [20, "~20m left", "blocked"],
  [0, "~0m left", "blocked"],
])("quotaLeftText formats %i minutes", (minutes_left, text, tone) => {
  expect(quotaLeftText({ provider: "claude", minutes_left, lasts_to_reset: false })).toEqual({
    text,
    tone,
  });
});

it.each([null, 20])(
  "quotaLeftText prioritizes lasting to reset over %s minutes",
  (minutes_left) => {
    expect(quotaLeftText({ provider: "claude", minutes_left, lasts_to_reset: true })).toEqual({
      text: "lasts to reset",
      tone: "dimmed",
    });
  },
);

it("quotaLeftText omits unavailable estimates", () => {
  expect(
    quotaLeftText({ provider: "codex", minutes_left: null, lasts_to_reset: false }),
  ).toBeNull();
});

it.each([
  [main, "main session"],
  [subagent, "loom-software-engineer"],
  [codex, "codex job"],
  [{ ...subagent, agent_type: null }, "subagent"],
])("agentLabel labels $0.kind as %s", (agent, expected) => {
  expect(agentLabel(agent)).toBe(expected);
});

it("modelRates sorts fixture models by fresh rate", () => {
  expect(modelRates(flow)).toEqual([
    { model: "claude-sonnet-5-5", perMinute: 25200 },
    { model: "claude-opus-5-5", perMinute: 14200 },
    { model: "gpt-6.1-sol", perMinute: 3800 },
  ]);
});

it("modelRates sums repeated models and unknown models across stages including idle agents", () => {
  const stages = [
    { ...server, agents: [main, { ...codex, model: null }] },
    {
      ...server,
      stage_id: "client",
      agents: [
        { ...main, active: false },
        { ...subagent, model: null },
      ],
    },
  ];

  expect(modelRates({ ...flow, stages })).toEqual([
    { model: "unknown", perMinute: 29000 },
    { model: "claude-opus-5-5", perMinute: 28400 },
  ]);
});

it("modelRates returns no rates for an empty flow", () => {
  expect(modelRates({ ...flow, stages: [] })).toEqual([]);
});

it("stageFlow returns the matching stage", () => {
  expect(stageFlow(flow, "server")).toBe(server);
});

it("stageFlow returns undefined for an unknown stage", () => {
  expect(stageFlow(flow, "unknown")).toBeUndefined();
});

it("stageFlow returns undefined when flow is absent", () => {
  expect(stageFlow(undefined, "server")).toBeUndefined();
});

it("maxOf returns the largest value, never below the floor", () => {
  expect(maxOf([3, 9, 4])).toBe(9);
  expect(maxOf([])).toBe(0);
  expect(maxOf([2, 3], 10)).toBe(10);
});

it("maxOf handles a series too long to spread into arguments", () => {
  const long = Array<number>(200_000).fill(1);
  long[150_000] = 7;

  expect(maxOf(long)).toBe(7);
});

it("padLeft pads short series with zeros and returns a long one whole", () => {
  expect(padLeft([1, 2], 4)).toEqual([0, 0, 1, 2]);
  expect(padLeft([1, 2, 3], 2)).toEqual([1, 2, 3]);
  expect(padLeft([], 2)).toEqual([0, 0]);
});

it("lastValues keeps the last N values and pads a short series on the left", () => {
  expect(lastValues([1, 2, 3, 4, 5], 3)).toEqual([3, 4, 5]);
  expect(lastValues([5, 6], 4)).toEqual([0, 0, 5, 6]);
  expect(lastValues([], 3)).toEqual([0, 0, 0]);
});

const models: ModelUsage[] = [
  { model: "claude-sonnet-5-5", effort: "medium", total: usage(50000, 900000, 13000000, 160000) },
  { model: "claude-opus-5-5", effort: "xhigh", total: usage(30000, 450000, 7000000, 80000) },
  { model: "gpt-6.1-sol", effort: "xhigh", total: usage(30000, 300000, 4000000, 50000) },
  { model: "claude-opus-5-5", effort: "high", total: usage(10000, 150000, 2000000, 20000) },
];

it("modelShares sums efforts per model and orders by fresh", () => {
  const shares = modelShares(models);

  expect(shares.map((s) => [s.model, s.fresh])).toEqual([
    ["claude-sonnet-5-5", 1110000],
    ["claude-opus-5-5", 740000],
    ["gpt-6.1-sol", 380000],
  ]);
  expect(shares[0].share).toBeCloseTo(0.4978, 4);
  expect(shares[1].share).toBeCloseTo(0.3318, 4);
});

it("modelShares breaks fresh ties by model name", () => {
  const tied: ModelUsage[] = [
    { model: "b", effort: null, total: usage(5, 0, 0, 0) },
    { model: "a", effort: null, total: usage(5, 0, 0, 0) },
  ];

  expect(modelShares(tied).map((s) => s.model)).toEqual(["a", "b"]);
});

it("modelShares is empty for no models or no fresh tokens", () => {
  const zero: ModelUsage[] = [{ model: "a", effort: null, total: usage(0, 0, 9, 0) }];

  expect(modelShares([])).toEqual([]);
  expect(modelShares(zero)).toEqual([]);
  expect(modelGroups([])).toEqual([]);
});

it("modelGroups keeps each model's efforts in input order with cache reads", () => {
  const groups = modelGroups(models);

  expect(groups.map((g) => g.model)).toEqual([
    "claude-sonnet-5-5",
    "claude-opus-5-5",
    "gpt-6.1-sol",
  ]);
  expect(groups[1]).toEqual({
    model: "claude-opus-5-5",
    fresh: 740000,
    cacheRead: 9000000,
    efforts: [
      { effort: "xhigh", fresh: 560000, cacheRead: 7000000 },
      { effort: "high", fresh: 180000, cacheRead: 2000000 },
    ],
  });
});

it("rollingRate ramps up over a young series and settles at the full-window rate", () => {
  const rate = rollingRate(Array<number>(12).fill(30), 30);

  expect(rate).toHaveLength(12);
  expect(rate[0]).toBe(6);
  expect(rate[11]).toBe(60);
});

it("rollingRate clamps a non-positive bucket length to one second", () => {
  const series = [10, 20, 30];

  for (const bucketSecs of [0, -30]) {
    const rate = rollingRate(series, bucketSecs);

    expect(rate.filter((value) => !Number.isFinite(value))).toEqual([]);
    expect(rate).toEqual(rollingRate(series, 1));
  }
});

it("cumulativeSeries ends at the total and never goes below zero", () => {
  expect(cumulativeSeries(100, [10, 20, 30])).toEqual([50, 70, 100]);
  expect(cumulativeSeries(10, [20, 30])).toEqual([0, 10]);
});

it("rollingRate and cumulativeSeries handle a series too long to spread", () => {
  const long = Array<number>(200_000).fill(1);

  expect(rollingRate(long, 30)).toHaveLength(200_000);
  expect(cumulativeSeries(200_000, long).at(-1)).toBe(200_000);
});

it("carryForward holds the previous value and zeroes leading gaps", () => {
  expect(carryForward([null, 5, null, 7])).toEqual([0, 5, 5, 7]);
  expect(carryForward([])).toEqual([]);
});

it("liveCounts counts agents active within the trailing window", () => {
  const burst = { ...main, series: [0, 0, 0, 1, 0, 0, 0, 0] };
  const idle = { ...codex, series: [] };

  expect(liveCounts([burst, idle], 8)).toEqual([0, 0, 0, 1, 1, 1, 1, 0]);
});

it("wholePercents gives three equal shares 34, 33, 33", () => {
  expect(wholePercents([1 / 3, 1 / 3, 1 / 3])).toEqual([34, 33, 33]);
});

it("wholePercents gives eight equal shares 13 for the first four and 12 for the rest", () => {
  const percents = wholePercents(Array<number>(8).fill(1 / 8));

  expect(percents).toEqual([13, 13, 13, 13, 12, 12, 12, 12]);
  expect(percents.reduce((total, percent) => total + percent, 0)).toBe(100);
});

it("wholePercents leaves the fixture's shares at 50, 33 and 17", () => {
  expect(wholePercents(modelShares(models).map((s) => s.share))).toEqual([50, 33, 17]);
});

it("wholePercents gives a single model 100 and no models none", () => {
  expect(wholePercents([1])).toEqual([100]);
  expect(wholePercents([])).toEqual([]);
});

it("wholePercents hands the missing points to the largest remainders", () => {
  expect(wholePercents([0.256, 0.254, 0.49])).toEqual([26, 25, 49]);
  expect(wholePercents([0.29, 0.71])).toEqual([29, 71]);
});
