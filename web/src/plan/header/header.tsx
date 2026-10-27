import { useAtomValue } from "jotai/react";

import { HeaderExtras } from "@/plan/header-extras";
import { useMediaQuery } from "@/lib/use-media-query";
import { useResendStoredName } from "@/plan/header/name-prompt";
import { Overflow } from "@/plan/header/overflow";
import { Presence } from "@/plan/header/presence";
import { Share } from "@/plan/header/share";
import { StepTracker } from "@/plan/header/step-tracker";
import { Tabs } from "@/plan/header/tabs";
import { checklistAtom } from "@/plan/state/atoms";

/** Below this the tracker collapses and presence and Share move to the overflow menu. */
const WIDE_QUERY = "(min-width: 1024px)";

export function PlanHeader() {
  const narrow = !useMediaQuery(WIDE_QUERY);
  const checklist = useAtomValue(checklistAtom);
  useResendStoredName();
  return (
    <header className="plan-header flex h-12 shrink-0 items-center gap-3 border-b border-hairline bg-card px-3">
      <span className="font-display text-base">loom plan</span>
      <StepTracker checklist={checklist} narrow={narrow} />
      <Tabs />
      <div className="ml-auto flex shrink-0 items-center gap-2">
        <HeaderExtras compact={narrow} />
        {narrow ? (
          <Overflow />
        ) : (
          <>
            <Presence />
            <Share />
          </>
        )}
      </div>
    </header>
  );
}
