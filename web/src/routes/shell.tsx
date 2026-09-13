import { useAtomValue } from "jotai/react";
import { useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";

import { ErrorBoundary } from "@/aurora-ui/feedback/ErrorBoundary";

import { Header } from "@/components/header";
import { useNow } from "@/components/hooks/use-now";
import { LegendDialog } from "@/components/legend-dialog";
import { QuotaMeters } from "@/components/quota-meters";
import { SettingsDialog } from "@/components/settings-dialog";
import { StageModal } from "@/components/stage-modal";
import { TooltipProvider } from "@/components/ui/tooltip";
import { providerRows } from "@/lib/quota";
import { snapshotAtom } from "@/state/atoms";

/// Header, routed body, footer, the stage dialog (`?stage=<id>` on any
/// route), the settings dialog (`?settings=1`), and the legend dialog
/// with its `?` shortcut. Each route sets its
/// own width: the overview runs wide, the ledger and stage pages are bounded.
export function Shell() {
  const [legendOpen, setLegendOpen] = useState(false);
  const { pathname } = useLocation();

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
      <div className="flex h-dvh flex-col">
        <Header onOpenLegend={() => setLegendOpen(true)} />
        <main className="flex min-h-0 w-full flex-1 flex-col overflow-y-auto">
          <ErrorBoundary resetKey={pathname}>
            <Outlet />
          </ErrorBoundary>
        </main>
        <Footer />
        <StageModal />
        <SettingsDialog />
        <LegendDialog open={legendOpen} onOpenChange={setLegendOpen} />
      </div>
    </TooltipProvider>
  );
}

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
}

/// Sticky provider quota gauges. The header owns legend access and feed status,
/// so there is no footer when neither provider has quota data.
function Footer() {
  const snapshot = useAtomValue(snapshotAtom);
  const now = useNow();
  const nowSecs = Math.floor(now / 1000);
  const quota = snapshot?.status.quota ?? null;
  const hasQuota = quota !== null && providerRows(quota).length > 0;
  if (!hasQuota || quota === null) return null;

  return (
    <footer className="z-10 border-t border-border bg-background/90 backdrop-blur-sm">
      <div className="mx-auto flex w-full max-w-[1440px] px-4 py-3 text-xs text-muted-foreground sm:px-6">
        <QuotaMeters snapshot={quota} nowSecs={nowSecs} />
      </div>
    </footer>
  );
}
