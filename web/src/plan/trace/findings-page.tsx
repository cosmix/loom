import { useAtomValue } from "jotai/react";
import { useEffect } from "react";
import { useLocation } from "react-router";

import type { TraceView } from "@/plan/api/schema-trace";
import { sessionAtom } from "@/plan/chat/model";
import { DraftTeamPanel } from "@/plan/feature/draft-team-panel";
import { offerOfKind } from "@/plan/feature/page-offer";
import { useFeature } from "@/plan/feature/use-feature";
import { hashAnchor } from "@/plan/files/address";
import { OfferButton } from "@/plan/offers/offer-button";
import { LoadState } from "@/plan/ui/load-state";
import { plural } from "@/plan/ui/plural";

import { Group } from "./findings-cards";
import { addressOffer, addressRunLive, groupFindings, openCount, tally } from "./findings-model";
import { useLedgerHash } from "./ledger-hash";
import { MapPage } from "./map-page";
import { useTrace } from "./state";
import { TraceGate } from "./trace-gate";

import "./maps.css";
import "./maps-findings.css";
import "./trace.css";

// `/f/<feature>/findings`: the ledger of what reviews found, grouped by the run that found it.
// The head holds the tally, Address the findings (one team session for every open, verified
// finding) and Review the feature (which verifies imported findings and records new ones); the
// team panel shows the address run while it is live. The head's Review button is drawn before
// the trace arrives, since the page owns that offer whatever state the trace is in.

/** Scrolls to the finding the address names (`#finding-F-3`), once the cards are drawn. */
function useScrollToHash(ready: boolean): void {
  const { hash } = useLocation();
  useEffect(() => {
    const id = hashAnchor(hash);
    if (!ready || id === null) return;
    document.getElementById(id)?.scrollIntoView?.();
  }, [ready, hash]);
}

function AddressControl({ trace, feature }: { trace: TraceView; feature: string }) {
  const session = useAtomValue(sessionAtom);
  const live = addressRunLive(session, feature);
  const offer = addressOffer(trace, feature);
  if (live) {
    return (
      <p className="plan-fine maps-findings-run" role="status">
        Addressing {plural(openCount(trace), "finding")} with a team
      </p>
    );
  }
  return offer ? <OfferButton offer={offer} /> : null;
}

function FindingsBody({ trace, feature }: { trace: TraceView; feature: string }) {
  const session = useAtomValue(sessionAtom);
  const ledger = useLedgerHash(feature, trace);
  const groups = groupFindings(trace.findings);
  const addressing = addressRunLive(session, feature);
  useScrollToHash(groups.length > 0);
  if (groups.length === 0) {
    return (
      <p className="plan-empty maps-findings">
        No findings yet. A run review adds its open findings here, and a review of the feature
        verifies them and adds the ones you record.
      </p>
    );
  }
  return (
    <div className="maps-findings">
      {addressing && <DraftTeamPanel feature={feature} />}
      {groups.map((group) => (
        <Group
          key={group.label}
          group={group}
          trace={trace}
          feature={feature}
          ledger={ledger}
          addressing={addressing}
        />
      ))}
    </div>
  );
}

const STANDFIRST =
  "What reviews found about this feature: each finding quotes the text it is about, and an address run edits the documents and records what changed.";

/** `/f/:feature/findings`: the ledger, its run groups, and the feature's two finding sessions. */
export function FindingsPage() {
  const { checklist } = useFeature();
  const { trace } = useTrace(checklist?.feature ?? "");
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  const review = offerOfKind(checklist, "review_feature");
  const reviewLook = checklist.primary?.kind === "review_feature" ? "primary" : "secondary";
  const counts = trace === null ? "" : tally(trace.findings);
  return (
    <MapPage feature={checklist.feature} page="findings" title="Findings" lede={STANDFIRST}>
      {(counts !== "" || review || trace) && (
        <div className="maps-findings-head">
          {counts !== "" && <p className="maps-findings-tally">{counts}</p>}
          <div className="maps-findings-offers">
            {trace && <AddressControl trace={trace} feature={checklist.feature} />}
            {review && <OfferButton offer={review} look={reviewLook} />}
          </div>
        </div>
      )}
      <TraceGate noun="the findings">
        {(loaded, feature) => <FindingsBody trace={loaded} feature={feature} />}
      </TraceGate>
    </MapPage>
  );
}
