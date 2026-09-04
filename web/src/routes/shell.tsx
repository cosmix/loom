import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { useEffect, useState } from "react";
import { Outlet } from "react-router";

import { Header } from "@/components/header";
import { useNow } from "@/components/hooks/use-now";
import { LegendDialog } from "@/components/legend-dialog";
import { QuotaMeters } from "@/components/quota-meters";
import { Kbd } from "@/components/ui/kbd";
import { TooltipProvider } from "@/components/ui/tooltip";
import { providerRows } from "@/lib/quota";
import { snapshotAtom } from "@/state/atoms";

/// Header, routed body, footer, and the legend dialog with its `?` shortcut.
export function Shell() {
  const [legendOpen, setLegendOpen] = useState(false);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "?" || event.altKey || event.ctrlKey || event.metaKey) return;
      if (isTyping(event.target)) return;
      event.preventDefault();
      setLegendOpen((open) => !open);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <TooltipProvider delayDuration={250}>
      <div className="flex min-h-dvh flex-col">
        <Header onOpenLegend={() => setLegendOpen(true)} />
        <main className="mx-auto w-full max-w-[1440px] flex-1 px-4 py-5 sm:px-6">
          <Outlet />
        </main>
        <Footer />
        <LegendDialog open={legendOpen} onOpenChange={setLegendOpen} />
      </div>
    </TooltipProvider>
  );
}

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/// Sticky status bar: provider quota gauges on the left, the legend hint and
/// the snapshot's source and time on the right. Without quota data it is the
/// plain one-line footer with the hint on the left.
function Footer() {
  const snapshot = useAtomValue(snapshotAtom);
  const nowSecs = Math.floor(useNow(30_000) / 1000);
  const quota = snapshot?.status.quota ?? null;
  const hasQuota = quota !== null && providerRows(quota).length > 0;
  return (
    <footer className="sticky bottom-0 z-10 border-t border-border bg-background/90 backdrop-blur-sm">
      <div className="mx-auto flex w-full max-w-[1440px] flex-wrap items-center gap-x-4 gap-y-1 px-4 py-3 text-xs text-muted-foreground sm:px-6">
        {quota && (
          <QuotaMeters snapshot={quota} nowSecs={nowSecs} className="max-[899px]:basis-full" />
        )}
        <span
          className={cn(
            "inline-flex items-center gap-4",
            hasQuota ? "ml-auto" : "flex-1 justify-between",
          )}
        >
          <span className="inline-flex items-center gap-1.5">
            <Kbd>?</Kbd> legend
          </span>
          {snapshot && (
            <span className="font-mono tabular-nums">
              {snapshot.source} · {new Date(snapshot.generated_at).toLocaleTimeString()}
            </span>
          )}
        </span>
      </div>
    </footer>
  );
}
