import { useAtomValue } from "jotai/react";

import type { AgentFlow, StageFlow, TokenFlow } from "@/api/schema";
import { HeatStrip, Sparkline } from "@/components/sparkline";
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
  padLeft,
  rollingRate,
  stageFlow,
} from "@/lib/tokens";
import { snapshotAtom } from "@/state/atoms";

const MIN_CELLS = 20;
const RAMP = [0, 1, 2, 3, 4];
/// The chips' trends show the last 60 buckets: 30 minutes of 30 s cells.
const TREND_POINTS = 60;

interface Trend {
  values: number[];
  label: string;
}

/// Each chip's trend over the last `TREND_POINTS` buckets of the stage's series.
function chipTrends(tokens: TokenFlow, flow: StageFlow) {
  return {
    rate: lastValues(rollingRate(flow.series, tokens.bucket_secs), TREND_POINTS),
    total: lastValues(cumulativeSeries(fresh(flow.total), flow.series), TREND_POINTS),
    hit: lastValues(carryForward(flow.hit_series), TREND_POINTS),
    live: lastValues(liveCounts(flow.agents, flow.series.length), TREND_POINTS),
  };
}

/// The lanes' series (an idle agent's can be empty or short, so each is
/// left-padded to the stage's length), trimmed of the leading buckets that are
/// zero for every agent (keeping at least `MIN_CELLS`), and the maximum they
/// are scaled to. Loops over the series rather than spreading them into
/// arguments, which throws a RangeError on a long series.
function laneSeries(stage: StageFlow): { lanes: number[][]; max: number } {
  const length = stage.series.length;
  const full = stage.agents.map((agent) => padLeft(agent.series, length));
  let firstUsed = length;
  for (const series of full) {
    const index = series.findIndex((value) => value > 0);
    if (index !== -1 && index < firstUsed) firstUsed = index;
  }
  const start = Math.max(0, Math.min(firstUsed, length - MIN_CELLS));
  const lanes = full.map((series) => series.slice(start));
  const max = lanes.reduce((best, series) => maxOf(series, best), maxOf(stage.series));
  return { lanes, max };
}

function Chip({ value, label, trend }: { value: string; label: string; trend: Trend }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-md border border-hairline bg-background px-2.5 py-1.5">
      <div className="flex flex-wrap items-baseline justify-between gap-x-2">
        <span className="font-mono text-sm tabular-nums">{value}</span>
        <span className="ml-auto text-right text-[11px] text-muted-foreground">{label}</span>
      </div>
      <Sparkline
        values={trend.values}
        width={96}
        height={20}
        stretch
        label={trend.label}
        className="text-(--tone-executing)"
      />
    </div>
  );
}

function Lane({ agent, series, max }: { agent: AgentFlow; series: number[]; max: number }) {
  const label = agentLabel(agent);
  const state = agent.active ? "live" : "idle";
  return (
    <li
      data-testid="token-lane"
      className="grid grid-cols-[minmax(0,20rem)_minmax(0,1fr)_auto_auto] items-center gap-x-3 gap-y-1 max-md:grid-cols-[minmax(0,1fr)_auto_auto]"
    >
      <div className="flex min-w-0 flex-col">
        <span className="flex items-center gap-1.5 text-sm font-medium">
          <span
            aria-hidden="true"
            data-testid="agent-dot"
            data-state={state}
            className={`size-2 shrink-0 rounded-full bg-(--tone) ${agent.active ? "tone-completed dot-live" : "tone-dimmed"}`}
          />
          <span className="truncate" title={label}>
            {label}
          </span>
          {agent.kind === "codex" && (
            <span className="rounded-sm border border-hairline px-1 text-[10px] text-muted-foreground">
              codex
            </span>
          )}
        </span>
        <span
          className="truncate font-mono text-[11px] text-muted-foreground"
          title={`${agent.id} · ${agent.model ?? "unknown model"} · ${state}`}
        >
          {agent.id} · {agent.model ?? "unknown model"} · {state}
        </span>
      </div>
      <HeatStrip
        values={series}
        max={max}
        label={`${label} fresh tokens per 30 s`}
        className="max-md:order-last max-md:col-span-3"
      />
      <span className="w-16 text-right font-mono text-xs tabular-nums text-muted-foreground">
        {agent.active ? `${compactTokens(fresh(agent.rate))}/m` : "—"}
      </span>
      <span className="w-16 text-right font-mono text-xs tabular-nums">
        {compactTokens(fresh(agent.total))}
      </span>
    </li>
  );
}

/// The four headline chips, each with the trend of its own statistic.
function Chips({ tokens, flow }: { tokens: TokenFlow; flow: StageFlow }) {
  const hit = cacheHitPercent(flow.total);
  const live = flow.agents.filter((agent) => agent.active).length;
  const trends = chipTrends(tokens, flow);
  return (
    <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
      <Chip
        value={`${compactTokens(fresh(flow.rate))}/m`}
        label="fresh, 5 min avg"
        trend={{ values: trends.rate, label: "fresh rate, last 30 min" }}
      />
      <Chip
        value={compactTokens(fresh(flow.total))}
        label="fresh so far"
        trend={{ values: trends.total, label: "fresh so far, last 30 min" }}
      />
      <Chip
        value={hit === null ? "—" : `${hit.toFixed(1)}%`}
        label="cache hit"
        trend={{ values: trends.hit, label: "cache hit per 30 s, last 30 min" }}
      />
      <Chip
        value={`${live} of ${flow.agents.length}`}
        label="agents live"
        trend={{ values: trends.live, label: "agents live, last 30 min" }}
      />
    </div>
  );
}

/// The stage's token flow: headline chips and one heat-strip lane per agent.
/// Renders nothing for a stage the tracker has not seen.
export function StageTokens({ stageId }: { stageId: string }) {
  const tokens = useAtomValue(snapshotAtom)?.status.tokens;
  const flow = stageFlow(tokens, stageId);
  if (!tokens || !flow) return null;

  const { lanes, max } = laneSeries(flow);

  return (
    <section
      aria-label="stage tokens"
      className="flex flex-col gap-2.5 rounded-lg border border-hairline bg-card p-4"
    >
      <div className="flex items-center justify-between gap-3">
        <h2 className="eyebrow">tokens</h2>
        <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
          <span>30 s cells</span>
          <HeatStrip values={RAMP} max={4} label="intensity, low to high" className="w-16" />
        </div>
      </div>
      <Chips tokens={tokens} flow={flow} />
      <ul className="flex flex-col gap-2.5">
        {flow.agents.map((agent, index) => (
          <Lane key={agent.id} agent={agent} series={lanes[index] ?? []} max={max} />
        ))}
      </ul>
    </section>
  );
}
