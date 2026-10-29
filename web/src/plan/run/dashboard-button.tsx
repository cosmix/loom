import { useAtomValue, useStore } from "jotai";
import { ExternalLinkIcon } from "lucide-react";
import { useEffect, useState, type JSX } from "react";

import { buttonVariants } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom, isHolderAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import { fetchDashboard, runStripAtom, type RunDashboard } from "./use-run";

/**
 * `full` is the run dialog's link with the tunnel hint and any failure; `link` and `icon` are the
 * header's, which have no room for either (the dialog shows them), `icon` being the narrow form.
 */
type Variant = "full" | "link" | "icon";

// The server validates the URL; refuse any other scheme here as well before it becomes an href.
const webUrl = (url: string | undefined) => (url && /^https?:\/\//i.test(url) ? url : null);

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
      <DashboardLink url={url} icon={variant === "icon"} />
      {variant === "full" && result.dashboard?.tunnel && (
        <p className="text-xs [overflow-wrap:anywhere]">
          <Untrusted text={result.dashboard.tunnel} />
        </p>
      )}
      {variant === "full" && result.error && <LoadState state="error" message={result.error} />}
    </div>
  );
}

function DashboardLink({ url, icon }: { url: string; icon: boolean }) {
  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={icon ? "Open dashboard" : undefined}
      title={icon ? "Open dashboard" : undefined}
      className={
        icon
          ? buttonVariants({ variant: "ghost", size: "icon-sm" })
          : "underline underline-offset-4"
      }
    >
      {icon ? <ExternalLinkIcon aria-hidden="true" /> : "Open dashboard"}
    </a>
  );
}
