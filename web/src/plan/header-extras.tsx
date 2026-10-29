import type { JSX } from "react";

import { ChatSheetButton } from "@/plan/chat/panel";
import { ChangedSinceControl } from "@/plan/review/changed-since";
import { HandoffMounts } from "@/plan/run/handoff-mounts";
import { RunStrip } from "@/plan/run/run-strip";
import { SummaryStatus } from "@/plan/summaries/summary-status";

/**
 * Header controls use their compact form below the `lg` breakpoint. SummaryStatus
 * always stays compact to retain the rebuild popover alongside the activity pill.
 */
export function HeaderExtras({ compact }: { compact: boolean }): JSX.Element {
  return (
    <>
      <HandoffMounts />
      <RunStrip compact={compact} />
      <ChangedSinceControl compact={compact} />
      <SummaryStatus compact />
      {compact ? <ChatSheetButton /> : null}
    </>
  );
}
