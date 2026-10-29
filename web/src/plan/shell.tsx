import { useAtomValue } from "jotai/react";
import { useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";

import { ErrorBoundary } from "@/aurora-ui/feedback/ErrorBoundary";
import { TooltipProvider } from "@/components/ui/tooltip";
import { isTyping } from "@/lib/is-typing";
import { Dock } from "@/plan/dock";
import { Banners } from "@/plan/header/banners";
import { PlanHeader } from "@/plan/header/header";
import { PlannerHelpDialog } from "@/plan/help/help-dialog";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";
import { SettingsDialog } from "@/plan/settings/settings-dialog";
import { unauthorizedAtom } from "@/plan/state/atoms";
import { Unauthorized } from "@/plan/unauthorized";

/** Header, banners, routed page and dock; a 401 replaces all of it with the sign-in notice. */
export function PlanShell() {
  const unauthorized = useAtomValue(unauthorizedAtom);
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  const [helpOpen, setHelpOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { pathname } = useLocation();
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "?" || event.altKey || event.ctrlKey || event.metaKey) return;
      if (isTyping(event.target)) return;
      event.preventDefault();
      setHelpOpen((open) => !open);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
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
            <Banners />
            <div className="flex min-h-0 flex-1">
              <main className="min-w-0 flex-1 overflow-y-auto">
                <ErrorBoundary resetKey={pathname}>
                  <Outlet />
                </ErrorBoundary>
              </main>
              <Dock />
            </div>
            <PlannerHelpDialog open={helpOpen} onOpenChange={setHelpOpen} />
            <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
          </>
        )}
      </div>
    </TooltipProvider>
  );
}
