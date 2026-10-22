import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { applySnapshot } from "@/state/apply";
import {
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

    applySnapshot(store, fixture);

    expect(store.get(snapshotAtom)).toBe(fixture);
    expect(store.get(orderedStagesAtom)).toHaveLength(9);
    expect(store.get(attentionAtom).map((entry) => entry.id)).toEqual([
      "client",
      "docs",
      "integration-verify",
      "merge-gate",
      "local-runtime",
    ]);
    expect(store.get(alertsAtom)).toHaveLength(3);
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
