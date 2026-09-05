import { cn } from "cn";
import { useAtomValue } from "jotai/react";

import type { Alert } from "@/api/schema";
import { HazardRail } from "@/aurora-ui/feedback/HazardPanel";
import { toneClass } from "@/components/state-badge";
import type { Tone } from "@/lib/format";
import { alertsAtom } from "@/state/atoms";

/// Marker and tone per severity, as `render_scheduler_alerts` draws them; a
/// warning or critical row carries the kit's caution-tape rail.
const SEVERITY: Record<
  Alert["severity"],
  { marker: string; tone: Tone; bold: boolean; hazard: "warning" | "error" | null }
> = {
  info: { marker: "·", tone: "executing", bold: false, hazard: null },
  warning: { marker: "!", tone: "warning", bold: false, hazard: "warning" },
  critical: { marker: "✖", tone: "blocked", bold: true, hazard: "error" },
};

/// Scheduler alerts, one thin row each; absent when there are none.
export function AlertsBand() {
  const alerts = useAtomValue(alertsAtom);
  if (alerts.length === 0) return null;
  return (
    <section aria-label="alerts" className="flex flex-col gap-1">
      {alerts.map((alert, index) => {
        const { marker, tone, bold, hazard } = SEVERITY[alert.severity];
        return (
          <div
            key={`${alert.severity}:${alert.text}:${index}`}
            role={alert.severity === "critical" ? "alert" : "status"}
            className={cn(
              "flex items-stretch overflow-hidden rounded-md border border-hairline bg-(--tone)/6 text-sm",
              toneClass(tone),
              bold && "font-semibold",
            )}
          >
            {hazard ? (
              <HazardRail tone={hazard} />
            ) : (
              <span aria-hidden="true" className="w-2.5 shrink-0 self-stretch bg-(--tone)/40" />
            )}
            <span className="flex items-baseline gap-2.5 px-3 py-1.5">
              <span aria-hidden="true" className="w-3 shrink-0 text-center font-mono">
                {marker}
              </span>
              <span className="text-foreground">{alert.text}</span>
              <span className="sr-only">{alert.severity}</span>
            </span>
          </div>
        );
      })}
    </section>
  );
}
