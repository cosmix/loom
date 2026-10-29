import { useAtomValue, useStore } from "jotai";
import { useEffect, useState, type JSX } from "react";

import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import { fetchDashboard, runStripAtom, type RunDashboard } from "./use-run";

export function DashboardButton(): JSX.Element | null {
  const store = useStore();
  const strip = useAtomValue(runStripAtom);
  const holder = useAtomValue(isHolderAtom);
  const authorReason = useAtomValue(authorBlockReasonAtom);
  const [result, setResult] = useState<{
    dashboard: RunDashboard | null;
    holder: boolean;
    authorReason: string | null;
    error: string | null;
  } | null>(null);

  useEffect(() => {
    let active = true;
    void fetchDashboard(store).then((reply) => {
      if (!active) return;
      setResult((previous) => ({
        dashboard: reply.ok
          ? reply.value
          : previous?.holder === holder && previous.authorReason === authorReason
            ? previous.dashboard
            : null,
        holder,
        authorReason,
        error: reply.ok ? null : reply.message,
      }));
    });
    return () => {
      active = false;
    };
  }, [store, strip, holder, authorReason]);

  if (!result || result.holder !== holder || result.authorReason !== authorReason) return null;
  if (!result.dashboard?.url) {
    return result.error ? <LoadState state="error" message={result.error} /> : null;
  }
  return (
    <div className="space-y-1">
      <a href={result.dashboard.url} target="_blank" rel="noopener noreferrer">
        Open dashboard
      </a>
      {result.dashboard.tunnel && <p className="text-xs">{result.dashboard.tunnel}</p>}
      {result.error && <LoadState state="error" message={result.error} />}
    </div>
  );
}
