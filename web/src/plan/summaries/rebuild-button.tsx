import { useAtomValue } from "jotai/react";
import { useRef, useState } from "react";

import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

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

  return (
    <div className="flex flex-col items-start gap-1">
      <DisabledReason reason={reason} size="sm" onClick={() => void handleRebuild()}>
        Rebuild
      </DisabledReason>
      {error !== null && (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
