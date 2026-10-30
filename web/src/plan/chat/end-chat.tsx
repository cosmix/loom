import { useAtomValue, useStore } from "jotai";
import { useState } from "react";

import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { endChat } from "./commands";
import { isActiveSession, panelSessionAtom } from "./session-home";
import { Untrusted } from "./visible-controls";

/**
 * Whether this browser may end the chat the panel shows: never one that belongs to another screen,
 * which the panel presents as no chat. Pressure-test sessions end on their own.
 */
export function useCanEndChat(): boolean {
  const session = useAtomValue(panelSessionAtom);
  const holder = useAtomValue(isHolderAtom);
  return holder && isActiveSession(session) && session.kind !== "pressure";
}

export function EndChatMenuItem() {
  const store = useStore();
  const session = useAtomValue(panelSessionAtom);
  const canEnd = useCanEndChat();
  const reason = useAtomValue(authorBlockReasonAtom);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!canEnd || !session) return null;
  const end = () => {
    if (
      pending ||
      !window.confirm(
        "End this chat? The session will write its open questions and anything pending into the documents, then stop.",
      )
    )
      return;
    setPending(true);
    setError(null);
    void endChat(store)
      .catch((error: unknown) => setError(error instanceof Error ? error.message : String(error)))
      .finally(() => setPending(false));
  };
  return (
    <div className="text-[13px]">
      <DisabledReason
        reason={
          reason ??
          (pending || session.state === "ending" ? "Writing final notes into the documents" : null)
        }
        onClick={end}
        size="sm"
      >
        End chat
      </DisabledReason>
      {error && (
        <p role="status">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}
