import { useAtomValue, useSetAtom } from "jotai";

import type { OfferKind, OfferView } from "@/plan/api/schema";
import type { PlanStateView } from "@/plan/api/schema-files";
import { ConfigBlockNotice } from "@/plan/chat/pressure/config-block-notice";
import { usePressureOptions, useStartReason } from "@/plan/chat/pressure/use-pressure";
import { offerAction } from "@/plan/offers/actions";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { authorBlockReasonAtom, checklistAtom, isHolderAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

export function PlanRunActions({ path, state }: { path: string; state: PlanStateView }) {
  if (state === "running" || state === "retired") return null;
  return state === "written" ? (
    <WrittenPlanActions path={path} />
  ) : (
    <BuiltPlanActions path={path} />
  );
}

function WrittenPlanActions({ path }: { path: string }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const holder = useAtomValue(isHolderAtom);
  const options = usePressureOptions();
  // Pressure start can take free authoring; /api/run/start and /api/run/commit need the holder.
  const runReason = reason ?? (holder ? null : "Start a chat to take authoring, then start a run.");
  const pressureReason = useStartReason(options?.ok ? options.value : null);
  const setPressure = useSetAtom(pressureDialogAtom);
  const setRun = useSetAtom(runDialogAtom);
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-2">
        <DisabledReason
          reason={pressureReason}
          onClick={() => setPressure({ open: true, plan: path })}
        >
          Pressure-test
        </DisabledReason>
        <DisabledReason reason={runReason} onClick={() => setRun({ open: true, plan: path })}>
          Start a run
        </DisabledReason>
      </div>
      <ConfigBlockNotice context="plan" />
    </div>
  );
}

function BuiltPlanActions({ path }: { path: string }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const checklist = useAtomValue(checklistAtom);
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
