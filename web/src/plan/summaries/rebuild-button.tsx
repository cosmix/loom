import { useAtomValue } from "jotai/react";
import { useRef, useState } from "react";

import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason, INLINE_BASELINE } from "@/plan/ui/disabled-reason";

import { useSummaryActions } from "./state";

export function RebuildButton({ ids }: { ids: string[] }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const { rebuild } = useSummaryActions();
  const pending = useRef(false);
  const [error, setError] = useState<string | null>(null);

  async function handleRebuild() {
    if (reason !== null || pending.current) return;
    pending.current = true;
    setError(null);
    try {
      setError(await rebuild(ids));
    } finally {
      pending.current = false;
    }
  }

  // One line beside the summary status: the button, then a blocked reason or a failure after it.
  // The wrapper stays: it holds the failure beside DisabledReason's own inline wrapper, so the
  // alert shares the baseline line instead of landing outside it.
  return (
    <span className={INLINE_BASELINE}>
      <DisabledReason
        reason={reason}
        size="sm"
        reasonPlacement="inline"
        onClick={() => void handleRebuild()}
      >
        Rebuild
      </DisabledReason>
      {error !== null && (
        <span role="alert" className="text-xs text-destructive">
          <Untrusted text={error} />
        </span>
      )}
    </span>
  );
}
