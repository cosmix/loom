import type { Snapshot } from "@/api/schema";
import { statusesById } from "@/lib/activity";
import { orderStages } from "@/lib/levels";

const INFORMATIONAL_LABEL = "COMPLETION PENDING";

export interface NotifyEvent {
  /** Stable de-dupe key; also used as the Notification `tag`. */
  key: string;
  title: string;
  body: string;
}

/** Key of the run-finished event; the delivery cap never drops this one. */
export const RUN_FINISHED_KEY = "run-finished";

function settled(snapshot: Snapshot): boolean {
  const { stages, merge } = snapshot.status;
  return (
    stages.length > 0 &&
    merge.conflicts.length === 0 &&
    stages.every(
      (stage) => stage.status === "skipped" || (stage.status === "completed" && stage.merged),
    )
  );
}

function dedupeEvents(events: NotifyEvent[]): NotifyEvent[] {
  const keys = new Set<string>();
  return events.filter((event) => {
    if (keys.has(event.key)) {
      return false;
    }
    keys.add(event.key);
    return true;
  });
}

export function notifiableEvents(previous: Snapshot | null, next: Snapshot): NotifyEvent[] {
  if (previous === null) {
    return [];
  }

  const events: NotifyEvent[] = [];
  const previousAttention = new Set(
    previous.attention.map((entry) => `${entry.id}:${entry.label}`),
  );

  for (const entry of next.attention) {
    const attentionKey = `${entry.id}:${entry.label}`;
    if (entry.label === INFORMATIONAL_LABEL || previousAttention.has(attentionKey)) {
      continue;
    }
    events.push({
      key: `attention:${attentionKey}`,
      title: `loom — ${entry.label.toLowerCase()}`,
      body: `${entry.name} (${entry.id})`,
    });
  }

  const previousStatuses = statusesById(previous);
  for (const { stage } of orderStages(next.status.stages)) {
    if (previousStatuses.get(stage.id) === stage.status || stage.status !== "needs-handoff") {
      continue;
    }
    events.push({
      key: `handoff:${stage.id}`,
      title: "loom — needs handoff",
      body: `${stage.name} (${stage.id}) needs a handoff`,
    });
  }

  if (!settled(previous) && settled(next)) {
    const mergedCount = next.status.stages.filter((stage) => stage.merged).length;
    events.push({
      key: RUN_FINISHED_KEY,
      title: "loom — run finished",
      body: `${next.status.plan_name ?? "the run"} finished; ${mergedCount} stages merged`,
    });
  }

  return dedupeEvents(events);
}
