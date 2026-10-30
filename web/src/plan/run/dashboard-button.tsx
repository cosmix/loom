import { useAtomValue, useStore } from "jotai";
import { useEffect, useState, type JSX } from "react";

import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import { fetchDashboard, runStripAtom, type RunDashboard } from "./use-run";

/**
 * `full` is the run dialog's link with the tunnel hint and any failure; `link` is the activity
 * popover's, which has room for neither (the dialog shows them).
 */
type Variant = "full" | "link";

// The server validates the URL; refuse any other scheme here as well before it becomes an href.
export const webUrl = (url: string | null | undefined) =>
  url && /^https?:\/\//i.test(url) ? url : null;

export function DashboardButton({ variant = "full" }: { variant?: Variant }): JSX.Element | null {
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
  const url = webUrl(result.dashboard?.url);
  if (!url) {
    return result.error && variant === "full" ? (
      <LoadState state="error" message={result.error} />
    ) : null;
  }
  return (
    <div className="space-y-1">
      <DashboardLink url={url} />
      {variant === "full" && result.dashboard?.tunnel && (
        <p className="text-xs [overflow-wrap:anywhere]">
          <Untrusted text={result.dashboard.tunnel} />
        </p>
      )}
      {variant === "full" && result.error && <LoadState state="error" message={result.error} />}
    </div>
  );
}

export function DashboardLink({ url }: { url: string }) {
  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      className="no-underline underline-offset-4 hover:underline"
    >
      Open dashboard
    </a>
  );
}
