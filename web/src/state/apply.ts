import type { Snapshot } from "@/api/schema";
import { snapshotAtom } from "@/state/atoms";
import { deliverNotifications } from "@/state/notifications";
import type { Store } from "@/state/store";

/** Store a frame.
 * Desktop notifications are raised here because both snapshots are in scope.
 */
export function applySnapshot(store: Store, next: Snapshot): void {
  const previous = store.get(snapshotAtom);
  // The server suppresses unchanged frames, so an out-of-order frame (the
  // initial /api/status fetch landing after the socket's first message)
  // would otherwise regress the UI to an older snapshot with nothing to
  // correct it until the tree next changes. An equal stamp is the same frame
  // again (generated_at is set once at build time): the server's keepalive
  // resend, or the fetch and the socket's first message, so skip it too.
  if (previous && Date.parse(next.generated_at) <= Date.parse(previous.generated_at)) {
    return;
  }
  store.set(snapshotAtom, next);
  deliverNotifications(store, previous, next);
}
