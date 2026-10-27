import { atom, type Atom } from "jotai/vanilla";
import { selectAtom } from "jotai/vanilla/utils";

import type { Alert, Attention, Snapshot, StageSummary } from "@/api/schema";
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

export const snapshotAtom = atom<Snapshot | null>(null);
export const connectionAtom = atom<ConnectionState>({
  phase: "connecting",
  since: Date.now(),
});
export const orderedStagesAtom = atom<OrderedStage[]>((get) => {
  const snapshot = get(snapshotAtom);
  return snapshot ? orderStages(snapshot.status.stages) : [];
});
export const attentionAtom = atom<Attention[]>((get) => get(snapshotAtom)?.attention ?? []);
export const alertsAtom = atom<Alert[]>((get) => get(snapshotAtom)?.alerts ?? []);

export function selectStage(snapshot: Snapshot | null, id: string): StageSummary | undefined {
  return snapshot?.status.stages.find((stage) => stage.id === id);
}

/// One value `read` off stage `id`. Its reader re-renders only when that value
/// changes (`Object.is`), not on every snapshot, so `read` should return a
/// primitive. Each call makes a new atom: memoize it per id.
export function stageValueAtom<T>(
  id: string,
  read: (stage: StageSummary | undefined) => T,
): Atom<T> {
  return selectAtom(snapshotAtom, (snapshot) => read(selectStage(snapshot, id)));
}
