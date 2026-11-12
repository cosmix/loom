import type { JSX } from "react";

import { PressureDialog } from "@/plan/chat/pressure/pressure-dialog";
import { RunDialog } from "@/plan/run/run-dialog";

/** The pressure and run dialogs; `handoff-mounts.tsx` loads this module as its own chunk. */
export function HandoffDialogs(): JSX.Element {
  return (
    <>
      <PressureDialog />
      <RunDialog />
    </>
  );
}
