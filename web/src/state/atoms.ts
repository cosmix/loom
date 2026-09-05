import { atom } from "jotai/vanilla";

import type { Alert, Attention, Snapshot, StageStatus, StageSummary } from "@/api/schema";
import { orderStages, type OrderedStage } from "@/lib/levels";

export type ConnectionPhase = "connecting" | "live" | "reconnecting" | "offline" | "error";

/**
 * `since` is when the CURRENT phase began, in `Date.now()` milliseconds.
 * An outage - the "reconnecting" and "offline" phases - is one event with
 * one `since`, stamped the moment the feed first dropped: every retry
 * within the outage, and the flip from "reconnecting" to "offline", keeps
 * that original `since`. Every other phase change stamps a fresh `since`.
 */
export interface ConnectionState {
  phase: ConnectionPhase;
  since: number;
  message?: string;
}

export interface ActivityEntry {
  at: number;
  stageId: string;
  status: StageStatus;
  message: string;
}

export const snapshotAtom = atom<Snapshot | null>(null);
export const connectionAtom = atom<ConnectionState>({
  phase: "connecting",
  since: Date.now(),
});
export const activityLogAtom = atom<ActivityEntry[]>([]);
export const orderedStagesAtom = atom<OrderedStage[]>((get) => {
  const snapshot = get(snapshotAtom);
  return snapshot ? orderStages(snapshot.status.stages) : [];
});
export const attentionAtom = atom<Attention[]>((get) => get(snapshotAtom)?.attention ?? []);
export const alertsAtom = atom<Alert[]>((get) => get(snapshotAtom)?.alerts ?? []);

export function selectStage(snapshot: Snapshot | null, id: string): StageSummary | undefined {
  return snapshot?.status.stages.find((stage) => stage.id === id);
}
