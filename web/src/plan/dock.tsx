import type { JSX } from "react";
import { useLocation } from "react-router";

import { ChatPanel, ChatRegistrations } from "@/plan/chat/panel";

/** The home screen and the kickoff have no chat panel; the narrow chat button follows the same rule. */
export function hasChatPanel(pathname: string): boolean {
  return pathname !== "/" && pathname !== "/new";
}

/**
 * Right-hand dock beside the routed page: the chat panel (a docked region at 1280px and above, a
 * sheet below), except on the home screen and the kickoff. The registrations mount here too so
 * offers, Send to author, the permission and conclusion announcements and the Brief dialog work
 * at every width. The panel is always shown at 1280px and wider; below it the sheet sits behind
 * the header's Chat button and may be open or closed. Where no panel
 * mounts, the registrations also own the chat socket and attention watcher, except on `/new`,
 * whose kickoff route opens its own socket, and the `c` shortcut has no composer to focus.
 */
export function Dock(): JSX.Element {
  const { pathname } = useLocation();
  const panel = hasChatPanel(pathname);
  return (
    <>
      <ChatRegistrations panel={panel} connect={!panel && pathname !== "/new"} />
      {/* Kickoff owns the transcript and replaces the chat panel with Taking shape. */}
      {panel && <ChatPanel />}
    </>
  );
}
