import { useAtomValue, useSetAtom } from "jotai";

import { ConfigBlockNotice } from "@/plan/chat/pressure/config-block-notice";
import { usePressureOptions, useStartReason } from "@/plan/chat/pressure/use-pressure";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { type ButtonLook, DisabledReason } from "@/plan/ui/disabled-reason";

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

/**
 * The Pressure-test control at the end of a written plan's pressure status row (`pressureChip`'s
 * `action`, DESIGN.md, Status). A blocked start draws its reason beside the button, on the row.
 */
export function PressureTestAction({ path }: { path: string }) {
  const options = usePressureOptions();
  const reason = useStartReason(options?.ok ? options.value : null);
  const setPressure = useSetAtom(pressureDialogAtom);
  return (
    <DisabledReason
      look="quiet"
      size="sm"
      reasonPlacement="inline"
      reason={reason}
      onClick={() => setPressure({ open: true, plan: path })}
    >
      Pressure-test
    </DisabledReason>
  );
}

/** The pressure row's detail line: the Codex configuration notice, drawn only while one blocks. */
export function PressureTestDetail() {
  return <ConfigBlockNotice context="plan" />;
}
