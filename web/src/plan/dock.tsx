import type { JSX } from "react";

import { ChatPanel, ChatRegistrations } from "@/plan/chat/panel";

/**
 * Right-hand dock beside the routed page: the chat panel (a docked region at lg and above, a
 * sheet below). The registrations mount here too so offers and Send to author work at every
 * width, whether the sheet is open or closed.
 */
export function Dock(): JSX.Element {
  return (
    <>
      <ChatRegistrations />
      <ChatPanel />
    </>
  );
}
