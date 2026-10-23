import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { ArrowDown, ArrowUp, ChevronRight, Flag, Sprout, type LucideIcon } from "lucide-react";

import type { StageSummary } from "@/api/schema";
import { ActivityRoundel } from "@/components/activity-roundel";
import { LedgerCard, StageSummaryText } from "@/components/stage-detail";
import { StateBadge, StateGlyph, toneClass } from "@/components/state-badge";
import { activityText, mergeText, stateMeta, timeText } from "@/lib/format";
import { dependentsOf } from "@/lib/graph";
import { STAGE_STATES } from "@/lib/states";
import { compactTokens, fresh, stageFlow } from "@/lib/tokens";
import { snapshotAtom } from "@/state/atoms";

/// State badge, activity, time and merge on one line: the row's cells
/// without the row.
export function StateLine({ stage, className }: { stage: StageSummary; className?: string }) {
  const activity = activityText(stage);
  const merge = mergeText(stage);
  const time = timeText(stage);
  const flow = stageFlow(useAtomValue(snapshotAtom)?.status.tokens, stage.id);
  return (
    <div className={cn("flex flex-wrap items-center gap-x-3 gap-y-1 text-sm", className)}>
      <StateBadge status={stage.status} />
      <ActivityRoundel stage={stage} size={15} />
      {activity && (
        <span className={toneClass(activity.tone)} title={stage.completion_blocker?.next_action}>
          {activity.text}
        </span>
      )}
      {time && <span className="font-mono text-xs text-muted-foreground">{time}</span>}
      {flow && stage.status === "executing" && (
        <span className="font-mono text-xs text-muted-foreground">
          {compactTokens(fresh(flow.rate))}/m fresh
        </span>
      )}
      {merge && <span className={toneClass(merge.tone)}>{merge.text}</span>}
    </div>
  );
}

/// The stages on either side of this one along the thread: what it waits
/// for, and what waits for it. Each chip hands its id to `pick`.
export function ThreadRows({
  stage,
  pick,
  className,
}: {
  stage: StageSummary;
  pick: (id: string) => void;
  className?: string;
}) {
  const stages = useAtomValue(snapshotAtom)?.status.stages ?? [];
  const feeds = dependentsOf(stages, stage.id).map((entry) => entry.id);
  if (stage.dependencies.length === 0 && feeds.length === 0) return null;
  // Both sides show whenever either has a stage, so upstream stays on the
  // left and downstream on the right; the empty side says why it is empty.
  const panel = (
    label: string,
    ids: string[],
    Arrow: LucideIcon,
    empty: { icon: LucideIcon; title: string; detail: string },
  ) => (
    <LedgerCard
      title={label}
      icon={Arrow}
      aside={<span className="ledger-count">{ids.length}</span>}
    >
      {ids.map((id) => {
        const target = stages.find((entry) => entry.id === id);
        return (
          <button
            key={id}
            type="button"
            className="thread-row"
            title={target?.name}
            onClick={() => pick(id)}
            disabled={target === undefined}
          >
            {target ? (
              <StateGlyph status={target.status} className="text-xs" />
            ) : (
              <span className="size-3" />
            )}
            <span className="flex min-w-0 flex-col">
              <span className="truncate text-[13px] font-medium">{target?.name ?? id}</span>
              <span className="font-mono text-[11.5px] text-muted-foreground">{id}</span>
            </span>
            {target && (
              <span className={cn("text-xs", toneClass(stateMeta(target.status).tone))}>
                {STAGE_STATES[target.status].label}
              </span>
            )}
            <ChevronRight aria-hidden="true" className="thread-row-go" />
          </button>
        );
      })}
      {ids.length === 0 && (
        <div className="thread-empty">
          <span className="thread-empty-icon">
            <empty.icon aria-hidden="true" className="size-4" />
          </span>
          <span className="flex min-w-0 flex-col">
            <span className="text-[13px] font-medium">{empty.title}</span>
            <span className="text-xs text-muted-foreground">{empty.detail}</span>
          </span>
        </div>
      )}
    </LedgerCard>
  );
  return (
    <div className={cn("thread-panels", className)}>
      {panel("depends on", stage.dependencies, ArrowUp, {
        icon: Sprout,
        title: "Start of the thread",
        detail: "No dependencies, so this stage starts first.",
      })}
      {panel("feeds", feeds, ArrowDown, {
        icon: Flag,
        title: "End of the thread",
        detail: "Nothing waits on this stage.",
      })}
    </div>
  );
}

/// The stage page's title block. The thread sits outside it, at the page's
/// full width, so its right edge meets the sections below.
export function StageHeading({ stage }: { stage: StageSummary }) {
  return (
    <header className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <h1 className="text-xl leading-tight font-semibold tracking-tight text-balance">
          {stage.name}
        </h1>
        <span className="stage-id-chip">{stage.id}</span>
      </div>
      <StageSummaryText summary={stage.summary} />
      <StateLine stage={stage} className="stage-state-strip" />
    </header>
  );
}
