import { useAtomValue } from "jotai";

import type { RunStrip as Strip } from "@/plan/api/schema-run";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import "@/plan/header/activity.css";
import { ActivityTile } from "@/plan/ui/activity-glyphs";

import { DashboardButton } from "./dashboard-button";
import { runStripAtom } from "./use-run";

function stripText(plan: string, strip: Strip): string {
  const daemon = strip.daemon_running ? "running" : "stopped";
  return `${plan} · daemon ${daemon} · ${strip.merged}/${strip.total} merged · ${strip.attention} need attention`;
}

/**
 * The run's footer row in the activity popover, which has room for the full status line: the plan
 * (as its file name, with the path in the title), the dashboard link and the merged progress.
 */
export function RunStrip() {
  const strip = useAtomValue(runStripAtom);
  if (!strip?.plan) return null;
  const { plan } = strip;
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
