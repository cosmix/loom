import { cn } from "cn";
import { useAtomValue } from "jotai/react";

import type { StageSummary } from "@/api/schema";
import { ActivityRoundel } from "@/components/activity-roundel";
import { StageSummaryText } from "@/components/stage-detail";
import { StateBadge, toneClass } from "@/components/state-badge";
import { mergeText, timeText } from "@/lib/format";
import { activityText } from "@/lib/format-activity";
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
