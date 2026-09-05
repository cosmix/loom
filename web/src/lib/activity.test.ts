import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary } from "@/api/schema";
import { appendTransitions, MAX_ACTIVITY_ENTRIES } from "@/lib/activity";

const fixture = snapshotSchema.parse(fixtureJson);

function snapshotWithStatus(status: StageSummary["status"]): Snapshot {
  const stage = { ...fixture.status.stages[0], id: "changing", status };
  return { ...fixture, status: { ...fixture.status, stages: [stage] } };
}

describe("activity transitions", () => {
  it("treats the first frame as a baseline and logs nothing for it", () => {
    expect(appendTransitions([], null, fixture, 100)).toEqual([]);
  });

  it("logs meaningful transitions observed after the baseline, at the observed time", () => {
    const next = structuredClone(fixture);
    next.status.stages.find((stage) => stage.id === "server")!.status = "completed";
    next.status.stages.find((stage) => stage.id === "design")!.status = "executing";

    expect(appendTransitions([], fixture, next, 250)).toEqual([
      { at: 250, stageId: "server", status: "completed", message: "server completed" },
      { at: 250, stageId: "design", status: "executing", message: "design started" },
    ]);
  });

  it("adds a transition when a stage becomes queued", () => {
    const next = structuredClone(fixture);
    next.status.stages.find((stage) => stage.id === "design")!.status = "queued";
    const log = appendTransitions([], null, fixture, 100);

    expect(appendTransitions(log, fixture, next, 200).at(-1)).toMatchObject({
      stageId: "design",
      status: "queued",
      message: "design ready",
    });
  });

  it("does not append entries for unchanged statuses", () => {
    const log = appendTransitions([], null, fixture, 100);

    expect(appendTransitions(log, fixture, fixture, 200)).toEqual(log);
  });

  it("remembers unlogged statuses between snapshots", () => {
    const executing = snapshotWithStatus("executing");
    const waiting = snapshotWithStatus("waiting-for-input");
    const queued = snapshotWithStatus("queued");
    const first = appendTransitions([], queued, executing, 100);
    const middle = appendTransitions(first, executing, waiting, 200);
    const final = appendTransitions(middle, waiting, executing, 300);

    expect(final.filter((entry) => entry.message === "changing started")).toHaveLength(2);
  });

  it("orders simultaneous transitions by level then id, even when the wire lists the dependent stage first (mimicking read_dir order)", () => {
    const base = fixture.status.stages[0];
    const stageA = { ...base, id: "a", status: "waiting-for-deps" as const, dependencies: [] };
    const stageB = { ...base, id: "b", status: "waiting-for-deps" as const, dependencies: ["a"] };
    const previous: Snapshot = {
      ...fixture,
      status: { ...fixture.status, stages: [stageA, stageB] },
    };
    // `b` (level 1, depends on `a`) is listed before `a` (level 0) here, as
    // an unsorted directory listing could order them.
    const next: Snapshot = {
      ...fixture,
      status: {
        ...fixture.status,
        stages: [
          { ...stageB, status: "executing" },
          { ...stageA, status: "executing" },
        ],
      },
    };

    const entries = appendTransitions([], previous, next, 100);

    expect(entries.map((entry) => entry.stageId)).toEqual(["a", "b"]);
  });

  it("retains the newest twenty transitions", () => {
    // Zero-padded so id order (what appendTransitions now sorts by) matches
    // numeric order; ordering itself is covered by the test above.
    const stages = Array.from({ length: 25 }, (_, index) => ({
      ...fixture.status.stages[0],
      id: `stage-${String(index).padStart(2, "0")}`,
      status: "executing" as const,
      dependencies: [],
    }));
    const previous: Snapshot = {
      ...fixture,
      status: {
        ...fixture.status,
        stages: stages.map((stage) => ({ ...stage, status: "queued" as const })),
      },
    };
    const next: Snapshot = { ...fixture, status: { ...fixture.status, stages } };
    const entries = appendTransitions([], previous, next, 100);

    expect(entries).toHaveLength(MAX_ACTIVITY_ENTRIES);
    expect(entries[0].stageId).toBe("stage-05");
    expect(entries.at(-1)?.stageId).toBe("stage-24");
  });
});
