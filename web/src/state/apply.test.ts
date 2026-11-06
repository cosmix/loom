import { createStore } from "jotai/vanilla";
import { afterEach, describe, expect, it, vi } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { applyFrame, applySnapshot } from "@/state/apply";
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
  it("replaces the stored snapshot whatever its stamp", () => {
    const store = createStore();
    applySnapshot(store, fixture);
    const earlier = { ...fixture, generated_at: "2020-01-01T00:00:00Z" };

    applySnapshot(store, earlier);

    expect(store.get(snapshotAtom)).toBe(earlier);
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

  it("does not deliver attention from the baseline snapshot", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);

    applySnapshot(store, fixture);

    expect(notifications.instances).toHaveLength(0);
  });
});

describe("applyFrame", () => {
  it("a full frame replaces the snapshot", () => {
    const store = createStore();

    applyFrame(store, { t: "full", snapshot: fixture });

    expect(store.get(snapshotAtom)).toBe(fixture);
  });

  it("a state frame delivers new attention", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);
    const next = nextSnapshotWithAttention("2100-01-01T00:00:00Z");

    applyFrame(store, { t: "full", snapshot: fixture });
    applyFrame(store, { t: "state", at: next.generated_at, attention: next.attention });

    expect(notifications.instances).toHaveLength(1);
    expect(store.get(snapshotAtom)?.attention).toHaveLength(next.attention.length);
    expect(store.get(snapshotAtom)?.generated_at).toBe(next.generated_at);
  });

  it("a tick delivers no notification", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);
    applyFrame(store, { t: "full", snapshot: fixture });

    applyFrame(store, {
      t: "tick",
      at: "2100-01-01T00:00:00Z",
      tick_age: 9,
      stages: { server: { elapsed: 1000 } },
    });

    expect(notifications.instances).toHaveLength(0);
    expect(store.get(snapshotAtom)?.tick_age_secs).toBe(9);
    expect(store.get(snapshotAtom)?.status.stages[1]?.elapsed_secs).toBe(1000);
  });

  it("a tokens frame replaces the token flow without a notification", () => {
    const notifications = installNotifications();
    const store = createStore();
    store.set(notificationsEnabledAtom, true);
    applyFrame(store, { t: "full", snapshot: fixture });

    applyFrame(store, { t: "tokens", tokens: undefined });

    expect(store.get(snapshotAtom)?.status.tokens).toBeUndefined();
    expect(notifications.instances).toHaveLength(0);
  });

  it("ignores a delta while no snapshot is stored", () => {
    const store = createStore();

    applyFrame(store, { t: "tick", at: "2100-01-01T00:00:00Z", tick_age: null, stages: {} });
    applyFrame(store, { t: "state", at: "2100-01-01T00:00:00Z", notice: "late" });
    applyFrame(store, { t: "tokens", tokens: undefined });

    expect(store.get(snapshotAtom)).toBeNull();
  });
});
