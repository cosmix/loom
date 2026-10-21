import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { CircleXIcon, InfoIcon, TriangleAlertIcon, type LucideIcon } from "lucide-react";

import type { Alert } from "@/api/schema";
import { HazardRail } from "@/aurora-ui/feedback/HazardPanel";
import { toneClass } from "@/components/state-badge";
import type { Tone } from "@/lib/format";
import { alertsAtom } from "@/state/atoms";

/// Icon and tone per severity; a
/// warning or critical row carries the kit's caution-tape rail.
const SEVERITY: Record<
  Alert["severity"],
  { icon: LucideIcon; tone: Tone; bold: boolean; hazard: "warning" | "error" | null }
> = {
  info: { icon: InfoIcon, tone: "executing", bold: false, hazard: null },
  warning: { icon: TriangleAlertIcon, tone: "warning", bold: false, hazard: "warning" },
  critical: { icon: CircleXIcon, tone: "blocked", bold: true, hazard: "error" },
};

/// Scheduler alerts, one thin row each; absent when there are none.
export function AlertsBand() {
  const alerts = useAtomValue(alertsAtom);
  if (alerts.length === 0) return null;
  return (
    <section aria-label="alerts" className="flex flex-col gap-1">
      {alerts.map((alert, index) => {
        const { icon: Icon, tone, bold, hazard } = SEVERITY[alert.severity];
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
              <Icon aria-hidden="true" className="size-3 shrink-0 self-center" />
              <span className="text-foreground">{alert.text}</span>
              <span className="sr-only">{alert.severity}</span>
            </span>
          </div>
        );
      })}
    </section>
  );
}
