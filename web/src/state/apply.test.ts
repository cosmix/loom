import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { applySnapshot } from "@/state/apply";
import { snapshotAtom } from "@/state/atoms";
import { notificationsEnabledAtom } from "@/state/notifications";

const fixture = snapshotSchema.parse(fixtureJson);

class TestNotification {
  static instances: TestNotification[] = [];
  static permission: NotificationPermission = "granted";
  static requestPermission = vi.fn(async (): Promise<NotificationPermission> => {
    return TestNotification.permission;
  });

  readonly close = vi.fn();
  readonly title: string;
  readonly options: NotificationOptions;
  onclick: (() => void) | null = null;

  constructor(title: string, options: NotificationOptions) {
    this.title = title;
    this.options = options;
    TestNotification.instances.push(this);
  }
}

function installNotifications(): typeof TestNotification {
  TestNotification.instances = [];
  TestNotification.permission = "granted";
  TestNotification.requestPermission = vi.fn(async (): Promise<NotificationPermission> => {
    return TestNotification.permission;
  });
  vi.stubGlobal("Notification", TestNotification);
  vi.stubGlobal("focus", vi.fn());
  Object.defineProperty(window, "isSecureContext", {
    configurable: true,
    value: true,
  });
  return TestNotification;
}

function nextSnapshotWithAttention(generatedAt: string) {
  const next = structuredClone(fixture);
  const stage = next.status.stages[0]!;
  next.generated_at = generatedAt;
  next.attention.push({
    ...next.attention[0]!,
    id: stage.id,
    name: stage.name,
    label: "NEEDS INPUT",
    command: null,
    note: "the agent is waiting on a question: answer it in the stage's terminal",
  });
  return next;
}

afterEach(() => {
  vi.unstubAllGlobals();
  Reflect.deleteProperty(window, "isSecureContext");
});

describe("applySnapshot", () => {
  it("ignores a frame older than the stored snapshot", () => {
    const store = createStore();
    applySnapshot(store, fixture);

    const stale = {
      ...fixture,
      generated_at: "2020-01-01T00:00:00Z",
      status: {
        ...fixture.status,
        stages: fixture.status.stages.map((stage) =>
          stage.id === "knowledge-distill" ? { ...stage, status: "queued" as const } : stage,
        ),
      },
    };

    applySnapshot(store, stale);

    expect(store.get(snapshotAtom)).toBe(fixture);
  });

  it("ignores a repeat of the stored frame", () => {
    const store = createStore();
    applySnapshot(store, fixture);
    const stored = store.get(snapshotAtom);

    applySnapshot(store, structuredClone(fixture));

    expect(store.get(snapshotAtom)).toBe(stored);
  });

  it("delivers new attention from a later snapshot", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);
    const next = nextSnapshotWithAttention("2100-01-01T00:00:00Z");

    applySnapshot(store, fixture);
    applySnapshot(store, next);

    expect(notifications.instances).toHaveLength(1);
  });

  it("does not deliver attention from an earlier snapshot", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);
    const stale = nextSnapshotWithAttention("2020-01-01T00:00:00Z");

    applySnapshot(store, fixture);
    applySnapshot(store, stale);

    expect(notifications.instances).toHaveLength(0);
  });

  it("does not deliver attention from the baseline snapshot", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);

    applySnapshot(store, fixture);

    expect(notifications.instances).toHaveLength(0);
  });
});
