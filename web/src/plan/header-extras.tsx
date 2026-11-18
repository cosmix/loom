import type { JSX } from "react";
import { useLocation } from "react-router";

import { ChatSheetButton } from "@/plan/chat/panel-collapse";
import { hasChatPanel } from "@/plan/dock";
import { HandoffMounts } from "@/plan/run/handoff-mounts";

/**
 * Header mounts: the run-handoff dialogs, and the chat sheet button on a route that mounts a chat
 * panel. From 1280px (`DESKTOP` in chat/sheet-state.ts) it is always shown and toggles the side panel
 * (pressed while the panel is open); that is a different width from the header's own 1024px layout switch. The
 * run, review and summary status live in the activity popover, not the header.
 */
export function HeaderExtras(): JSX.Element {
  const { pathname } = useLocation();
  return (
    <>
      <HandoffMounts />
      {hasChatPanel(pathname) ? <ChatSheetButton /> : null}
    </>
  );
}
