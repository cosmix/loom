import { useAtomValue } from "jotai/react";
import { useEffect, useState } from "react";
import { Outlet } from "react-router";

import { Header } from "@/components/header";
import { LegendDialog } from "@/components/legend-dialog";
import { Kbd } from "@/components/ui/kbd";
import { TooltipProvider } from "@/components/ui/tooltip";
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

function Footer() {
  const snapshot = useAtomValue(snapshotAtom);
  return (
    <footer className="mx-auto flex w-full max-w-[1440px] flex-wrap items-center gap-x-4 gap-y-1 px-4 py-3 text-xs text-muted-foreground sm:px-6">
      <span className="inline-flex items-center gap-1.5">
        <Kbd>?</Kbd> legend
      </span>
      {snapshot && (
        <span className="ml-auto font-mono tabular-nums">
          {snapshot.source} · {new Date(snapshot.generated_at).toLocaleTimeString()}
        </span>
      )}
    </footer>
  );
}
