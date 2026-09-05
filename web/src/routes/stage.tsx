import { useAtomValue } from "jotai/react";
import { ArrowLeftIcon } from "lucide-react";
import { Link, useParams } from "react-router";

import type { StageSummary } from "@/api/schema";
import { StateBadge, toneClass } from "@/components/state-badge";
import {
  AdjudicationSection,
  ContextSection,
  FailureSection,
  GraphSection,
  IdentitySection,
  MergeSection,
  NotesSection,
  RetriesSection,
  SessionSection,
  TimingSection,
} from "@/components/stage-sections";
import { Skeleton } from "@/components/ui/skeleton";
import { activityText, mergeText, timeText } from "@/lib/format";
import { orderedStagesAtom, selectStage, snapshotAtom } from "@/state/atoms";

/// Route `/stages/:stageId`: everything the row shows plus every remaining
/// `StageSummary` field, grouped.
export function StagePage() {
  const { stageId = "" } = useParams<{ stageId: string }>();
  const snapshot = useAtomValue(snapshotAtom);
  const ordered = useAtomValue(orderedStagesAtom);

  if (snapshot === null) return <StageSkeleton />;
  const stage = selectStage(snapshot, stageId);
  if (stage === undefined) return <NotFound id={stageId} />;
  const level = ordered.find((entry) => entry.stage.id === stageId)?.level ?? null;

  return (
    <article className="flex flex-col gap-5">
      <BackLink />
      <StageHeading stage={stage} />
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        <IdentitySection stage={stage} />
        <GraphSection stage={stage} level={level} />
        <TimingSection stage={stage} />
        <ContextSection stage={stage} />
        <SessionSection stage={stage} />
        <RetriesSection stage={stage} />
        <AdjudicationSection stage={stage} />
        <MergeSection stage={stage} />
        <NotesSection stage={stage} />
        <div className="md:col-span-2 xl:col-span-3">
          <FailureSection stage={stage} />
        </div>
      </div>
    </article>
  );
}

function StageHeading({ stage }: { stage: StageSummary }) {
  const activity = activityText(stage);
  const merge = mergeText(stage);
  const time = timeText(stage);
  return (
    <header className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <h1 className="text-xl font-semibold tracking-tight">{stage.name}</h1>
        <span className="font-mono text-sm text-muted-foreground">{stage.id}</span>
      </div>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
        <StateBadge status={stage.status} />
        {activity && <span className={toneClass(activity.tone)}>{activity.text}</span>}
        {time && <span className="font-mono text-xs text-muted-foreground">{time}</span>}
        {merge && <span className={toneClass(merge.tone)}>{merge.text}</span>}
      </div>
    </header>
  );
}

function BackLink() {
  return (
    <Link
      to="/"
      className="inline-flex w-fit items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
    >
      <ArrowLeftIcon className="size-4" />
      ledger
    </Link>
  );
}

function NotFound({ id }: { id: string }) {
  return (
    <div className="flex flex-col gap-3">
      <BackLink />
      <p className="text-sm">
        No stage named <code className="rounded bg-muted px-1 py-0.5 text-xs">{id}</code> in this
        workspace.
      </p>
    </div>
  );
}

function StageSkeleton() {
  return (
    <div className="flex flex-col gap-4" aria-busy="true">
      <Skeleton className="h-4 w-16" />
      <Skeleton className="h-7 w-64" />
      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
        <Skeleton className="h-36" />
        <Skeleton className="h-36" />
        <Skeleton className="h-36" />
      </div>
    </div>
  );
}
