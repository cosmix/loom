import { useAtomValue, useSetAtom } from "jotai";

import type { ChecklistView, OfferKind, OfferView } from "@/plan/api/schema";
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

/**
 * The offer of `kind` the checklist makes for this plan. An offer with no target (`refine_spec`,
 * `new_metaplan_round`) is the feature's, so it applies only to a plan under that feature: the
 * checklist is the last visited feature's, and a standalone plan has none to be offered.
 */
export function findOffer(
  checklist: ChecklistView | null,
  kind: OfferKind,
  path: string,
): OfferView | undefined {
  const offers = [
    checklist?.primary,
    ...(checklist?.secondary ?? []),
    ...(checklist?.items.map((item) => item.offer) ?? []),
  ];
  const inFeature = checklist !== null && path.startsWith(`doc/plans/${checklist.feature}/`);
  return offers.find(
    (offer): offer is OfferView =>
      offer?.kind === kind && (offer.target === path || (offer.target === null && inFeature)),
  );
}

const BUILT_OFFERS: readonly { kind: OfferKind; label: string }[] = [
  { kind: "refine_spec", label: "Compare spec with build" },
  { kind: "new_metaplan_round", label: "New metaplan round" },
];

/** A done plan's follow-ups: a button only for an offer the server makes, nothing otherwise. */
function BuiltPlanActions({ path }: { path: string }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const checklist = useAtomValue(checklistAtom);
  const available = BUILT_OFFERS.flatMap(({ kind, label }) => {
    const offer = findOffer(checklist, kind, path);
    return offer ? [{ kind, label, offer }] : [];
  });
  if (available.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-2">
      {available.map(({ kind, label, offer }) => (
        <DisabledReason key={kind} reason={reason} onClick={() => offerAction(kind)?.(offer)}>
          {label}
        </DisabledReason>
      ))}
    </div>
  );
}
