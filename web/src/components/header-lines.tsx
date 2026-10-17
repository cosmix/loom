import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import type { CSSProperties, ReactNode } from "react";

import type { Snapshot, StageStatus } from "@/api/schema";
import { useNow } from "@/components/hooks/use-now";
import { StageStrip } from "@/components/stage-strip";
import { toneClass } from "@/components/state-badge";
import { daemonLine, progressPercent, stateMeta, summaryCounts, type Tone } from "@/lib/format";
import { STAGE_STATES } from "@/lib/states";
import { cacheHitPercent, compactTokens, fresh } from "@/lib/tokens";
import { connectionAtom, snapshotAtom, type ConnectionPhase } from "@/state/atoms";

const CONNECTION_LINE: Record<ConnectionPhase, ReturnType<typeof daemonLine>> = {
  connecting: { text: "connecting", tone: "dimmed" },
  live: { text: "awaiting data", tone: "dimmed" },
  reconnecting: { text: "reconnecting", tone: "warning" },
  offline: { text: "connection offline", tone: "blocked" },
  error: { text: "connection error", tone: "blocked" },
};

/// "● daemon running · tick 4s ago", toned by `daemonLine`. Before a valid
/// frame arrives it shows transport state. Once the feed drops, daemon state
/// is unknown regardless of the frozen snapshot, so connection staleness
/// overrides it.
export function DaemonLine({ snapshot }: { snapshot: Snapshot | null }) {
  const connection = useAtomValue(connectionAtom);
  const now = useNow();
  const stale =
    connection.phase === "reconnecting" ||
    connection.phase === "offline" ||
    connection.phase === "error";
  const staleSecs = stale ? Math.max(0, Math.round((now - connection.since) / 1000)) : null;
  const line = snapshot
    ? daemonLine(snapshot.daemon, snapshot.tick_age_secs, staleSecs)
    : CONNECTION_LINE[connection.phase];
  const diagnostics = [connection.message, snapshot?.notice].filter(Boolean).join("\n");
  return (
    <span
      className={cn(
        "inline-flex h-7 items-center gap-1.5 rounded-full border border-hairline bg-card/60 px-2.5 text-xs font-medium",
        toneClass(line.tone),
      )}
      title={diagnostics || undefined}
    >
      <span aria-hidden="true" className="size-2 rounded-full bg-(--tone)" />
      <span>{line.text}</span>
      {line.detail && <span className="text-muted-foreground">· {line.detail}</span>}
    </span>
  );
}

/// "78%" set large, "7 of 9 stages complete", and the warp strip stretched
/// across the rest of the row.
export function ProgressLine({ snapshot }: { snapshot: Snapshot }) {
  const { completed, total } = snapshot.status.progress;
  const percent = progressPercent(completed, total);
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      <div className="flex items-baseline gap-2.5">
        <span className="font-display text-2xl leading-none font-semibold tracking-tight tabular-nums">
          {percent}
          <span className="text-base text-muted-foreground">%</span>
        </span>
        <span className="text-sm text-muted-foreground tabular-nums">
          {completed} of {total} stages complete
        </span>
      </div>
      <div className="min-w-32 max-w-xl flex-1 basis-full sm:basis-0">
        <StageStrip className="header-strip" />
      </div>
    </div>
  );
}

type Cell = {
  key: string;
  glyph: string;
  count: number;
  label: string;
  tone: Tone;
  /// A cell whose non-zero count is a problem: the whole cell takes the tone.
  alarm?: boolean;
  title?: string;
};

/// The stage counts as a ruled rail: executing, queued, waiting, attention,
/// done. Glyphs are the state glyphs used everywhere else.
export function SummaryLine({ snapshot, attention }: { snapshot: Snapshot; attention: number }) {
  const counts = summaryCounts(snapshot.status.stages, attention);
  const cell = (key: keyof typeof counts, status: StageStatus, label: string): Cell => ({
    key,
    glyph: STAGE_STATES[status].icon,
    count: counts[key],
    label,
    tone: stateMeta(status).tone,
  });
  return (
    <StatRail
      title="stages"
      cells={[
        cell("executing", "executing", "executing"),
        cell("queued", "queued", "queued"),
        cell("waiting", "waiting-for-deps", "waiting"),
        {
          key: "attention",
          glyph: "!",
          count: counts.attention,
          label: "attention",
          tone: "blocked",
          alarm: true,
        },
        cell("done", "completed", "done"),
      ]}
    />
  );
}

/// The merge counts as a rail beside the stage rail. The diamonds pair
/// filled/hollow the way ● and ○ do; ⚡ is the merge-conflict state glyph.
/// Hovering a count lists its stage ids.
export function MergeLine({ snapshot }: { snapshot: Snapshot }) {
  const { merged, pending, conflicts } = snapshot.status.merge;
  const cell = (key: string, glyph: string, ids: string[], tone: Tone, alarm: boolean): Cell => ({
    key,
    glyph,
    count: ids.length,
    label: key,
    tone,
    alarm,
    title: ids.length > 0 ? ids.join(", ") : undefined,
  });
  return (
    <StatRail
      title="merge"
      cells={[
        cell("merged", "◆", merged, "merged", false),
        cell("unmerged", "◇", pending, "warning", true),
        cell("conflicts", "⚡", conflicts, "warning", true),
      ]}
    />
  );
}

/// The plan's token summary rail beside the merge rail: the plan total and
/// cache hit. The sidebar token-flow panel owns the fresh-rate display.
/// Absent while the snapshot carries no token flow.
export function TokenLine() {
  const tokens = useAtomValue(snapshotAtom)?.status.tokens;
  if (!tokens) return null;
  const hit = cacheHitPercent(tokens.plan.total);
  return (
    <section aria-label="tokens" className="flex w-full min-w-0 flex-col gap-1 sm:w-auto">
      <h2 className="eyebrow text-[10px]">tokens</h2>
      <ol className="grid grid-cols-2 rounded-lg border border-hairline bg-card/60 shadow-xs sm:inline-grid sm:grid-cols-[repeat(2,auto)]">
        <TokenCell label="plan total">{compactTokens(fresh(tokens.plan.total))}</TokenCell>
        <TokenCell label="cache hit">{hit === null ? "—" : `${Math.round(hit)}%`}</TokenCell>
      </ol>
    </section>
  );
}

/// One cell of the token rail.
function TokenCell({ label, children }: { label: string; children: ReactNode }) {
  return (
    <li className="stat-cell flex min-w-0 flex-col items-center gap-1 px-1.5 py-1.5 sm:items-start sm:px-3.5">
      <span className="flex h-[18px] items-center font-display text-lg leading-none font-semibold tabular-nums">
        {children}
      </span>
      <span className="max-w-full truncate text-[11px] font-medium text-muted-foreground sm:text-xs">
        {label}
      </span>
    </li>
  );
}

function StatRail({ title, cells }: { title: string; cells: Cell[] }) {
  return (
    <section aria-label={title} className="flex w-full min-w-0 flex-col gap-1 sm:w-auto">
      <h2 className="eyebrow text-[10px]">{title}</h2>
      {/* Phones: equal columns across the full width. From sm: each cell
          sizes to its content. */}
      <ol
        className="grid grid-cols-[repeat(var(--cells),minmax(0,1fr))] rounded-lg border border-hairline bg-card/60 shadow-xs sm:inline-grid sm:grid-cols-[repeat(var(--cells),auto)]"
        style={{ "--cells": cells.length } as CSSProperties}
      >
        {cells.map((cell) => (
          <StatCell key={cell.key} cell={cell} />
        ))}
      </ol>
    </section>
  );
}

function StatCell({ cell }: { cell: Cell }) {
  const quiet = cell.count === 0;
  const raised = cell.alarm && !quiet;
  return (
    <li
      className={cn(
        "stat-cell flex min-w-0 flex-col items-center gap-1 px-1.5 py-1.5 sm:items-start sm:px-3.5",
        raised && cn(toneClass(cell.tone), "stat-cell-raised"),
      )}
      title={cell.title}
    >
      <span className="flex items-baseline gap-1.5">
        <span
          aria-hidden="true"
          className={cn(
            "inline-block w-[1.1em] text-center font-mono text-sm",
            quiet ? "text-muted-foreground/50" : toneClass(cell.tone),
          )}
        >
          {cell.glyph}
        </span>
        <span
          className={cn(
            "font-display text-lg leading-none font-semibold tabular-nums",
            quiet && "text-muted-foreground/70",
          )}
        >
          {cell.count}
        </span>
      </span>
      <span
        className={cn(
          "max-w-full truncate text-[11px] font-medium sm:text-xs",
          raised ? "text-(--tone)" : "text-muted-foreground",
        )}
      >
        {cell.label}
      </span>
    </li>
  );
}
