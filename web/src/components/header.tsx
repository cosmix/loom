import { useAtomValue } from "jotai/react";
import { Link } from "react-router";
import { CircleHelpIcon, SlidersHorizontalIcon } from "lucide-react";

import { ThemeToggle } from "@/aurora-ui/theme/ThemeToggle";
import { WorkRoundel } from "@/components/activity-roundel";
import {
  DaemonLine,
  MergeLine,
  ProgressLine,
  SummaryLine,
  TokenLine,
} from "@/components/header-lines";
import { Logo } from "@/components/logo";
import { useOpenSettings } from "@/components/settings-dialog";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { ViewSwitch } from "@/components/view-switch";
import { attentionAtom, snapshotAtom } from "@/state/atoms";

/// The TUI's header block: the logo, the plan name, the progress row and the
/// stage and merge rails, with the daemon/feed state and controls on the right.
export function Header({ onOpenLegend }: { onOpenLegend: () => void }) {
  const snapshot = useAtomValue(snapshotAtom);
  // The alarm cell counts only what waits on the operator.
  const attention = useAtomValue(attentionAtom).filter((entry) => !entry.automatic);
  const openSettings = useOpenSettings();

  return (
    <header className="border-b border-hairline bg-linear-to-b from-card to-background">
      {/* Below lg: logo and controls share the top row; the daemon state, plan
          name and details each run full width beneath. From lg: the logo spans
          the left column; name, daemon state and controls share the first row. */}
      <div className="mx-auto grid max-w-[1920px] grid-cols-[auto_1fr] items-center gap-x-5 gap-y-3 px-4 py-4 sm:px-6 lg:grid-cols-[auto_1fr_auto_auto] lg:gap-x-3 lg:gap-y-0">
        <Link
          to="/"
          className="col-start-1 row-start-1 text-(--logo) lg:row-span-2 lg:self-start lg:pt-1"
          aria-label="overview"
        >
          <Logo className="h-8 w-auto sm:h-10 lg:h-14" />
        </Link>
        <div className="header-controls col-start-2 row-start-1 flex flex-wrap items-center justify-end gap-2 lg:col-start-4">
          {snapshot && <WorkRoundel />}
          <ViewSwitch />
          <ThemeToggle />
          <Button
            variant="ghost"
            size="sm"
            onClick={() => openSettings()}
            aria-label="open settings"
          >
            <SlidersHorizontalIcon />
            <span className="hidden sm:inline">settings</span>
          </Button>
          <Button variant="ghost" size="sm" onClick={onOpenLegend} aria-label="open legend">
            <CircleHelpIcon />
            <span className="hidden sm:inline">legend</span>
          </Button>
        </div>
        <div className="col-span-2 row-start-2 flex min-w-0 justify-center lg:col-span-1 lg:col-start-3 lg:row-start-1">
          <DaemonLine snapshot={snapshot} />
        </div>
        <div className="col-span-2 row-start-3 min-w-0 lg:col-span-1 lg:col-start-2 lg:row-start-1">
          <PlanName name={snapshot?.status.plan_name} loading={snapshot === null} />
        </div>
        <div className="col-span-2 row-start-4 min-w-0 lg:col-span-3 lg:col-start-2 lg:row-start-2">
          {snapshot ? (
            <div className="flex flex-col gap-3.5 lg:mt-3">
              <ProgressLine snapshot={snapshot} />
              <div className="flex flex-wrap items-end gap-x-4 gap-y-3">
                <SummaryLine snapshot={snapshot} attention={attention.length} />
                <MergeLine snapshot={snapshot} />
                <TokenLine />
              </div>
            </div>
          ) : (
            <HeaderSkeleton />
          )}
        </div>
      </div>
    </header>
  );
}

function PlanName({ name, loading }: { name: string | null | undefined; loading: boolean }) {
  if (loading) return <Skeleton className="h-10 w-56" />;
  return (
    <div className="min-w-0">
      <p className="eyebrow text-[10px]">plan</p>
      {name ? (
        <h1 className="font-display text-xl leading-tight font-semibold tracking-tight">{name}</h1>
      ) : (
        <span className="text-xl leading-tight text-muted-foreground">(no plan name)</span>
      )}
    </div>
  );
}

function HeaderSkeleton() {
  return (
    <div className="flex flex-col gap-3.5 lg:mt-3" aria-busy="true">
      <Skeleton className="h-6 w-96 max-w-full" />
      <Skeleton className="h-14 w-[36rem] max-w-full" />
    </div>
  );
}
