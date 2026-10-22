import type { Snapshot, StageStatus } from "@/api/schema";

function transitionMessage(
  id: string,
  status: StageStatus,
  previous: StageStatus | undefined,
): string | null {
  switch (status) {
    case "executing":
      return `${id} started`;
    case "completed":
      return `${id} completed`;
    case "blocked":
      return `${id} blocked`;
    case "queued":
      // An Accept or NeedsMoreEvidence verdict requeues the stage for a fresh
      // session; a bare "ready" would read as an unexplained restart.
      return previous === "needs-adjudication" ? `${id} verdict applied, requeued` : `${id} ready`;
    case "needs-handoff":
      return `${id} needs handoff`;
    case "needs-adjudication":
      return `${id} entered adjudication`;
    case "needs-human-review":
      return `${id} needs human review`;
    default:
      return null;
  }
}

export function statusesById(snapshot: Snapshot | null): Map<string, StageStatus> {
  const statuses = new Map<string, StageStatus>();
  for (const stage of snapshot?.status.stages ?? []) {
    if (!statuses.has(stage.id)) {
      statuses.set(stage.id, stage.status);
    }
  }
  return statuses;
}

export interface ActivityEntry {
  at: number;
  stageId: string;
  status: StageStatus;
  message: string;
}

/**
 * The server's recorded stage transitions as panel entries, newest first.
 * The log is durable, so a freshly loaded page shows what happened before it
 * connected; statuses without a message stay out of the panel.
 */
export function activityEntries(snapshot: Snapshot | null): ActivityEntry[] {
  const entries: ActivityEntry[] = [];
  for (const event of snapshot?.status.activity ?? []) {
    const message = transitionMessage(event.stage_id, event.to, event.from);
    if (message) {
      entries.push({
        at: Date.parse(event.at),
        stageId: event.stage_id,
        status: event.to,
        message,
      });
    }
  }
  return entries.reverse();
}
