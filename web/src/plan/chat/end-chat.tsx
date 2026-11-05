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

/**
 * End chat as one full-width row of the chat menu: a quiet button, left-aligned, its own hover.
 * `onEnded` runs only after the end command succeeded; a failure or a blocked click shows its
 * text here and calls nothing, so the menu holding it stays open.
 */
export function EndChatMenuItem({ onEnded }: { onEnded?: () => void }) {
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
      .then(
        () => onEnded?.(),
        (error: unknown) => setError(error instanceof Error ? error.message : String(error)),
      )
      .finally(() => setPending(false));
  };
  return (
    <div className="chat-menu-item">
      <DisabledReason
        reason={
          reason ??
          (pending || session.state === "ending" ? "Writing final notes into the documents" : null)
        }
        onClick={end}
        look="quiet"
      >
        End chat
      </DisabledReason>
      {error && (
        <p role="status" className="plan-fine">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}
