import { useAtomValue } from "jotai/react";
import type { JSX } from "react";

import type { EndedChat, SessionView } from "@/plan/api/schema-chat";
import { stepForSession, stepLabel } from "./pressure/steps";
import { pressureRunAtom } from "./pressure/use-pressure";

const HOUR_MINUTE = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});

/** The break between an ended chat and the next: a centred muted line between two hairlines. */
export function SessionDivider({
  previous,
  next,
}: {
  previous: EndedChat;
  next: SessionView | null;
}): JSX.Element {
  const run = useAtomValue(pressureRunAtom);
  const step =
    previous.session.kind === "pressure" && next?.kind === "pressure"
      ? stepForSession(run, next.id)
      : null;
  const since = previous.session.state_since;
  const known = !Number.isNaN(Date.parse(since));
  return (
    <div className="chat-session-divider">
      <span>
        {step
          ? `${stepLabel(step.kind)} · ${step.model}`
          : known
            ? `Chat ended at ${HOUR_MINUTE.format(new Date(since))}`
            : "Chat ended"}
      </span>
    </div>
  );
}
