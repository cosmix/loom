import { useAtomValue } from "jotai/react";
import { lazy, useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";

import { ErrorBoundary } from "@/aurora-ui/feedback/ErrorBoundary";
import { TooltipProvider } from "@/components/ui/tooltip";
import { isTyping } from "@/lib/is-typing";
import { Dock } from "@/plan/dock";
import { Banners } from "@/plan/header/banners";
import { PlanHeader } from "@/plan/header/header";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";
import { SettingsDialog } from "@/plan/settings/settings-dialog";
import { PlanFooter } from "@/plan/shell-footer";
import { unauthorizedAtom } from "@/plan/state/atoms";
import { SummaryFeedHost, useAutoRebuildSync } from "@/plan/summaries/state";
import { OptionalChunk } from "@/plan/ui/optional-chunk";
import { Unauthorized } from "@/plan/unauthorized";

// Help is a separate chunk, fetched after the shell renders: its topics and scenes kept plan.js
// over vite's 500 kB chunk warning. Revisit if help must be available offline from the start.
const PlannerHelpDialog = lazy(() =>
  import("@/plan/help/help-dialog").then((module) => ({ default: module.PlannerHelpDialog })),
);

/** The help dialog's open state, toggled by `?` outside typing targets and without modifiers. */
function useHelpKey() {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "?" || event.altKey || event.ctrlKey || event.metaKey) return;
      if (isTyping(event.target)) return;
      event.preventDefault();
      setOpen((current) => !current);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  return [open, setOpen] as const;
}

/** Header, routed page, dock, banners and footer; a 401 replaces all of it with the sign-in notice. */
export function PlanShell() {
  useAutoRebuildSync();
  const unauthorized = useAtomValue(unauthorizedAtom);
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  const [helpOpen, setHelpOpen] = useHelpKey();
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { pathname } = useLocation();
  return (
    <TooltipProvider delayDuration={250}>
      <div
        data-loom-plan
        data-reading-face={face}
        data-reading-size={String(size)}
        className="flex h-dvh flex-col"
      >
        {unauthorized ? (
          <Unauthorized />
        ) : (
          <>
            <PlanHeader
              onOpenSettings={() => setSettingsOpen(true)}
              onOpenHelp={() => setHelpOpen(true)}
            />
            <div className="flex min-h-0 flex-1">
              {/* Positioned: the page's sr-only text and other absolute content takes <main> as
                  its containing block and is clipped by its scroll, never the page's. */}
              <main className="relative min-w-0 flex-1 overflow-y-auto">
                <ErrorBoundary resetKey={pathname}>
                  <Outlet />
                </ErrorBoundary>
              </main>
              <Dock />
            </div>
            <Banners />
            <PlanFooter />
            <SummaryFeedHost />
            <OptionalChunk>
              <PlannerHelpDialog open={helpOpen} onOpenChange={setHelpOpen} />
            </OptionalChunk>
            <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
          </>
        )}
      </div>
    </TooltipProvider>
  );
}
