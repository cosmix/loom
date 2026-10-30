import { useAtomValue } from "jotai";

import { Button } from "@/components/ui/button";
import type { RunStrip as Strip } from "@/plan/api/schema-run";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import "@/plan/header/activity.css";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { ActivityGlyph, ActivityTile } from "@/plan/ui/activity-glyphs";

import { DashboardButton } from "./dashboard-button";
import { runStripAtom } from "./use-run";

const daemonState = (strip: Strip) => (strip.daemon_running ? "running" : "stopped");

function stripText(plan: string, strip: Strip): string {
  return `${plan} · daemon ${daemonState(strip)} · ${strip.merged}/${strip.total} merged · ${strip.attention} need attention`;
}

/**
 * `bar` is the bordered strip; `row` is the footer row of the activity popover, which has room
 * for the full status line and ignores `compact`.
 */
export function RunStrip({
  compact = false,
  variant = "bar",
}: {
  compact?: boolean;
  variant?: "bar" | "row";
}) {
  const strip = useAtomValue(runStripAtom);
  if (!strip?.plan) return null;
  if (variant === "row") return <RowStrip plan={strip.plan} strip={strip} />;
  return (
    <div className="relative flex min-w-0 items-center gap-2 rounded-lg border border-border/70 bg-muted/30 px-2 py-1 text-sm">
      {compact ? (
        <CompactStrip plan={strip.plan} strip={strip} />
      ) : (
        <WideStrip plan={strip.plan} strip={strip} />
      )}
      <MergedProgress strip={strip} />
    </div>
  );
}

function RowStrip({ plan, strip }: { plan: string; strip: Strip }) {
  return (
    <div className="activity-row activity-run">
      <ActivityTile kind="running" paused={!strip.daemon_running} />
      <div className="activity-row__body">
        <span className="activity-row__title">loom run</span>
        <span
          className="activity-row__sub activity-row__sub--wrap"
          title={visibleControls(stripText(plan, strip))}
        >
          <StripStatusText plan={plan.split("/").at(-1) ?? plan} strip={strip} />
        </span>
      </div>
      <DashboardButton variant="link" />
      <MergedProgress strip={strip} />
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
            <ActivityGlyph
              kind="running"
              paused={!strip.daemon_running}
              className={strip.daemon_running ? "" : "opacity-50"}
            />
            <span
              className={`tabular-nums${strip.attention > 0 ? " font-medium text-(--tone-warning)" : ""}`}
            >
              {strip.attention}
            </span>
          </Button>
        </PopoverTrigger>
        <PopoverContent aria-label="Run status" className="[overflow-wrap:anywhere]">
          <p>
            <StripStatusText plan={plan} strip={strip} />
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
      <ActivityGlyph kind="running" paused={!strip.daemon_running} />
      <span className="min-w-0 max-w-72 truncate" title={visibleControls(stripText(plan, strip))}>
        <StripStatusText plan={plan.split("/").at(-1) ?? plan} strip={strip} />
      </span>
      <DashboardButton variant="link" />
    </>
  );
}

function StripStatusText({ plan, strip }: { plan: string; strip: Strip }) {
  const text = stripText(plan, strip);
  if (strip.attention === 0) return <Untrusted text={text} />;
  const attention = `${strip.attention} need attention`;
  return (
    <>
      <Untrusted text={text.slice(0, -attention.length)} />
      <span className="font-medium text-(--tone-warning) tabular-nums">
        <Untrusted text={attention} />
      </span>
    </>
  );
}

function MergedProgress({ strip }: { strip: Strip }) {
  if (strip.total === 0) return null;
  return (
    <div
      role="progressbar"
      aria-label="Merged stages"
      aria-valuenow={strip.merged}
      aria-valuemin={0}
      aria-valuemax={strip.total}
      className="absolute inset-x-0 bottom-0 h-0.5 overflow-hidden rounded-b-lg bg-muted"
    >
      <div
        className="h-full bg-(--tone-merged)"
        style={{ width: `${(strip.merged / strip.total) * 100}%` }}
      />
    </div>
  );
}
