import type { Snapshot, StageStatus } from "@/api/schema";
import { orderStages } from "@/lib/levels";
import type { ActivityEntry } from "@/state/atoms";

export const MAX_ACTIVITY_ENTRIES = 20;

function transitionMessage(id: string, status: StageStatus): string | null {
  switch (status) {
    case "executing":
      return `${id} started`;
    case "completed":
      return `${id} completed`;
    case "blocked":
      return `${id} blocked`;
    case "queued":
      return `${id} ready`;
    case "needs-handoff":
      return `${id} needs handoff`;
    default:
      return null;
  }
}

function statusesById(snapshot: Snapshot | null): Map<string, StageStatus> {
  const statuses = new Map<string, StageStatus>();
  for (const stage of snapshot?.status.stages ?? []) {
    if (!statuses.has(stage.id)) {
      statuses.set(stage.id, stage.status);
    }
  }
  return statuses;
}

/** Append meaningful stage transitions while retaining the newest twenty entries. */
export function appendTransitions(
  log: readonly ActivityEntry[],
  previous: Snapshot | null,
  next: Snapshot,
  now: number,
): ActivityEntry[] {
  const entries = [...log];
  const previousStatuses = statusesById(previous);

  // Order like the TUI (level, then id) so simultaneous transitions log in a
  // stable sequence instead of raw, filesystem-dependent wire order.
  // orderStages already dedupes by id (keeping the first), which is why the
  // dedupe set that used to live here was removed.
  for (const { stage } of orderStages(next.status.stages)) {
    if (previousStatuses.get(stage.id) === stage.status) {
      continue;
    }
    const message = transitionMessage(stage.id, stage.status);
    if (message) {
      entries.push({ at: now, stageId: stage.id, status: stage.status, message });
    }
  }
  return entries.slice(-MAX_ACTIVITY_ENTRIES);
}
