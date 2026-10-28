import type { JSX } from "react";

import type { EndedChat, SessionView } from "@/plan/api/schema-chat";

const HOUR_MINUTE = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});

// `next` is part of the divider's contract (the next session, if any) but no wording depends on
// it yet; revisit when a session kind needs its own divider text.
export function SessionDivider({
  previous,
}: {
  previous: EndedChat;
  next: SessionView | null;
}): JSX.Element {
  const since = previous.session.state_since;
  const known = !Number.isNaN(Date.parse(since));
  return (
    <div className="py-3 text-center text-xs text-muted-foreground">
      {known ? `Chat ended at ${HOUR_MINUTE.format(new Date(since))}` : "Chat ended"}
    </div>
  );
}
