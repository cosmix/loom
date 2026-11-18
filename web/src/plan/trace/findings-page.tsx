import { useAtomValue } from "jotai/react";
import { useEffect, useState } from "react";
import { useLocation } from "react-router";

import type { OfferView } from "@/plan/api/schema";
import type { TraceView } from "@/plan/api/schema-trace";
import { sessionAtom } from "@/plan/chat/model";
import { DraftTeamPanel } from "@/plan/feature/draft-team-panel";
import { offerOfKind } from "@/plan/feature/page-offer";
import { useFeature } from "@/plan/feature/use-feature";
import { hashAnchor } from "@/plan/files/address";
import { OfferButton } from "@/plan/offers/offer-button";
import type { ButtonLook } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";
import { plural } from "@/plan/ui/plural";

import { Board } from "./findings-cards";
import {
  addressOffer,
  addressRunLive,
  boardLanes,
  ledgerSummary,
  nothingWaits,
  openCount,
} from "./findings-model";
import { FindingSheet } from "./findings-sheet";
import { useLedgerHash } from "./ledger-hash";
import { MapPage } from "./map-page";
import { useTrace } from "./state";
import { TraceGate } from "./trace-gate";

import "./maps.css";
import "./maps-findings.css";
import "./trace.css";

// `/f/<feature>/findings`: the ledger of what reviews found, as a board of four lanes in the order
// a finding moves (To verify, Yours to decide, Addressed, Set aside). Each lane owns the offer for
// its state: Review the feature (which verifies imported findings and records new ones) under To
// verify, Address the findings (one team session for every open, verified finding) under Yours to
// decide. A card opens its finding in a dialog, which holds Dismiss and Reopen. The team panel
// shows the address run while it is live. Before the board exists (the trace loading, failed or
// holding no findings) the head draws the Review button, since the page owns that offer whatever
// state the trace is in.

const LEDE =
  "Problems that reviews found in this feature's spec and plans. Each moves left to right: verified, decided by you, fixed by a team.";
const LEDE_QUIET =
  "Nothing waits for you. Every problem the reviews found is fixed in the documents or set aside.";

/** Opens the finding a `#finding-F-3` address names, and scrolls its card into view. */
function useOpenFromHash(ready: boolean, open: (id: string) => void): void {
  const { hash } = useLocation();
  useEffect(() => {
    const anchor = hashAnchor(hash);
    if (!ready || anchor === null) return;
    const id = /^finding-(.+)$/.exec(anchor)?.[1];
    if (id === undefined) return;
    document.getElementById(anchor)?.scrollIntoView?.();
    open(id);
  }, [ready, hash, open]);
}

function AddressControl(props: { trace: TraceView; feature: string; look: ButtonLook }) {
  const { trace, feature, look } = props;
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
  return offer ? <OfferButton offer={offer} look={look} /> : null;
}

interface ReviewOffer {
  offer: OfferView | undefined;
  look: ButtonLook;
}

function FindingsBody(props: { trace: TraceView; feature: string; review: ReviewOffer }) {
  const { trace, feature, review } = props;
  const session = useAtomValue(sessionAtom);
  const ledger = useLedgerHash(feature, trace);
  const addressing = addressRunLive(session, feature);
  const [openId, setOpenId] = useState<string | null>(null);
  useOpenFromHash(trace.findings.length > 0, setOpenId);
  if (trace.findings.length === 0) {
    return (
      <p className="plan-empty maps-findings">
        No findings yet. A run review adds its open findings here, and a review of the feature
        verifies them and adds the ones you record.
      </p>
    );
  }
  const context = { trace, feature, ledger, addressing };
  // One primary on the page: Address the findings, unless the checklist's own next step is a review.
  const addressLook = review.look === "primary" ? "secondary" : "primary";
  const actions = {
    verify: review.offer && <OfferButton offer={review.offer} look={review.look} />,
    decide: <AddressControl trace={trace} feature={feature} look={addressLook} />,
  };
  return (
    <div className="maps-findings">
      {addressing && <DraftTeamPanel feature={feature} />}
      <Board
        {...context}
        lanes={boardLanes(trace.findings, addressing)}
        onOpen={setOpenId}
        actions={actions}
      />
      <FindingSheet
        {...context}
        entry={trace.findings.find(({ finding }) => finding.id === openId)}
        onClose={() => setOpenId(null)}
      />
    </div>
  );
}

/** `/f/:feature/findings`: the ledger as four lanes, and the feature's two finding sessions. */
export function FindingsPage() {
  const { checklist } = useFeature();
  const { trace } = useTrace(checklist?.feature ?? "");
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  const offer = offerOfKind(checklist, "review_feature");
  const look = checklist.primary?.kind === "review_feature" ? "primary" : "secondary";
  const board = trace !== null && trace.findings.length > 0;
  const lede = trace !== null && nothingWaits(trace.findings) ? LEDE_QUIET : LEDE;
  return (
    <MapPage feature={checklist.feature} page="findings" title="Findings" lede={lede}>
      {(board || offer) && (
        <div className="maps-findings-head">
          {board && <p className="maps-findings-summary">{ledgerSummary(trace.findings)}</p>}
          {!board && offer && <OfferButton offer={offer} look={look} />}
        </div>
      )}
      <TraceGate noun="the findings">
        {(loaded, feature) => (
          <FindingsBody trace={loaded} feature={feature} review={{ offer, look }} />
        )}
      </TraceGate>
    </MapPage>
  );
}
