import { useAtomValue } from "jotai/react";
import { useRef, useState } from "react";

import { Untrusted } from "@/plan/chat/visible-controls";
import "@/plan/run/pressure-test-action.css";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { BusyText } from "@/plan/ui/busy-text";
import { type ButtonLook, DisabledReason, INLINE_BASELINE } from "@/plan/ui/disabled-reason";

import { summaryPanelAtom, useSummaryActions } from "./state";

/**
 * Whether the live summary panel has any of `ids` running or queued to start. An id whose
 * background rebuild waits for edits to stop is not: a press starts it now.
 */
function useQueued(ids: string[]): boolean {
  const panel = useAtomValue(summaryPanelAtom);
  if (panel === null) return false;
  const waiting = new Set(panel.waiting);
  const starting = panel.queue.filter((id) => !waiting.has(id));
  const queued = new Set([...(panel.running?.ids ?? []), ...starting]);
  return ids.some((id) => queued.has(id));
}

/**
 * Rebuild, or "Rebuilding…" with the arrow spinner and inert while the request is in flight, the
 * page reports its summary rebuilding, or the live panel runs or queues one of `ids`. The panel's
 * push that drops the id turns it back into Rebuild.
 */
export function RebuildButton({
  ids,
  look,
  rebuilding = false,
}: {
  ids: string[];
  look?: ButtonLook;
  rebuilding?: boolean;
}) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const { rebuild } = useSummaryActions();
  const pending = useRef(false);
  const [requesting, setRequesting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const queued = useQueued(ids);
  const busy = requesting || rebuilding || queued;

  async function handleRebuild() {
    if (reason !== null || pending.current) return;
    pending.current = true;
    setRequesting(true);
    setError(null);
    try {
      setError(await rebuild(ids));
    } finally {
      pending.current = false;
      setRequesting(false);
    }
  }

  // One line beside the summary status: the button, then a blocked reason or a failure after it.
  // The wrapper stays: it holds the failure beside DisabledReason's own inline wrapper, so the
  // alert shares the baseline line instead of landing outside it.
  return (
    <span className={`rebuild-action ${INLINE_BASELINE}`}>
      <DisabledReason
        reason={reason}
        busy={busy}
        size="sm"
        look={look}
        reasonPlacement="inline"
        onClick={() => void handleRebuild()}
      >
        {busy ? <BusyText>Rebuilding…</BusyText> : "Rebuild"}
      </DisabledReason>
      {error !== null && (
        <span role="alert" className="text-xs text-destructive">
          <Untrusted text={error} />
        </span>
      )}
    </span>
  );
}
