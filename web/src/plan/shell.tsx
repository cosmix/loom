import { useAtomValue } from "jotai/react";
import { useEffect, useRef, useState } from "react";
import { Outlet, useLocation } from "react-router";

import { ErrorBoundary } from "@/aurora-ui/feedback/ErrorBoundary";
import { ShortcutGroups } from "@/components/help-shortcuts";
import { isTyping } from "@/lib/is-typing";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Dock } from "@/plan/dock";
import { Banners } from "@/plan/header/banners";
import { PlanHeader } from "@/plan/header/header";
import { PLAN_HELP_KEYS } from "@/plan/help-keys";
import { unauthorizedAtom } from "@/plan/state/atoms";
import { Unauthorized } from "@/plan/unauthorized";

function PlanHelp({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  // The dialog has no trigger (the `?` key opens it), so Radix has nothing to return focus to.
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="sm:max-w-xl"
        onOpenAutoFocus={() => {
          returnFocus.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocus.current?.focus();
        }}
      >
        <DialogHeader>
          <DialogTitle>Help</DialogTitle>
          <DialogDescription>Keyboard shortcuts</DialogDescription>
        </DialogHeader>
        <ShortcutGroups groups={PLAN_HELP_KEYS} />
      </DialogContent>
    </Dialog>
  );
}

/** Header, banners, routed page and dock; a 401 replaces all of it with the sign-in notice. */
export function PlanShell() {
  const unauthorized = useAtomValue(unauthorizedAtom);
  const [helpOpen, setHelpOpen] = useState(false);
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
      <div data-loom-plan className="flex h-dvh flex-col">
        {unauthorized ? (
          <Unauthorized />
        ) : (
          <>
            <PlanHeader />
            <Banners />
            <div className="flex min-h-0 flex-1">
              <main className="min-w-0 flex-1 overflow-y-auto">
                <ErrorBoundary resetKey={pathname}>
                  <Outlet />
                </ErrorBoundary>
              </main>
              <Dock />
            </div>
            <PlanHelp open={helpOpen} onOpenChange={setHelpOpen} />
          </>
        )}
      </div>
    </TooltipProvider>
  );
}
