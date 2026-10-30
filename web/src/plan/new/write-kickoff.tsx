import { useAtomValue, useStore } from "jotai";
import { Feather } from "lucide-react";
import { useRef, useState } from "react";

import type { KickoffSize } from "@/plan/api/schema-kickoff";
import { sendMessage } from "@/plan/chat/commands";
import { sessionAtom } from "@/plan/chat/model";
import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom, canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

function useKickoffWrite(size: KickoffSize | null, target: string | null) {
  const store = useStore();
  const canAuthor = useAtomValue(canAuthorAtom);
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const session = useAtomValue(sessionAtom);
  const inFlight = useRef(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const label = size === "plan" ? "Write the plan" : "Write the vision";
  const reason = !canAuthor
    ? "Writing requires authoring"
    : target === null
      ? "Settle the size and name first"
      : session?.state !== "idle"
        ? "Wait for the current turn to finish"
        : pending
          ? "Sending the write request"
          : null;
  const write = async () => {
    if (reason !== null || !store.get(canAuthorAtom) || inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      await sendMessage(store, `${label}.`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  };
  return { label, reason, authorReason: canAuthor ? null : authorReason, error, write };
}

/** The large write button; disabled until the conversation has settled a target. */
export function WriteKickoff({
  size,
  target,
}: {
  size: KickoffSize | null;
  target: string | null;
}) {
  const { label, reason, authorReason, error, write } = useKickoffWrite(size, target);
  return (
    <div className="kickoff-write">
      <DisabledReason reason={reason} look="primary" onClick={() => void write()} size="lg">
        <Feather aria-hidden="true" />
        {label}
      </DisabledReason>
      {authorReason !== null && (
        <p className="kickoff-muted">
          <Untrusted text={authorReason} />
        </p>
      )}
      {error !== null && (
        <p role="status">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}
