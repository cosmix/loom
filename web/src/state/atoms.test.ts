import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { applySnapshot } from "@/state/apply";
import {
  activityLogAtom,
  alertsAtom,
  attentionAtom,
  connectionAtom,
  orderedStagesAtom,
  selectStage,
  snapshotAtom,
} from "@/state/atoms";

const fixture = snapshotSchema.parse(fixtureJson);

describe("dashboard atoms", () => {
  it("stores a snapshot and derives its dashboard data", () => {
    const store = createStore();

    applySnapshot(store, fixture, 100);

    expect(store.get(snapshotAtom)).toBe(fixture);
    expect(store.get(orderedStagesAtom)).toHaveLength(7);
    expect(store.get(attentionAtom).map((entry) => entry.id)).toEqual([
      "client",
      "docs",
      "integration-verify",
    ]);
    expect(store.get(alertsAtom)).toHaveLength(3);
    // The first frame is the baseline; the log records later transitions.
    expect(store.get(activityLogAtom)).toHaveLength(0);
    const next = structuredClone(fixture);
    next.generated_at = "2026-09-04T12:00:01Z";
    next.status.stages.find((stage) => stage.id === "server")!.status = "completed";
    applySnapshot(store, next, 500);
    expect(store.get(activityLogAtom)).toEqual([
      { at: 500, stageId: "server", status: "completed", message: "server completed" },
    ]);
  });

  it("selects a stage from a nullable snapshot", () => {
    expect(selectStage(null, "server")).toBeUndefined();
    expect(selectStage(fixture, "server")?.name).toBe("Rust server");
  });

  it("starts with the connecting connection state", () => {
    // `since` is stamped once, at module load (the atom's initial value is
    // created a single time and shared by every store), so it predates this
    // test rather than `Date.now()` called here - just check it's a sane
    // timestamp, not a hardcoded 0 that would render as "connecting since
    // the epoch" if read before connectStatusSocket ever starts.
    const connection = createStore().get(connectionAtom);
    expect(connection.phase).toBe("connecting");
    expect(connection.since).toBeGreaterThan(0);
    expect(connection.since).toBeLessThanOrEqual(Date.now());
  });
});
