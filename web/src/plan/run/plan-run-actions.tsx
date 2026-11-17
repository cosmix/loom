import { useAtomValue, useSetAtom } from "jotai";

import { ConfigBlockNotice } from "@/plan/chat/pressure/config-block-notice";
import { usePressureOptions, useStartReason } from "@/plan/chat/pressure/use-pressure";
import { visibleControls } from "@/plan/chat/visible-controls";
import { pressureStatus, READ_PRESSURE_RECORD } from "@/plan/files/pressure-words";
import { pressureDialogAtom } from "@/plan/offers/actions-pressure";
import { runDialogAtom } from "@/plan/offers/actions-run";
import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { StateMenu, StateMenuItem, StateMenuLink } from "@/plan/trace/state-menu";
import { type ButtonLook, DisabledReason } from "@/plan/ui/disabled-reason";

import "./pressure-test-action.css";

/**
 * Why Start a run is blocked, or null: pressure start can take free authoring, but
 * /api/run/start and /api/run/commit need the holder, so a browser without it sees the reason.
 * `blockedBy` is the caller's own reason, such as a run holding the workspace; one reason is
 * drawn, the authoring one first.
 */
export function useStartRunReason(blockedBy: string | null = null): string | null {
  const reason = useAtomValue(authorBlockReasonAtom);
  const holder = useAtomValue(isHolderAtom);
  return (
    reason ?? blockedBy ?? (holder ? null : "Start a chat to take authoring, then start a run.")
  );
}

/**
 * The Start a run button for one plan (null opens the dialog on its own choice). By default it
 * draws its blocked reason under itself; a caller that draws the reason elsewhere (the run row of
 * a ledger) passes `reasonId` of the element holding it, and the button is described by it.
 */
export function StartRunButton({
  path,
  look,
  blockedBy = null,
  reasonId,
}: {
  path: string | null;
  look?: ButtonLook;
  blockedBy?: string | null;
  reasonId?: string;
}) {
  const setRun = useSetAtom(runDialogAtom);
  const runReason = useStartRunReason(blockedBy);
  return (
    <DisabledReason
      look={look}
      reason={runReason}
      reasonId={reasonId}
      onClick={() => setRun({ open: true, plan: path })}
    >
      Start a run
    </DisabledReason>
  );
}

/** Why the menu's own choice is blocked while a test runs on the plan. */
const TEST_RUNNING = "A pressure test is running on this plan.";

/**
 * The Pressure-test control of a written plan (DESIGN.md, Buttons): one state button, as the
 * settle button is. Its label is the plan's pressure state ("Not pressure-tested",
 * "Pressure-tested 5 Nov", "Being pressure-tested"), led by the state's toned glyph, and a press
 * opens a menu under it (`StateMenu`). The menu's choice opens the pressure dialog on the plan;
 * its note says so, after the record's date as sent. A blocked start, or a test already running,
 * disables the choice and draws the reason in the note's place. `recordHref` is where the plan's
 * pressure record reads, for a tested plan that has such a place: it adds a link to it.
 */
export function PressureTestAction(props: {
  path: string;
  date: string | null;
  active?: boolean;
  recordHref?: string | null;
}) {
  const { path, date, active = false, recordHref = null } = props;
  const options = usePressureOptions();
  const startReason = useStartReason(options?.ok ? options.value : null);
  const setPressure = useSetAtom(pressureDialogAtom);
  const { label, Icon, tone } = pressureStatus(date, active);
  return (
    <span className="pressure-test-action" data-tone={tone}>
      <StateMenu label={label} glyph={<Icon aria-hidden="true" />}>
        {(close) => (
          <>
            <StateMenuItem
              label={date === null ? "Pressure-test this plan" : "Pressure-test this plan again"}
              note={
                date === null
                  ? "Opens the pressure test, where you start it."
                  : `Tested ${visibleControls(date)}. Opens the pressure test, where you start it.`
              }
              reason={active ? TEST_RUNNING : startReason}
              onChoose={() => {
                close();
                setPressure({ open: true, plan: path });
              }}
            />
            {date !== null && recordHref !== null && (
              <StateMenuLink
                label={READ_PRESSURE_RECORD}
                note="The plan's Pressure tests heading, in its source."
                to={recordHref}
                onChoose={close}
              />
            )}
          </>
        )}
      </StateMenu>
    </span>
  );
}

/** The pressure row's detail line: the Codex configuration notice, drawn only while one blocks. */
export function PressureTestDetail() {
  return <ConfigBlockNotice context="plan" />;
}
