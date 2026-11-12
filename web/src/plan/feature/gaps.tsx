import { DraftTeamPanel } from "@/plan/feature/draft-team-panel";
import { gapStep, type GapScope } from "@/plan/feature/page-offer";
import { PaceChoice } from "@/plan/feature/pace";
import { hasGaps, Queue, type QueueKind } from "@/plan/feature/queue";
import { useFeature } from "@/plan/feature/use-feature";

const QUEUE_KIND: Record<GapScope, QueueKind> = { product: "section", metaplan: "plan" };

/**
 * What is still to write, above the page text: the pace choice, the draft team while one is
 * working, and the queue of documents while any is missing. `ConceptRoute` renders it through its
 * `above` slot, so the page keeps its own column and scroll container. The block stays empty
 * (and hidden) when nothing is left to write; the way to Coverage is the spine's row, which the
 * Product spec group ends with.
 */
export function Gaps({ scope }: { scope: GapScope }) {
  const { checklist } = useFeature();
  if (!checklist) return null;
  const step = gapStep(checklist, scope);
  const kind = QUEUE_KIND[scope];
  return (
    <div className="plan-gaps">
      {step && <PaceChoice checklist={checklist} step={step} />}
      <DraftTeamPanel feature={checklist.feature} />
      {hasGaps(checklist, kind) && <Queue checklist={checklist} kind={kind} />}
    </div>
  );
}
