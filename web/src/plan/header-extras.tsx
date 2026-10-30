import type { JSX } from "react";

import { ChatSheetButton } from "@/plan/chat/panel";
import { HandoffMounts } from "@/plan/run/handoff-mounts";

/**
 * Header mounts: the run-handoff dialogs, and below `lg` the chat sheet button. The run, review
 * and summary status live in the activity popover, not the header.
 */
export function HeaderExtras({ compact }: { compact: boolean }): JSX.Element {
  return (
    <>
      <HandoffMounts />
      {compact ? <ChatSheetButton /> : null}
    </>
  );
}
