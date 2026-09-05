import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { useNavigate } from "react-router";

import type { StageSummary } from "@/api/schema";
import { ActivityRoundel } from "@/components/activity-roundel";
import { stageHref } from "@/components/stage-href";
import { StateBadge, StateGlyph, toneClass } from "@/components/state-badge";
import { activityText, mergeText, timeText } from "@/lib/format";
import { dependentsOf } from "@/lib/graph";
import { snapshotAtom } from "@/state/atoms";

/// State badge, activity, time and merge on one line: the row's cells
/// without the row.
export function StateLine({ stage, className }: { stage: StageSummary; className?: string }) {
  const activity = activityText(stage);
  const merge = mergeText(stage);
  const time = timeText(stage);
  return (
    <div className={cn("flex flex-wrap items-center gap-x-3 gap-y-1 text-sm", className)}>
      <StateBadge status={stage.status} />
      <ActivityRoundel stage={stage} size={15} />
      {activity && <span className={toneClass(activity.tone)}>{activity.text}</span>}
      {time && <span className="font-mono text-xs text-muted-foreground">{time}</span>}
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
  const chips = (label: string, ids: string[]) =>
    ids.length > 0 && (
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="eyebrow mr-1">{label}</span>
        {ids.map((id) => {
          const target = stages.find((entry) => entry.id === id);
          return (
            <button
              key={id}
              type="button"
              className="thread-chip"
              onClick={() => pick(id)}
              disabled={target === undefined}
            >
              {target && <StateGlyph status={target.status} className="text-[10px]" />}
              {id}
            </button>
          );
        })}
      </div>
    );
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      {chips("depends on", stage.dependencies)}
      {chips("feeds", feeds)}
    </div>
  );
}

/// The stage page's title block.
export function StageHeading({ stage }: { stage: StageSummary }) {
  const navigate = useNavigate();
  return (
    <header className="flex flex-col gap-2">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h1 className="text-xl font-semibold tracking-tight">{stage.name}</h1>
        <span className="font-mono text-sm text-muted-foreground">{stage.id}</span>
      </div>
      <StateLine stage={stage} />
      <ThreadRows stage={stage} pick={(id) => void navigate(stageHref(id))} className="mt-1" />
    </header>
  );
}
