import { useEffect, useRef } from "react";
import { useLocation } from "react-router";

import type { Attention, StageSummary } from "@/api/schema";
import { StageDetails } from "@/components/stage-details";
import { HISTORY_HASH } from "@/components/stage-href";
import { HistoryCard } from "@/components/stage-history";
import { NowCard } from "@/components/stage-now";
import { StageTokens } from "@/components/stage-tokens";

/// The stage in reading order, shared by the stage dialog and the stage page:
/// what to do now (only when the stage has an attention entry), what it has been through, the
/// remaining detail behind one disclosure, and the token flow. A `#history` hash (the attention
/// card's "earlier issues" link) brings the history card into view.
export function StageReport({
  stage,
  entry,
  level,
  wide = false,
  detailsOpen = false,
}: {
  stage: StageSummary;
  entry: Attention | undefined;
  level: number | null;
  wide?: boolean;
  detailsOpen?: boolean;
}) {
  const history = useRef<HTMLElement>(null);
  const { hash } = useLocation();
  useEffect(() => {
    // jsdom has no scrollIntoView, hence the optional call.
    if (hash === HISTORY_HASH) history.current?.scrollIntoView?.({ block: "start" });
  }, [hash, stage.id]);

  return (
    <div className="flex flex-col gap-4">
      {entry && <NowCard stage={stage} entry={entry} />}
      {/* Keyed by stage so moving along the thread resets the fold of each. */}
      <HistoryCard key={`history:${stage.id}`} ref={history} stage={stage} entry={entry} />
      <StageDetails
        key={`details:${stage.id}`}
        stage={stage}
        level={level}
        wide={wide}
        defaultOpen={detailsOpen}
      />
      <StageTokens stageId={stage.id} />
    </div>
  );
}
