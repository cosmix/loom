import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema } from "@/api/schema";
import { applySnapshot } from "@/state/apply";
import { activityLogAtom, snapshotAtom } from "@/state/atoms";

const fixture = snapshotSchema.parse(fixtureJson);

describe("applySnapshot", () => {
  it("ignores a frame older than the stored snapshot and logs no transitions for it", () => {
    const store = createStore();
    applySnapshot(store, fixture, 100);
    const loggedAfterFirst = store.get(activityLogAtom);

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

    applySnapshot(store, stale, 200);

    expect(store.get(snapshotAtom)).toBe(fixture);
    expect(store.get(activityLogAtom)).toBe(loggedAfterFirst);
  });
});
