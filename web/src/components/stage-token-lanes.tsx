import { ChevronRight } from "lucide-react";
import { useId, type ReactNode } from "react";

import type { AgentFlow, CodexJob } from "@/api/schema";
import { useNow } from "@/components/hooks/use-now";
import { HeatStrip } from "@/components/sparkline";
import { toneClass } from "@/components/state-badge";
import { useDisclosure } from "@/components/use-disclosure";
import { jobStatus, jobTimingText } from "@/lib/codex-jobs";
import { modelWithEffort } from "@/lib/format";
import { agentLabel, compactTokens, fresh } from "@/lib/tokens";

export interface LaneRow {
  agent: AgentFlow;
  series: number[];
  /// The native worker invocation behind a codex lane, when the stage reports one.
  job?: CodexJob;
}

const CHIP = "rounded-sm border border-hairline px-1 text-[10px]";

/// A codex job's outcome chip (its reason on hover) and how long it ran against
/// its deadline. A running job's clock ticks; a finished one is fixed.
function CodexJobMeta({ job }: { job: CodexJob }) {
  const now = useNow();
  const status = jobStatus(job, now);
  return (
    <span data-testid="codex-job-meta" className="flex items-baseline gap-1.5 text-[11px]">
      <span title={status.reason} className={`${CHIP} ${toneClass(status.tone)}`}>
        {status.text}
      </span>
      <span className="font-mono tabular-nums text-muted-foreground">
        {jobTimingText(job, now)}
      </span>
    </span>
  );
}

/// True when the agent drew nothing in the visible window.
export function isIdleRow({ series }: LaneRow): boolean {
  return series.every((value) => value === 0);
}

function LaneIdentity({ agent, job }: { agent: AgentFlow; job?: CodexJob }) {
  const label = agentLabel(agent);
  // A finished job is never live, even while its transcript still reads as active.
  const live = agent.active && job?.phase !== "terminal";
  const state = live ? "live" : "idle";
  return (
    <div className="flex min-w-0 flex-col">
      <span className="flex items-center gap-1.5 text-sm font-medium">
        <span
          role="img"
          aria-label={state}
          data-testid="agent-dot"
          data-state={state}
          className={`size-2 shrink-0 rounded-full bg-(--tone) ${live ? "tone-completed dot-live" : "tone-dimmed"}`}
        />
        <span className="min-w-0 wrap-anywhere" title={label}>
          {label}
        </span>
        {agent.kind === "codex" && <span className={`${CHIP} text-muted-foreground`}>codex</span>}
      </span>
      <span className="flex min-w-0 items-baseline font-mono text-[11px] text-muted-foreground">
        <span className="shrink-0">
          {modelWithEffort(agent.model ?? "unknown model", agent.effort)}
        </span>
        <span className="shrink-0 whitespace-pre"> · </span>
        <span className="min-w-0 wrap-anywhere" title={agent.id}>
          {agent.id}
        </span>
      </span>
      {job && <CodexJobMeta job={job} />}
    </div>
  );
}

function Lane({
  agent,
  series,
  job,
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
      <LaneIdentity agent={agent} job={job} />
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

/// The tighter frame the codex lanes of one group share, so they read as a unit; every lane
/// keeps all its fields. The frame pulls outward by its own border and padding
/// (1px + `px-2.5`), so the lanes inside keep the width, and the heat-strip
/// cells the x positions, of the plain lanes above.
function CodexFrame({ children, testId }: { children: ReactNode; testId: string }) {
  return (
    <li data-testid={testId} className="list-none">
      <div className="-mx-[11px] flex flex-col gap-1.5 rounded-md border border-hairline bg-background px-2.5 py-2">
        <span className="eyebrow">codex</span>
        <ul className="flex flex-col gap-1.5">{children}</ul>
      </div>
    </li>
  );
}

/// A forwarder subagent and the codex job(s) it forwarded to, clustered in one frame.
function CodexCluster({ rows, max, window }: { rows: LaneRow[]; max: number; window: string }) {
  return (
    <CodexFrame testId="codex-cluster">
      {rows.map((row) => (
        <Lane key={row.agent.id} {...row} max={max} window={window} compact />
      ))}
    </CodexFrame>
  );
}

/// A native codex job with no lane: it failed before a Codex thread started, so it has
/// no token agent. The row shows who it was and how it ended, with no strip and no tokens.
function UnthreadedJob({ job }: { job: CodexJob }) {
  return (
    <li
      data-testid="codex-job-row"
      className="grid grid-cols-[minmax(0,20rem)_minmax(0,1fr)_auto_auto] items-center gap-x-3 gap-y-0.5 max-md:grid-cols-[minmax(0,1fr)_auto_auto]"
    >
      <div className="flex min-w-0 flex-col">
        <span className="flex items-center gap-1.5 text-sm font-medium">
          <span className="min-w-0 wrap-anywhere">{job.unit_id}</span>
          <span className={`${CHIP} text-muted-foreground`}>codex</span>
        </span>
        <span className="font-mono text-[11px] text-muted-foreground wrap-anywhere">
          {modelWithEffort(job.model, job.effort)} · no thread
        </span>
        <CodexJobMeta job={job} />
      </div>
      <span aria-hidden="true" className="max-md:hidden" />
      <span className="w-16 text-right font-mono text-xs tabular-nums text-muted-foreground">
        —
      </span>
      <span className="w-16 text-right font-mono text-xs tabular-nums">—</span>
    </li>
  );
}

/// Agents with nothing in the window, folded into one row under the lanes so a
/// blank strip never reads as broken. Opens to each one's identity and fresh
/// total; no heat strip.
function EarlierAgents({ rows }: { rows: LaneRow[] }) {
  const [open, toggle] = useDisclosure(false);
  const panel = useId();
  const total = rows.reduce((sum, { agent }) => sum + fresh(agent.total), 0);
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
        <span>{rows.length === 1 ? "1 earlier agent" : `${rows.length} earlier agents`}</span>
        <span className="ml-auto font-mono text-xs tabular-nums text-foreground">
          {compactTokens(total)}
        </span>
      </button>
      {open && (
        <ul id={panel} className="mt-2 flex flex-col gap-2 border-l border-hairline pl-3">
          {rows.map(({ agent, job }) => (
            <li
              key={agent.id}
              data-testid="earlier-agent"
              className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3"
            >
              <LaneIdentity agent={agent} job={job} />
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

/// The active lanes (plain first, codex last), then the earlier-agents fold. A
/// forwarder cluster has its own codex frame and is earlier only when every lane
/// in it is idle. Every native codex lane (no forwarder) and every `unthreaded`
/// job share one codex frame after the clusters.
export function TokenLanes({
  plain,
  clusters,
  unthreaded,
  max,
  window,
}: {
  plain: LaneRow[];
  clusters: LaneRow[][];
  unthreaded: CodexJob[];
  max: number;
  window: string;
}) {
  const forwarded = clusters.filter(([first]) => first?.agent.kind !== "codex");
  const native = clusters.filter(([first]) => first?.agent.kind === "codex").flat();
  const shownClusters = forwarded.filter((cluster) => !cluster.every(isIdleRow));
  const shownNative = native.filter((row) => !isIdleRow(row));
  const earlier = [
    ...plain.filter(isIdleRow),
    ...forwarded.filter((cluster) => cluster.every(isIdleRow)).flat(),
    ...native.filter(isIdleRow),
  ];
  return (
    <ul className="flex flex-col gap-2.5">
      {plain
        .filter((row) => !isIdleRow(row))
        .map((row) => (
          <Lane key={row.agent.id} {...row} max={max} window={window} />
        ))}
      {shownClusters.map((cluster) => (
        <CodexCluster key={cluster[0]?.agent.id} rows={cluster} max={max} window={window} />
      ))}
      {(shownNative.length > 0 || unthreaded.length > 0) && (
        <CodexFrame testId="codex-native">
          {shownNative.map((row) => (
            <Lane key={row.agent.id} {...row} max={max} window={window} compact />
          ))}
          {unthreaded.map((job) => (
            <UnthreadedJob key={job.invocation_id} job={job} />
          ))}
        </CodexFrame>
      )}
      {earlier.length > 0 && <EarlierAgents rows={earlier} />}
    </ul>
  );
}
