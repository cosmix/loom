import { atom } from "jotai/vanilla";

import type { Alert, Attention, Snapshot, StageStatus, StageSummary } from "@/api/schema";
import { orderStages, type OrderedStage } from "@/lib/levels";

export type ConnectionPhase = "connecting" | "live" | "reconnecting" | "error";

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
  since: 0,
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
