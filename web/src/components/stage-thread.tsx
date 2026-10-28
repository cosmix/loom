import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { ChevronRight, Flag, Sprout, type LucideIcon } from "lucide-react";
import { useState } from "react";

import type { StageSummary } from "@/api/schema";
import { StateGlyph, toneClass } from "@/components/state-badge";
import { stateMeta } from "@/lib/format";
import { dependentsOf } from "@/lib/graph";
import { STAGE_STATES } from "@/lib/states";
import { snapshotAtom } from "@/state/atoms";

/// A run longer than this folds into a count and one dot per neighbour.
const FOLD_ABOVE = 6;

/// The stage in the middle of its thread: what it waits for on the left,
/// what waits for it on the right. Each neighbour chip hands its id to `pick`;
/// an empty side is capped, and a long run folds until opened.
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
  const feeds = dependentsOf(stages, stage.id);
  if (stage.dependencies.length === 0 && feeds.length === 0) return null;
  const waits = stage.dependencies.flatMap((id) => stages.find((entry) => entry.id === id) ?? []);
  return (
    <div className={cn("fulcrum", className)}>
      <div className="fulcrum-track">
        {stage.dependencies.length === 0 ? (
          <ThreadCap icon={Sprout} text="start" title="No dependencies: this stage starts first" />
        ) : (
          <>
            <ThreadRun label="waits for" stages={waits} pick={pick} />
            <span aria-hidden="true" className="fulcrum-caption">
              waits for
            </span>
          </>
        )}
        <ChevronRight aria-hidden="true" className="fulcrum-arrow" />
        <span className="fulcrum-self" aria-current="true">
          <span aria-hidden="true" className="fulcrum-self-dot" />
          this stage
        </span>
        <ChevronRight aria-hidden="true" className="fulcrum-arrow fulcrum-arrow-end" />
        {feeds.length === 0 ? (
          <ThreadCap icon={Flag} text="end" title="Nothing waits on this stage" end />
        ) : (
          <>
            <ThreadRun label="feeds" stages={feeds} pick={pick} end />
            <span aria-hidden="true" className="fulcrum-caption fulcrum-caption-end">
              feeds
            </span>
          </>
        )}
      </div>
    </div>
  );
}

function ThreadCap({
  icon: Icon,
  text,
  title,
  end,
}: {
  icon: LucideIcon;
  text: string;
  title: string;
  end?: boolean;
}) {
  return (
    <span className={cn("fulcrum-cap", end && "fulcrum-cap-end")} title={title}>
      <Icon aria-hidden="true" />
      {text}
    </span>
  );
}

function ThreadRun({
  label,
  stages,
  pick,
  end,
}: {
  label: string;
  stages: StageSummary[];
  pick: (id: string) => void;
  end?: boolean;
}) {
  const runClass = cn("fulcrum-run", end && "fulcrum-run-end");
  const [open, setOpen] = useState(false);
  const foldable = stages.length > FOLD_ABOVE;
  if (foldable && !open) {
    return (
      <div className={runClass} role="group" aria-label={label}>
        <button
          type="button"
          className="fulcrum-fold"
          aria-expanded={false}
          aria-label={`${label} ${stages.length}: ${tally(stages)}`}
          onClick={() => setOpen(true)}
        >
          <span aria-hidden="true">{stages.length}</span>
          <span aria-hidden="true" className="fulcrum-dots">
            {stages.map((entry) => (
              <span
                key={entry.id}
                className={cn("fulcrum-dot", toneClass(stateMeta(entry.status).tone))}
              />
            ))}
          </span>
        </button>
      </div>
    );
  }
  return (
    <div className={runClass} role="group" aria-label={label}>
      {stages.map((entry) => (
        <ThreadChip key={entry.id} stage={entry} pick={pick} />
      ))}
      {foldable && (
        <button
          type="button"
          className="fulcrum-fold"
          aria-expanded={true}
          aria-label={`Fold ${label} ${stages.length}`}
          onClick={() => setOpen(false)}
        >
          fold
        </button>
      )}
    </div>
  );
}

function ThreadChip({ stage, pick }: { stage: StageSummary; pick: (id: string) => void }) {
  const label = STAGE_STATES[stage.status].label;
  return (
    <button
      type="button"
      className={cn("fulcrum-chip", toneClass(stateMeta(stage.status).tone))}
      title={`${stage.name} · ${label}`}
      onClick={() => pick(stage.id)}
    >
      <span aria-hidden="true" className="inline-flex">
        <StateGlyph status={stage.status} className="text-[11px]" />
      </span>
      <span className="text-foreground">{stage.id}</span>
      <span className="sr-only">{label}</span>
    </button>
  );
}

/// "5 completed, 2 executing": counts per state, in first-seen order.
function tally(stages: StageSummary[]): string {
  const counts = new Map<string, number>();
  for (const entry of stages) {
    const label = STAGE_STATES[entry.status].label.toLowerCase();
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  return [...counts].map(([label, count]) => `${count} ${label}`).join(", ");
}
