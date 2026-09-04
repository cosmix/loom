import { cn } from "cn";
import { useAtomValue } from "jotai/react";

import type { Alert } from "@/api/schema";
import { toneClass } from "@/components/state-badge";
import type { Tone } from "@/lib/format";
import { alertsAtom } from "@/state/atoms";

/// Marker and tone per severity, as `render_scheduler_alerts` draws them.
const SEVERITY: Record<Alert["severity"], { marker: string; tone: Tone; bold: boolean }> = {
  info: { marker: "·", tone: "executing", bold: false },
  warning: { marker: "!", tone: "warning", bold: false },
  critical: { marker: "✖", tone: "blocked", bold: true },
};

/// Scheduler alerts, one thin row each; absent when there are none.
export function AlertsBand() {
  const alerts = useAtomValue(alertsAtom);
  if (alerts.length === 0) return null;
  return (
    <section aria-label="alerts" className="flex flex-col gap-px">
      {alerts.map((alert, index) => {
        const { marker, tone, bold } = SEVERITY[alert.severity];
        return (
          <div
            key={`${alert.severity}:${alert.text}:${index}`}
            role={alert.severity === "critical" ? "alert" : "status"}
            className={cn(
              "flex items-baseline gap-2.5 rounded-md border-l-2 border-(--tone) bg-(--tone)/8 px-3 py-1.5 text-sm",
              toneClass(tone),
              bold && "font-semibold",
            )}
          >
            <span aria-hidden="true" className="w-3 shrink-0 text-center font-mono">
              {marker}
            </span>
            <span className="text-foreground">{alert.text}</span>
            <span className="sr-only">{alert.severity}</span>
          </div>
        );
      })}
    </section>
  );
}
