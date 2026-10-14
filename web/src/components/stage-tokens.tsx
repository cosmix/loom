import { useAtomValue } from "jotai/react";

import type { AgentFlow, StageFlow } from "@/api/schema";
import { HeatStrip } from "@/components/sparkline";
import { agentLabel, cacheHitPercent, compactTokens, fresh, stageFlow } from "@/lib/tokens";
import { snapshotAtom } from "@/state/atoms";

const MIN_CELLS = 20;
const RAMP = [0, 1, 2, 3, 4];

/// An agent's series left-padded with zeros to `length`: an idle agent's
/// series can be empty or short.
function padded(series: readonly number[], length: number): number[] {
  return [...Array<number>(Math.max(0, length - series.length)).fill(0), ...series];
}

/// The lanes' series, trimmed of the leading buckets that are zero for every
/// agent (keeping at least `MIN_CELLS`), and the maximum they are scaled to.
function laneSeries(stage: StageFlow): { lanes: number[][]; max: number } {
  const length = stage.series.length;
  const full = stage.agents.map((agent) => padded(agent.series, length));
  const firstUsed = Math.min(
    length,
    ...full.map((series) => {
      const index = series.findIndex((value) => value > 0);
      return index === -1 ? length : index;
    }),
  );
  const start = Math.max(0, Math.min(firstUsed, length - MIN_CELLS));
  const lanes = full.map((series) => series.slice(start));
  return { lanes, max: Math.max(0, ...stage.series, ...lanes.flat()) };
}

function Chip({ value, label }: { value: string; label: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-0.5 rounded-md border border-hairline bg-background px-2.5 py-1.5">
      <span className="font-mono text-sm tabular-nums">{value}</span>
      <span className="text-[11px] text-muted-foreground">{label}</span>
    </div>
  );
}

function Lane({ agent, series, max }: { agent: AgentFlow; series: number[]; max: number }) {
  const label = agentLabel(agent);
  const state = agent.active ? "live" : "idle";
  return (
    <li
      data-testid="token-lane"
      className="grid grid-cols-[minmax(0,12rem)_minmax(0,1fr)_auto_auto] items-center gap-x-3 gap-y-1 max-md:grid-cols-[minmax(0,1fr)_auto_auto]"
    >
      <div className="flex min-w-0 flex-col">
        <span className="flex items-center gap-1.5 text-sm font-medium">
          <span className="truncate">{label}</span>
          {agent.kind === "codex" && (
            <span className="rounded-sm border border-hairline px-1 text-[10px] text-muted-foreground">
              codex
            </span>
          )}
        </span>
        <span className="truncate font-mono text-[11px] text-muted-foreground">
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

/// The stage's token flow: headline chips and one heat-strip lane per agent.
/// Renders nothing for a stage the tracker has not seen.
export function StageTokens({ stageId }: { stageId: string }) {
  const flow = stageFlow(useAtomValue(snapshotAtom)?.status.tokens, stageId);
  if (!flow) return null;

  const { lanes, max } = laneSeries(flow);
  const hit = cacheHitPercent(flow.total);
  const live = flow.agents.filter((agent) => agent.active).length;

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
      <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
        <Chip value={`${compactTokens(fresh(flow.rate))}/m`} label="fresh, 5 min avg" />
        <Chip value={compactTokens(fresh(flow.total))} label="fresh so far" />
        <Chip value={hit === null ? "—" : `${hit.toFixed(1)}%`} label="cache hit" />
        <Chip value={`${live} of ${flow.agents.length}`} label="agents live" />
      </div>
      <ul className="flex flex-col gap-2.5">
        {flow.agents.map((agent, index) => (
          <Lane key={agent.id} agent={agent} series={lanes[index] ?? []} max={max} />
        ))}
      </ul>
    </section>
  );
}
