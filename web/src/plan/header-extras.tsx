import type { JSX } from "react";

import { ChatSheetButton } from "@/plan/chat/panel";
import { ChangedSinceControl } from "@/plan/review/changed-since";
import { HandoffMounts } from "@/plan/run/handoff-mounts";
import { RunStrip } from "@/plan/run/run-strip";
import { SummaryStatus } from "@/plan/summaries/summary-status";

/**
 * Header slot for controls later plans add (plans 9, 10 and 11 each add one
 * line rendering their control). `compact` is true below the `lg` breakpoint:
 * a control then renders its compact form, glyph plus count.
 */
export function HeaderExtras({ compact }: { compact: boolean }): JSX.Element {
  return (
    <>
      <HandoffMounts />
      <RunStrip compact={compact} />
      <ChangedSinceControl compact={compact} />
      <SummaryStatus compact={compact} />
      {compact ? <ChatSheetButton /> : null}
    </>
  );
}
