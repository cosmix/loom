import type { JSX } from "react";

import { ChatPanel, ChatRegistrations } from "@/plan/chat/panel";

/**
 * Right-hand dock beside the routed page: the chat panel (a docked region at lg and above, a
 * sheet below). The registrations mount here too so offers, Send to author, the permission and
 * conclusion announcements and the Brief dialog work at every width, whether the panel is
 * collapsed or the sheet is open or closed.
 */
export function Dock(): JSX.Element {
  return (
    <>
      <ChatRegistrations />
      <ChatPanel />
    </>
  );
}
