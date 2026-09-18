import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary } from "@/api/schema";
import { RUN_FINISHED_KEY } from "@/lib/notify";
import {
  deliverNotifications,
  notificationSupport,
  NOTIFICATIONS_STORAGE_KEY,
  notificationsEnabledAtom,
  watchNotificationPreference,
} from "@/state/notifications";

const fixture = snapshotSchema.parse(fixtureJson);

class TestNotification {
  static instances: TestNotification[] = [];
  static permission: NotificationPermission = "granted";
  static requestPermission = vi.fn(async (): Promise<NotificationPermission> => {
    return TestNotification.permission;
  });

  readonly close = vi.fn();
  onclick: (() => void) | null = null;
  readonly title: string;
  readonly options: NotificationOptions;

  constructor(title: string, options: NotificationOptions) {
    this.title = title;
    this.options = options;
    TestNotification.instances.push(this);
  }
}

function installNotifications(
  permission: NotificationPermission = "granted",
): typeof TestNotification {
  TestNotification.instances = [];
  TestNotification.permission = permission;
  TestNotification.requestPermission = vi.fn(async (): Promise<NotificationPermission> => {
    return TestNotification.permission;
  });
  vi.stubGlobal("Notification", TestNotification);
  vi.stubGlobal("focus", vi.fn());
  Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
  return TestNotification;
}

function attentionFor(stage: StageSummary, label: string) {
  return {
    ...fixture.attention[0]!,
    id: stage.id,
    name: stage.name,
    label,
    hint: "loom stage resume example",
  };
}

function snapshotWithNewAttention(count: number): Snapshot {
  const snapshot = structuredClone(fixture);
  snapshot.attention = snapshot.status.stages
    .slice(0, count)
    .map((stage) => attentionFor(stage, "NEEDS INPUT"));
  return snapshot;
}

function settledSnapshot(): Snapshot {
  const snapshot = structuredClone(fixture);
  snapshot.status.stages = snapshot.status.stages.map((stage) => ({
    ...stage,
    status: "completed",
    merged: true,
  }));
  snapshot.status.merge.pending = [];
  snapshot.status.merge.conflicts = [];
  return snapshot;
}

function dispatchPreference(value: "true" | "false"): void {
  window.localStorage.setItem(NOTIFICATIONS_STORAGE_KEY, value);
  window.dispatchEvent(
    new StorageEvent("storage", {
      key: NOTIFICATIONS_STORAGE_KEY,
      newValue: value,
      storageArea: window.localStorage,
    }),
  );
}

afterEach(() => {
  vi.unstubAllGlobals();
  Reflect.deleteProperty(window, "isSecureContext");
  window.localStorage.removeItem(NOTIFICATIONS_STORAGE_KEY);
});

describe("deliverNotifications", () => {
  it("does nothing while notifications are disabled", () => {
    const notifications = installNotifications();
    const store = createStore();

    deliverNotifications(store, fixture, snapshotWithNewAttention(1));

    expect(notifications.instances).toHaveLength(0);
  });

  it("delivers each event with its title, body, tag, and click behavior", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);
    const next = snapshotWithNewAttention(1);
    const stage = next.status.stages[0]!;

    deliverNotifications(store, fixture, next);

    expect(notifications.instances).toHaveLength(1);
    const notification = notifications.instances[0]!;
    expect(notification.title).toBe("loom — needs input");
    expect(notification.options).toEqual({
      body: `${stage.name} (${stage.id})`,
      tag: `attention:${stage.id}:NEEDS INPUT`,
    });
    expect(notification.onclick).toEqual(expect.any(Function));
    notification.onclick?.();
    expect(notification.close).toHaveBeenCalledTimes(1);
  });

  it("does nothing when permission is denied", () => {
    const notifications = installNotifications("denied");
    const store = createStore();
    store.set(notificationsEnabledAtom, true);

    deliverNotifications(store, fixture, snapshotWithNewAttention(1));

    expect(notifications.instances).toHaveLength(0);
  });

  it("does nothing when Notification is unsupported", () => {
    Reflect.deleteProperty(window, "Notification");
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
    const store = createStore();
    store.set(notificationsEnabledAtom, true);

    expect(() => deliverNotifications(store, fixture, snapshotWithNewAttention(1))).not.toThrow();
    expect(notificationSupport()).toBe("unsupported");
  });

  it("reports insecure support and does not construct notifications", () => {
    const notifications = installNotifications();
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: false });
    const store = createStore();
    store.set(notificationsEnabledAtom, true);

    deliverNotifications(store, fixture, snapshotWithNewAttention(1));

    expect(notificationSupport()).toBe("insecure");
    expect(notifications.instances).toHaveLength(0);
  });

  it("caps seven new attention entries at five notifications", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);

    deliverNotifications(store, fixture, snapshotWithNewAttention(7));

    expect(notifications.instances).toHaveLength(5);
  });

  it("delivers a run-finished notification after capped attention entries", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);
    const next = settledSnapshot();
    next.attention = next.status.stages.map((stage) => attentionFor(stage, "NEEDS INPUT"));

    deliverNotifications(store, fixture, next);

    expect(notifications.instances).toHaveLength(6);
    expect(notifications.instances.at(-1)?.options.tag).toBe(RUN_FINISHED_KEY);
  });

  it("swallows notification constructor errors", () => {
    class ThrowingNotification {
      static permission: NotificationPermission = "granted";
      static requestPermission = vi.fn(async (): Promise<NotificationPermission> => "granted");

      constructor(_title: string, _options: NotificationOptions) {
        throw new TypeError("service worker required");
      }
    }

    vi.stubGlobal("Notification", ThrowingNotification);
    Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
    const store = createStore();
    store.set(notificationsEnabledAtom, true);

    expect(() => deliverNotifications(store, fixture, snapshotWithNewAttention(1))).not.toThrow();
  });

  it("another tab synchronizes the notification preference", () => {
    const notifications = installNotifications();
    const store = createStore();
    const dispose = watchNotificationPreference(store);
    const first = snapshotWithNewAttention(1);
    const second = snapshotWithNewAttention(2);

    dispatchPreference("true");
    deliverNotifications(store, fixture, first);
    expect(notifications.instances).toHaveLength(1);

    dispatchPreference("false");
    deliverNotifications(store, first, second);
    expect(notifications.instances).toHaveLength(1);
    dispose();
  });

  it("stops receiving storage events after the preference watcher is disposed", () => {
    installNotifications();
    const store = createStore();
    const dispose = watchNotificationPreference(store);

    dispose();
    dispatchPreference("true");

    expect(store.get(notificationsEnabledAtom)).toBe(false);
  });
});
