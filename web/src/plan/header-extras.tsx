import type { JSX } from "react";
import { useLocation } from "react-router";

import { ChatSheetButton } from "@/plan/chat/panel";
import { hasChatPanel } from "@/plan/dock";
import { HandoffMounts } from "@/plan/run/handoff-mounts";

/**
 * Header mounts: the run-handoff dialogs, and the chat sheet button on a route that mounts a chat
 * panel. The button hides itself while the side panel is shown (from 1280px, see `DESKTOP` in
 * chat/sheet-state.ts), which is a different width from the header's own 1024px layout switch. The
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
