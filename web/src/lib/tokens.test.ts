import { expect, it } from "vitest";

import type { AgentFlow, StageFlow, TokenFlow, TokenUsage } from "@/api/schema";
import {
  agentLabel,
  cacheHitPercent,
  compactTokens,
  fresh,
  modelRates,
  quotaLeftText,
  stageFlow,
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
  agents: [main, subagent, codex],
};
const flow: TokenFlow = {
  observed_at: 1788523200,
  bucket_secs: 30,
  plan: { total: server.total, rate: server.rate, series: Array(60).fill(21600) },
  stages: [server],
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
