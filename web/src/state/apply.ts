import type { Frame } from "@/api/frames";
import type { Snapshot } from "@/api/schema";
import { snapshotAtom } from "@/state/atoms";
import { mergeState, mergeTick, mergeTokens } from "@/state/merge";
import { deliverNotifications } from "@/state/notifications";
import type { Store } from "@/state/store";

/** Store a whole snapshot.
 * Desktop notifications are raised here because both snapshots are in scope.
 */
export function applySnapshot(store: Store, next: Snapshot): void {
  const previous = store.get(snapshotAtom);
  store.set(snapshotAtom, next);
  deliverNotifications(store, previous, next);
}

/** Apply one `/ws` message. Only `full` and `state` can raise notifications:
 * a `tick` moves clocks and a `tokens` message the token flow, neither of which
 * the notification rules read. A delta before any snapshot has nothing to
 * change and is ignored.
 */
export function applyFrame(store: Store, frame: Frame): void {
  if (frame.t === "full") {
    applySnapshot(store, frame.snapshot);
    return;
  }
  const previous = store.get(snapshotAtom);
  if (previous === null) return;
  if (frame.t === "state") {
    const next = mergeState(previous, frame);
    store.set(snapshotAtom, next);
    deliverNotifications(store, previous, next);
  } else if (frame.t === "tick") {
    store.set(snapshotAtom, mergeTick(previous, frame));
  } else {
    store.set(snapshotAtom, mergeTokens(previous, frame.tokens));
  }
}
