import { useAtomValue, useStore } from "jotai/react";
import { Globe } from "lucide-react";
import { useState, type JSX } from "react";

import type { SessionView } from "@/plan/api/schema-chat";
import { isHolderAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { askFetchesAgain } from "./commands";

/**
 * The session's standing fetch permission, granted from a card ("Allow this session"): one quiet
 * line under the head while the session runs, with Ask again for the holder. Settings' Never
 * still refuses every fetch; the line says what the card granted, not what Settings decide.
 */
export function FetchGrantLine({ session }: { session: SessionView }): JSX.Element | null {
  const store = useStore();
  const isHolder = useAtomValue(isHolderAtom);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const over = session.state === "ended" || session.state === "failed";
  if (!session.fetch_grant || over) return null;
  async function askAgain() {
    setBusy(true);
    setError(null);
    try {
      const result = await askFetchesAgain(store);
      if (!result.ok) setError(result.error ?? "The change was refused");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="chat-fetch-grant">
      <div className="chat-fetch-grant-line">
        <Globe size={14} aria-hidden="true" />
        <span>Web fetches allowed for this session</span>
        {isHolder && (
          <DisabledReason
            size="sm"
            look="quiet"
            reason={null}
            busy={busy}
            onClick={() => void askAgain()}
          >
            Ask again
          </DisabledReason>
        )}
      </div>
      {error && <ActionError message={error} />}
    </div>
  );
}
