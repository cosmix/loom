import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { ArrowDown, ArrowUp, ChevronRight, Flag, Sprout, type LucideIcon } from "lucide-react";

import type { StageSummary } from "@/api/schema";
import { ActivityRoundel } from "@/components/activity-roundel";
import { StageSummaryText } from "@/components/stage-detail";
import { StateBadge, toneClass } from "@/components/state-badge";
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

/// The stages on either side of this one along the thread, as one row of
/// chips: what it waits for, and what waits for it. Each chip hands its id to
/// `pick`; an empty side says why it is empty.
export function ThreadChips({
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
  const side = (label: string, ids: string[], Arrow: LucideIcon, empty: ThreadEnd) => (
    <div className="thread-side" role="group" aria-label={label}>
      <span className="eyebrow">{label}</span>
      {ids.map((id) => (
        <ThreadChip
          key={id}
          id={id}
          target={stages.find((entry) => entry.id === id)}
          arrow={Arrow}
          pick={pick}
        />
      ))}
      {ids.length === 0 && (
        <span className="thread-end">
          <empty.icon aria-hidden="true" className="thread-end-icon" />
          {empty.text}
        </span>
      )}
    </div>
  );
  return (
    <div className={cn("thread-chips", className)}>
      {side("depends on", stage.dependencies, ArrowUp, {
        icon: Sprout,
        text: "Start of the thread: no dependencies, so this stage starts first",
      })}
      {side("feeds", feeds, ArrowDown, {
        icon: Flag,
        text: "End of the thread: nothing waits on this stage",
      })}
    </div>
  );
}

interface ThreadEnd {
  icon: LucideIcon;
  text: string;
}

function ThreadChip({
  id,
  target,
  arrow: Arrow,
  pick,
}: {
  id: string;
  target: StageSummary | undefined;
  arrow: LucideIcon;
  pick: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className="thread-chip"
      onClick={() => pick(id)}
      disabled={target === undefined}
    >
      <Arrow aria-hidden="true" className="thread-chip-arrow" />
      <span className="font-medium wrap-anywhere">{target?.name ?? id}</span>
      <span className="font-mono text-[11.5px] wrap-anywhere text-muted-foreground">{id}</span>
      {target && (
        <span className={cn("text-xs", toneClass(stateMeta(target.status).tone))}>
          {STAGE_STATES[target.status].label}
        </span>
      )}
      <ChevronRight aria-hidden="true" className="thread-chip-go" />
    </button>
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
