import { ChevronRight } from "lucide-react";
import { useId } from "react";

import type { AgentFlow } from "@/api/schema";
import { HeatStrip } from "@/components/sparkline";
import { useDisclosure } from "@/components/use-disclosure";
import { agentLabel, compactTokens, fresh } from "@/lib/tokens";

export interface LaneRow {
  agent: AgentFlow;
  series: number[];
}

/// True when the agent drew nothing in the visible window.
export function isIdleRow({ series }: LaneRow): boolean {
  return series.every((value) => value === 0);
}

function LaneIdentity({ agent }: { agent: AgentFlow }) {
  const label = agentLabel(agent);
  const state = agent.active ? "live" : "idle";
  return (
    <div className="flex min-w-0 flex-col">
      <span className="flex items-center gap-1.5 text-sm font-medium">
        <span
          role="img"
          aria-label={state}
          data-testid="agent-dot"
          data-state={state}
          className={`size-2 shrink-0 rounded-full bg-(--tone) ${agent.active ? "tone-completed dot-live" : "tone-dimmed"}`}
        />
        <span className="min-w-0 wrap-anywhere" title={label}>
          {label}
        </span>
        {agent.kind === "codex" && (
          <span className="rounded-sm border border-hairline px-1 text-[10px] text-muted-foreground">
            codex
          </span>
        )}
      </span>
      <span className="flex min-w-0 items-baseline font-mono text-[11px] text-muted-foreground">
        <span className="shrink-0">{agent.model ?? "unknown model"}</span>
        <span className="shrink-0 whitespace-pre"> · </span>
        <span className="min-w-0 wrap-anywhere" title={agent.id}>
          {agent.id}
        </span>
      </span>
    </div>
  );
}

function Lane({
  agent,
  series,
  max,
  window,
  compact = false,
}: LaneRow & {
  max: number;
  /// The span the strip covers, as in "last 60 min".
  window: string;
  compact?: boolean;
}) {
  return (
    <li
      data-testid="token-lane"
      className={`grid grid-cols-[minmax(0,20rem)_minmax(0,1fr)_auto_auto] items-center gap-x-3 max-md:grid-cols-[minmax(0,1fr)_auto_auto] ${compact ? "gap-y-0.5" : "gap-y-1"}`}
    >
      <LaneIdentity agent={agent} />
      <HeatStrip
        values={series}
        max={max}
        label={`${agentLabel(agent)} fresh tokens per 30 s, ${window}`}
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

/// A forwarder subagent and the codex job(s) it forwarded to, clustered in one
/// tighter frame so the lanes read as a unit; every lane keeps all its fields.
/// The frame pulls outward by its own border and padding (1px + `px-2.5`), so
/// the lanes inside keep the width, and the heat-strip cells the x positions,
/// of the plain lanes above.
function CodexCluster({ rows, max, window }: { rows: LaneRow[]; max: number; window: string }) {
  return (
    <li data-testid="codex-cluster" className="list-none">
      <div className="-mx-[11px] flex flex-col gap-1.5 rounded-md border border-hairline bg-background px-2.5 py-2">
        <span className="eyebrow">codex</span>
        <ul className="flex flex-col gap-1.5">
          {rows.map(({ agent, series }) => (
            <Lane key={agent.id} agent={agent} series={series} max={max} window={window} compact />
          ))}
        </ul>
      </div>
    </li>
  );
}

/// Agents with nothing in the window, folded into one row under the lanes so a
/// blank strip never reads as broken. Opens to each one's identity and fresh
/// total; no heat strip.
function EarlierAgents({ agents }: { agents: AgentFlow[] }) {
  const [open, toggle] = useDisclosure(false);
  const panel = useId();
  const total = agents.reduce((sum, agent) => sum + fresh(agent.total), 0);
  return (
    <li data-testid="earlier-agents" className="list-none">
      <button
        type="button"
        className="disclosure-toggle w-full text-[11px] text-muted-foreground"
        aria-expanded={open}
        aria-controls={open ? panel : undefined}
        onClick={toggle}
      >
        <ChevronRight aria-hidden="true" className="disclosure-chevron" />
        <span>{agents.length === 1 ? "1 earlier agent" : `${agents.length} earlier agents`}</span>
        <span className="ml-auto font-mono text-xs tabular-nums text-foreground">
          {compactTokens(total)}
        </span>
      </button>
      {open && (
        <ul id={panel} className="mt-2 flex flex-col gap-2 border-l border-hairline pl-3">
          {agents.map((agent) => (
            <li
              key={agent.id}
              data-testid="earlier-agent"
              className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3"
            >
              <LaneIdentity agent={agent} />
              <span className="w-16 text-right font-mono text-xs tabular-nums">
                {compactTokens(fresh(agent.total))}
              </span>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

/// The active lanes (plain first, codex clusters last), then the earlier-agents
/// fold. A cluster is earlier only when every lane in it is idle.
export function TokenLanes({
  plain,
  clusters,
  max,
  window,
}: {
  plain: LaneRow[];
  clusters: LaneRow[][];
  max: number;
  window: string;
}) {
  const shownClusters = clusters.filter((cluster) => !cluster.every(isIdleRow));
  const earlier = [
    ...plain.filter(isIdleRow),
    ...clusters.filter((cluster) => cluster.every(isIdleRow)).flat(),
  ].map(({ agent }) => agent);
  return (
    <ul className="flex flex-col gap-2.5">
      {plain
        .filter((row) => !isIdleRow(row))
        .map(({ agent, series }) => (
          <Lane key={agent.id} agent={agent} series={series} max={max} window={window} />
        ))}
      {shownClusters.map((cluster) => (
        <CodexCluster key={cluster[0]?.agent.id} rows={cluster} max={max} window={window} />
      ))}
      {earlier.length > 0 && <EarlierAgents agents={earlier} />}
    </ul>
  );
}
