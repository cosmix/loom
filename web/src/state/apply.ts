import type { Snapshot } from "@/api/schema";
import { appendTransitions } from "@/lib/activity";
import { activityLogAtom, snapshotAtom } from "@/state/atoms";
import type { Store } from "@/state/store";

/** Store a frame and append the activity transitions it implies. */
export function applySnapshot(store: Store, next: Snapshot, now: number = Date.now()): void {
  const previous = store.get(snapshotAtom);
  // The server suppresses unchanged frames, so an out-of-order frame (the
  // initial /api/status fetch landing after the socket's first message)
  // would otherwise regress the UI to an older snapshot with nothing to
  // correct it until the tree next changes.
  if (previous && Date.parse(next.generated_at) < Date.parse(previous.generated_at)) {
    return;
  }
  store.set(snapshotAtom, next);
  store.set(activityLogAtom, appendTransitions(store.get(activityLogAtom), previous, next, now));
}
