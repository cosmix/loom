import { useAtomValue } from "jotai";
import { MessageSquare } from "lucide-react";

import type { SessionView } from "@/plan/api/schema-chat";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { pendingPermissionsAtom } from "./model";
import { shownSessionAtom } from "./shown-session";

export const STATE_NAME: Record<SessionView["state"], string> = {
  starting: "Starting session",
  idle: "Waiting for you",
  working: "Working",
  permission: "Needs a decision",
  ending: "Writing final notes",
  ended: "Chat ended",
  failed: "Chat failed",
};
export const BUSY_STATES: ReadonlySet<SessionView["state"]> = new Set([
  "starting",
  "working",
  "ending",
]);

export function ChatGlyph() {
  const session = useAtomValue(shownSessionAtom);
  const permissions = useAtomValue(pendingPermissionsAtom);
  const state = session?.state;
  const label = state ? STATE_NAME[state] : "No chat";
  if (permissions.length || state === "permission")
    return (
      <span role="img" aria-label={STATE_NAME.permission} className="text-(--tone-warning)">
        <ActivityGlyph kind="permission" />
      </span>
    );
  if (state === "failed" || state === "ended")
    return (
      <span role="img" aria-label={label}>
        {state === "failed" ? "!" : "✓"}
      </span>
    );
  if (state !== undefined && BUSY_STATES.has(state))
    return (
      <span className="inline-flex">
        <ActivityGlyph kind={state === "starting" ? "starting" : "drafting"} />
        <span role="status" aria-live="polite" className="sr-only">
          {label}
        </span>
      </span>
    );
  // An idle chat, or no chat, is a plain chat glyph: the activity glyph means work in progress.
  return (
    <span role="img" aria-label={label} className="inline-flex">
      <MessageSquare size={16} aria-hidden="true" />
    </span>
  );
}
