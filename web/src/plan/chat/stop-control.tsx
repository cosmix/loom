import { useAtomValue, useStore } from "jotai";
import { useEffect, useState } from "react";

import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { interruptChat } from "./commands";
import { forgetGiven } from "./context-attach";
import { draftAtom } from "./drafts";
import { queueReturnedAtom, sessionAtom } from "./model";
import { Untrusted } from "./visible-controls";

/**
 * Moves the texts the server returns after Stop or End chat into the draft. It is mounted with the
 * chat registrations, so it runs while the composer is replaced by the concluded-chat view.
 */
export function useQueueRefill(): void {
  const store = useStore();
  useEffect(() => {
    const refill = () => {
      const texts = store.get(queueReturnedAtom);
      if (!texts.length) return;
      store.set(queueReturnedAtom, []);
      forgetGiven(store);
      store.set(draftAtom, (draft) => ({
        ...draft,
        text: [draft.text, ...texts].filter((text) => text.length > 0).join("\n\n"),
      }));
    };
    const unsubscribe = store.sub(queueReturnedAtom, refill);
    refill();
    return unsubscribe;
  }, [store]);
}

/**
 * Stop, in the composer's action row immediately before Send and at Send's size, while a turn
 * runs or waits on a decision. Its error sits under it, in the row's reason size.
 */
export function StopControl() {
  const store = useStore();
  const session = useAtomValue(sessionAtom);
  const isHolder = useAtomValue(isHolderAtom);
  const reason = useAtomValue(authorBlockReasonAtom);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  if (!isHolder || (session?.state !== "working" && session?.state !== "permission")) return null;
  const stop = () => {
    setError(null);
    setPending(true);
    void interruptChat(store)
      .catch((err: unknown) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setPending(false));
  };
  return (
    <div className="chat-composer-stop">
      <DisabledReason
        reason={reason ?? (pending ? "Stopping the current turn" : null)}
        onClick={stop}
      >
        Stop
      </DisabledReason>
      {error && (
        <p role="status" className="plan-fine">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}
