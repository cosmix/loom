import { useNow } from "@/components/hooks/use-now";
import { Button } from "@/components/ui/button";
import { formatElapsed } from "@/lib/format";
import { ActivityGlyph, ACTIVITY_LABELS } from "@/plan/ui/activity-glyphs";

import { Popover, PopoverContent, PopoverTrigger } from "./popover";
import { mostUrgent, useActivities, type Activity } from "./use-activities";

function elapsedLabel(since: number, now: number): string {
  // Keep the shared duration calculation, with minute precision for header rows.
  return formatElapsed(Math.max(0, Math.floor((now - since) / 1000)))
    .replace(/m\d+s$/, "m")
    .replace(/h(\d+)m$/, (_, minutes: string) => `h ${minutes.padStart(2, "0")}m`);
}

function ActivityList({ activities }: { activities: Activity[] }) {
  const now = useNow();

  return (
    <ul aria-label="In progress" className="flex flex-col gap-3">
      {activities.map((activity) => (
        <li key={activity.id} className="flex min-w-0 items-center gap-2">
          <ActivityGlyph kind={activity.kind} className="shrink-0" />
          <div className="min-w-0 flex-1">
            <p className="truncate text-xs font-medium" title={activity.title}>
              {activity.title}
            </p>
            {activity.subject !== null && (
              <p className="truncate text-xs text-muted-foreground" title={activity.subject}>
                {activity.subject}
              </p>
            )}
          </div>
          {activity.since !== null && (
            <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
              {elapsedLabel(activity.since, now)}
            </span>
          )}
        </li>
      ))}
    </ul>
  );
}

export function ActivityPill({ compact }: { compact: boolean }) {
  const activities = useActivities();
  const urgent = mostUrgent(activities);
  if (!urgent) return null;

  const label = `${activities.length} in progress`;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="rounded-full font-sans text-xs font-medium"
          aria-label={`${label}: ${ACTIVITY_LABELS[urgent.kind]}`}
        >
          <ActivityGlyph kind={urgent.kind} />
          <span className="tabular-nums">{compact ? activities.length : label}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent
        aria-label="In progress"
        collisionPadding={16}
        className="max-h-[80vh] overflow-y-auto font-sans"
      >
        <ActivityList activities={activities} />
      </PopoverContent>
    </Popover>
  );
}
