import { useAtomValue } from "jotai";
import { ActivityIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { RunStrip as Strip } from "@/plan/api/schema-run";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";

import { DashboardButton } from "./dashboard-button";
import { runStripAtom } from "./use-run";

const daemonState = (strip: Strip) => (strip.daemon_running ? "running" : "stopped");

function stripText(plan: string, strip: Strip): string {
  return `${plan} · daemon ${daemonState(strip)} · ${strip.merged}/${strip.total} merged · ${strip.attention} need attention`;
}

export function RunStrip({ compact = false }: { compact?: boolean }) {
  const strip = useAtomValue(runStripAtom);
  if (!strip?.plan) return null;
  return (
    <div className="flex items-center gap-2 text-sm">
      {compact ? (
        <CompactStrip plan={strip.plan} strip={strip} />
      ) : (
        <WideStrip plan={strip.plan} strip={strip} />
      )}
    </div>
  );
}

/** Beside Changed since and Summary status in a narrow header: a button, details in a popover. */
function CompactStrip({ plan, strip }: { plan: string; strip: Strip }) {
  return (
    <>
      <Popover>
        <PopoverTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-label={`Run status: daemon ${daemonState(strip)}, ${strip.attention} need attention`}
          >
            <ActivityIcon aria-hidden="true" className={strip.daemon_running ? "" : "opacity-50"} />
            <span className="tabular-nums">{strip.attention}</span>
          </Button>
        </PopoverTrigger>
        <PopoverContent aria-label="Run status" className="[overflow-wrap:anywhere]">
          <p>
            <Untrusted text={stripText(plan, strip)} />
          </p>
        </PopoverContent>
      </Popover>
      <DashboardButton variant="icon" />
    </>
  );
}

/**
 * The header cannot spare the repo-relative path: the line shows the file name and truncates,
 * the title carries the full text.
 */
function WideStrip({ plan, strip }: { plan: string; strip: Strip }) {
  return (
    <>
      <span className="min-w-0 max-w-72 truncate" title={visibleControls(stripText(plan, strip))}>
        <Untrusted text={stripText(plan.split("/").at(-1) ?? plan, strip)} />
      </span>
      <DashboardButton variant="link" />
    </>
  );
}
