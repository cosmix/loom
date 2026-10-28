import type { JSX } from "react";

import { ChatSheetButton } from "@/plan/chat/panel";
import { ChangedSinceControl } from "@/plan/review/changed-since";

/**
 * Header slot for controls later plans add (plans 9, 10 and 11 each add one
 * line rendering their control). `compact` is true below the `lg` breakpoint:
 * a control then renders its compact form, glyph plus count.
 */
export function HeaderExtras({ compact }: { compact: boolean }): JSX.Element {
  return (
    <>
      <ChangedSinceControl compact={compact} />
      {compact ? <ChatSheetButton /> : null}
    </>
  );
}
