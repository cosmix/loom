import { useAtomValue, useSetAtom } from "jotai";

import type { PlanStateView } from "@/plan/api/schema-files";
import { ConfigBlockNotice } from "@/plan/chat/pressure/config-block-notice";
import { usePressureOptions, useStartReason } from "@/plan/chat/pressure/use-pressure";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { type ButtonLook, DisabledReason } from "@/plan/ui/disabled-reason";

/**
 * A written plan's Pressure-test and Start a run. A done plan has nothing here.
 */
export function PlanRunActions({ path, state }: { path: string; state: PlanStateView }) {
  return state === "written" ? <WrittenPlanActions path={path} /> : null;
}

/**
 * The Start a run button for one plan (null opens the dialog on its own choice). Pressure start
 * can take free authoring, but /api/run/start and /api/run/commit need the holder, so a browser
 * without it sees the reason. `blockedBy` is the caller's own reason, such as a run holding the
 * workspace; the control draws one reason, the authoring one first.
 */
export function StartRunButton({
  path,
  look,
  blockedBy = null,
}: {
  path: string | null;
  look?: ButtonLook;
  blockedBy?: string | null;
}) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const holder = useAtomValue(isHolderAtom);
  const setRun = useSetAtom(runDialogAtom);
  const runReason =
    reason ?? blockedBy ?? (holder ? null : "Start a chat to take authoring, then start a run.");
  return (
    <DisabledReason
      look={look}
      reason={runReason}
      onClick={() => setRun({ open: true, plan: path })}
    >
      Start a run
    </DisabledReason>
  );
}

function WrittenPlanActions({ path }: { path: string }) {
  const options = usePressureOptions();
  const pressureReason = useStartReason(options?.ok ? options.value : null);
  const setPressure = useSetAtom(pressureDialogAtom);
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-2">
        <DisabledReason
          reason={pressureReason}
          onClick={() => setPressure({ open: true, plan: path })}
        >
          Pressure-test
        </DisabledReason>
        <StartRunButton path={path} />
      </div>
      <ConfigBlockNotice context="plan" />
    </div>
  );
}
