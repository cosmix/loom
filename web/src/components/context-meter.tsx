import { cn } from "cn";

import type { StageSummary } from "@/api/schema";
import { contextUsage, type Tone } from "@/lib/format";
import { toneClass } from "@/components/state-badge";

const HEALTH_TONE: Record<"green" | "yellow" | "red", Tone> = {
  green: "completed",
  yellow: "warning",
  red: "blocked",
};

const SEGMENTS = [0, 1, 2, 3, 4] as const;

/// The TUI's five-segment context meter (`━━━╌╌ 39%`) as bars plus a percent.
/// Renders nothing when the stage has no applicable context reading.
export function ContextMeter({
  stage,
  className,
  detail = false,
}: {
  stage: StageSummary;
  className?: string;
  detail?: boolean;
}) {
  const usage = contextUsage(stage);
  if (usage === null) return null;
  const label = `context ${usage.percent}% of ceiling`;
  return (
    <span
      role="img"
      aria-label={label}
      title={`${usage.tokens.toLocaleString()} of ${usage.ceiling.toLocaleString()} tokens`}
      className={cn(
        "inline-flex items-center gap-2 whitespace-nowrap tabular-nums",
        toneClass(HEALTH_TONE[usage.health]),
        className,
      )}
    >
      <span className={cn("inline-flex gap-0.5", detail ? "h-2.5" : "h-1.5")}>
        {SEGMENTS.map((index) => (
          <span
            key={index}
            className={cn(
              "block rounded-[1px]",
              detail ? "w-4" : "w-2.5",
              index < usage.filled ? "bg-(--tone)" : "bg-(--tone)/25",
            )}
          />
        ))}
      </span>
      <span className={cn(detail ? "text-sm" : "text-xs")}>{usage.percent}%</span>
    </span>
  );
}
