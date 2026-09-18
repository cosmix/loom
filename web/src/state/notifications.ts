import { atomWithStorage } from "jotai/utils";

import type { Snapshot } from "@/api/schema";
import { notifiableEvents, RUN_FINISHED_KEY } from "@/lib/notify";
import type { Store } from "@/state/store";

const MAX_NOTIFICATIONS_PER_FRAME = 5;

export const NOTIFICATIONS_STORAGE_KEY = "loom:notifications";

export const notificationsEnabledAtom = atomWithStorage<boolean>(
  NOTIFICATIONS_STORAGE_KEY,
  false,
  undefined,
  { getOnInit: true },
);

export type NotificationSupport = "unsupported" | "insecure" | "default" | "granted" | "denied";

export function notificationSupport(): NotificationSupport {
  if (!("Notification" in window)) {
    return "unsupported";
  }
  if (window.isSecureContext !== true) {
    return "insecure";
  }
  return window.Notification.permission;
}

export async function requestNotificationPermission(): Promise<NotificationSupport> {
  const support = notificationSupport();
  if (support !== "default") {
    return support;
  }
  return window.Notification.requestPermission();
}

export function deliverNotifications(
  store: Store,
  previous: Snapshot | null,
  next: Snapshot,
): void {
  if (store.get(notificationsEnabledAtom) !== true || notificationSupport() !== "granted") {
    return;
  }

  const events = notifiableEvents(previous, next);
  const finished = events.filter((event) => event.key === RUN_FINISHED_KEY);
  const rest = events.filter((event) => event.key !== RUN_FINISHED_KEY);

  for (const event of [...rest.slice(0, MAX_NOTIFICATIONS_PER_FRAME), ...finished]) {
    try {
      const notification = new window.Notification(event.title, {
        body: event.body,
        tag: event.key,
      });
      notification.onclick = () => {
        window.focus();
        notification.close();
      };
    } catch {
      // Mobile browsers may require a service worker for notifications.
    }
  }
}

/** Keeps the preference atom mounted for the app's lifetime, so a `storage` event from another tab
 *  updates it even while the settings dialog is closed. Returns the unsubscribe. */
export function watchNotificationPreference(store: Store): () => void {
  return store.sub(notificationsEnabledAtom, () => {});
}
