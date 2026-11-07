import { useEffect } from "react";
import { useLocation } from "react-router";

import type { TraceView } from "@/plan/api/schema-trace";
import { hashAnchor } from "@/plan/files/address";
import { offerOfKind } from "@/plan/feature/page-offer";
import { PlanPage } from "@/plan/feature/plan-page";
import { useFeature } from "@/plan/feature/use-feature";
import { OfferButton } from "@/plan/offers/offer-button";
import { LoadState } from "@/plan/ui/load-state";
import { plural } from "@/plan/ui/plural";

import { Group } from "./findings-cards";
import { followUpsOffer, groupFindings, openCount } from "./findings-model";
import { useLedgerHash } from "./ledger-hash";
import { useTrace } from "./state";
import { TraceGate } from "./trace-gate";

import "./maps-findings.css";
import "./trace.css";

// `/f/<feature>/findings`: the ledger of what reviews found, grouped by the run that found it,
// with the three ways to settle a finding (`findings-cards.tsx`). The page head and its Review
// the feature button are drawn before the trace arrives, since the page owns that offer whatever
// state the trace is in; only the ledger waits on the trace.

/** Scrolls to the finding the address names (`#finding-F-3`), once the cards are drawn. */
function useScrollToHash(ready: boolean): void {
  const { hash } = useLocation();
  useEffect(() => {
    const id = hashAnchor(hash);
    if (!ready || id === null) return;
    document.getElementById(id)?.scrollIntoView?.();
  }, [ready, hash]);
}

function FindingsBody({ trace, feature }: { trace: TraceView; feature: string }) {
  const { checklist } = useFeature();
  const ledger = useLedgerHash(feature, trace);
  const groups = groupFindings(trace.findings);
  useScrollToHash(groups.length > 0);
  const followUps = followUpsOffer(
    checklist ? offerOfKind(checklist, "plan_follow_ups") : undefined,
    trace,
    feature,
  );
  if (groups.length === 0) {
    return (
      <p className="plan-empty">
        No findings yet. A run review adds its open findings here, and a review of the feature adds
        the ones you record.
      </p>
    );
  }
  return (
    <div className="maps-findings">
      {groups.map((group) => (
        <Group
          key={group.label}
          group={group}
          trace={trace}
          feature={feature}
          offer={followUps}
          ledger={ledger}
        />
      ))}
    </div>
  );
}

/** `/f/:feature/findings`: the ledger, its run groups and the actions on each open finding. */
export function FindingsPage() {
  const { checklist } = useFeature();
  const { trace } = useTrace(checklist?.feature ?? "");
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  const review = offerOfKind(checklist, "review_feature");
  const reviewLook = checklist.primary?.kind === "review_feature" ? "primary" : "secondary";
  const lede =
    trace === null
      ? undefined
      : `${openCount(trace)} open of ${plural(trace.findings.length, "finding")}.`;
  return (
    <PlanPage feature={checklist.feature} label="findings" title="Findings" lede={lede}>
      {review && (
        <div className="maps-findings-head">
          <OfferButton offer={review} look={reviewLook} />
        </div>
      )}
      <TraceGate noun="the findings">
        {(loaded, feature) => <FindingsBody trace={loaded} feature={feature} />}
      </TraceGate>
    </PlanPage>
  );
}
