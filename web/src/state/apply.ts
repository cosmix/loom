import type { Snapshot } from "@/api/schema";
import { appendTransitions } from "@/lib/activity";
import { activityLogAtom, snapshotAtom } from "@/state/atoms";
import type { Store } from "@/state/store";

/** Store a frame and append the activity transitions it implies. */
export function applySnapshot(store: Store, next: Snapshot, now: number = Date.now()): void {
  const previous = store.get(snapshotAtom);
  store.set(snapshotAtom, next);
  store.set(activityLogAtom, appendTransitions(store.get(activityLogAtom), previous, next, now));
}
