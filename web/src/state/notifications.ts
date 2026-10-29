import { useAtom } from "jotai";
import { atomWithStorage } from "jotai/utils";
import { useEffect, useState } from "react";

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

export function notificationStatus(support: NotificationSupport, on: boolean): string {
  switch (support) {
    case "unsupported":
      return "this browser has no notification support";
    case "insecure":
      return "notifications need localhost or HTTPS; this page is plain HTTP on a LAN address";
    case "denied":
      return "the browser has blocked notifications for this page";
    case "granted":
      return on ? "on" : "off";
    case "default":
      return "the browser will ask when you turn this on";
  }
}

export function useNotificationToggle(): {
  on: boolean;
  disabled: boolean;
  support: NotificationSupport;
  toggle: () => Promise<void>;
} {
  const [enabled, setEnabled] = useAtom(notificationsEnabledAtom);
  const [support, setSupport] = useState<NotificationSupport>(() => notificationSupport());
  const on = enabled === true && support === "granted";
  const disabled = support === "unsupported" || support === "insecure" || support === "denied";

  useEffect(() => {
    if (enabled && support !== "granted") setEnabled(false);
  }, [enabled, support, setEnabled]);

  async function toggle(): Promise<void> {
    if (on) {
      setEnabled(false);
      return;
    }

    if (support === "default") {
      const result = await requestNotificationPermission();
      setSupport(result);
      setEnabled(result === "granted");
      return;
    }

    if (support === "granted") setEnabled(true);
  }

  return { on, disabled, support, toggle };
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
