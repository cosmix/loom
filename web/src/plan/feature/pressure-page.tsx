import { Link } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { toAddress } from "@/plan/files/address";
import { pressureOffer, type PlanRow } from "@/plan/feature/page-offer";
import { NoPlans, PlanPage, UnwrittenPlans } from "@/plan/feature/plan-page";
import { PlanName, PlanStateStatus } from "@/plan/feature/plan-state";
import { livePlans } from "@/plan/feature/run-scope";
import { useFeature } from "@/plan/feature/use-feature";
import { OfferButton } from "@/plan/offers/offer-button";
import { LoadState } from "@/plan/ui/load-state";
import { plural } from "@/plan/ui/plural";

/**
 * What a plan's row ends in: the Record link of a tested plan, the plan's pressure-test offer
 * when it has one (a primary only when the checklist's primary is this very offer), else nothing.
 */
function RowEnd({ checklist, row }: { checklist: ChecklistView; row: PlanRow }) {
  if (row.pressure_date !== null) {
    return (
      <Link to={toAddress(row.path)} className="plan-page__link">
        Record
      </Link>
    );
  }
  const offer = pressureOffer(checklist, row);
  if (offer === null) return null;
  const owned = checklist.primary?.id === offer.id;
  return <OfferButton offer={offer} look={owned ? "primary" : "secondary"} size="sm" />;
}

function PlanRowItem({ checklist, row }: { checklist: ChecklistView; row: PlanRow }) {
  return (
    <li>
      <PlanName feature={checklist.feature} slug={row.slug} />
      <span className="plan-page__side">
        <PlanStateStatus state={row.state} />
        <span className="plan-fine">
          {row.pressure_date === null ? "Not tested" : <Untrusted text={row.pressure_date} />}
        </span>
        <RowEnd checklist={checklist} row={row} />
      </span>
    </li>
  );
}

/**
 * Counted from the rows the page draws, so a running plan (never counted by `checklist.untested`,
 * which covers written plans only) reads as untested here as it does in its row. A retired plan is
 * out of the build and out of the count.
 */
function ledeOf(checklist: ChecklistView): string {
  const live = livePlans(checklist);
  const tested = live.filter((row) => row.pressure_date !== null).length;
  if (tested === 0) return "No plan has a pressure test on record.";
  const verb = live.length === 1 ? "has" : "have";
  return `${tested} of ${plural(live.length, "plan")} ${verb} a pressure test on record.`;
}

/** `/f/:feature/pressure`: one row per plan with its last pressure test, or "Not tested". */
export function PressurePage() {
  const { checklist } = useFeature();
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  const rows = checklist.plan_rows;
  return (
    <PlanPage
      feature={checklist.feature}
      label="pressure test"
      title="Pressure test"
      lede={rows.length > 0 ? ledeOf(checklist) : undefined}
    >
      {rows.length === 0 ? (
        <NoPlans
          feature={checklist.feature}
          sentence="Write a plan first. A pressure test reads a written plan against the spec."
        />
      ) : (
        <>
          <ul aria-label="Plans" className="plan-list plan-page__rows">
            {rows.map((row) => (
              <PlanRowItem key={row.path} checklist={checklist} row={row} />
            ))}
          </ul>
          <UnwrittenPlans checklist={checklist} />
        </>
      )}
    </PlanPage>
  );
}
