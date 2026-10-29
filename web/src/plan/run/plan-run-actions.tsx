import { useAtomValue, useSetAtom } from "jotai";

import type { OfferKind, OfferView } from "@/plan/api/schema";
import type { PlanStateView } from "@/plan/api/schema-files";
import { usePressureOptions } from "@/plan/chat/pressure/use-pressure";
import { offerAction } from "@/plan/offers/actions";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { authorBlockReasonAtom, checklistAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

export function PlanRunActions({ path, state }: { path: string; state: PlanStateView }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const options = usePressureOptions();
  const pressureReason =
    reason ??
    (options?.ok && !options.value.available
      ? (options.value.disabled_reason ?? "Pressure-testing is unavailable")
      : null);
  const checklist = useAtomValue(checklistAtom);
  const setPressure = useSetAtom(pressureDialogAtom);
  const setRun = useSetAtom(runDialogAtom);
  if (state === "running" || state === "retired") return null;
  if (state === "written") {
    return (
      <div className="flex flex-wrap gap-2">
        <DisabledReason
          reason={pressureReason}
          onClick={() => setPressure({ open: true, plan: path })}
        >
          Pressure-test
        </DisabledReason>
        <DisabledReason reason={reason} onClick={() => setRun({ open: true, plan: path })}>
          Start a run
        </DisabledReason>
      </div>
    );
  }
  const offers = [
    checklist?.primary,
    ...(checklist?.secondary ?? []),
    ...(checklist?.items.map((item) => item.offer) ?? []),
  ];
  const findOffer = (kind: OfferKind): OfferView | undefined =>
    offers.find(
      (offer): offer is OfferView =>
        offer?.kind === kind && (offer.target === path || offer.target === null),
    );
  const review = findOffer("refine_spec");
  const round = findOffer("new_metaplan_round");
  return (
    <div className="flex flex-wrap gap-2">
      <DisabledReason
        reason={reason ?? (review ? null : "No offer is available")}
        onClick={() => {
          if (review) offerAction("refine_spec")?.(review);
        }}
      >
        Review what was built
      </DisabledReason>
      <DisabledReason
        reason={reason ?? (round ? null : "No offer is available")}
        onClick={() => {
          if (round) offerAction("new_metaplan_round")?.(round);
        }}
      >
        Start a new metaplan round
      </DisabledReason>
    </div>
  );
}
