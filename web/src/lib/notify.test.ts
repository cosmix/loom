import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageSummary, type StageStatus } from "@/api/schema";
import { notifiableEvents, RUN_FINISHED_KEY } from "@/lib/notify";

const fixture = snapshotSchema.parse(fixtureJson);

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

function attentionFor(stage: StageSummary, label: string) {
  return {
    ...fixture.attention[0]!,
    id: stage.id,
    name: stage.name,
    label,
    hint: "loom stage resume example",
  };
}

describe("notifiableEvents", () => {
  it("treats the first frame as a baseline", () => {
    expect(notifiableEvents(null, fixture)).toEqual([]);
  });

  it("does not repeat attention that remains present", () => {
    expect(notifiableEvents(fixture, structuredClone(fixture))).toEqual([]);
  });

  it("emits a newly arrived attention entry without its hint", () => {
    const previous = structuredClone(fixture);
    const next = structuredClone(fixture);
    const stage = next.status.stages[0]!;
    const entry = attentionFor(stage, "NEEDS INPUT");
    next.attention.push(entry);

    const events = notifiableEvents(previous, next);

    expect(events).toEqual([
      {
        key: `attention:${stage.id}:NEEDS INPUT`,
        title: "loom — needs input",
        body: `${stage.name} (${stage.id})`,
      },
    ]);
    expect(events[0]!.body).not.toContain(entry.hint);
  });

  it("emits attention again when the same stage receives a different label", () => {
    const previous = structuredClone(fixture);
    const next = structuredClone(fixture);
    const stage = next.status.stages[0]!;
    previous.attention = [attentionFor(stage, "NEEDS REVIEW")];
    next.attention = [attentionFor(stage, "COMPLETION BLOCKED")];

    expect(notifiableEvents(previous, next)).toMatchObject([
      { key: `attention:${stage.id}:COMPLETION BLOCKED` },
    ]);
  });

  it("ignores completion pending until it becomes completion blocked", () => {
    const previous = structuredClone(fixture);
    const pending = structuredClone(fixture);
    const stage = pending.status.stages[0]!;
    pending.attention = [attentionFor(stage, "COMPLETION PENDING")];
    const blocked = structuredClone(pending);
    blocked.attention = [attentionFor(stage, "COMPLETION BLOCKED")];

    expect(notifiableEvents(previous, pending)).toEqual([]);
    expect(notifiableEvents(pending, blocked)).toMatchObject([
      { key: `attention:${stage.id}:COMPLETION BLOCKED` },
    ]);
  });

  it("emits a handoff only when a stage enters needs-handoff", () => {
    const previous = structuredClone(fixture);
    const next = structuredClone(fixture);
    const stage = next.status.stages[0]!;
    next.status.stages[0] = { ...stage, status: "needs-handoff" };
    const following = structuredClone(next);

    expect(notifiableEvents(previous, next)).toEqual([
      {
        key: `handoff:${stage.id}`,
        title: "loom — needs handoff",
        body: `${stage.name} (${stage.id}) needs a handoff`,
      },
    ]);
    expect(notifiableEvents(next, following)).toEqual([]);
  });

  it("mid-run merge does not announce a finished run", () => {
    const previous = structuredClone(fixture);
    const next = structuredClone(fixture);
    previous.status.merge.pending = ["a"];
    next.status.merge.pending = [];
    next.status.merge.conflicts = [];
    previous.status.stages[0] = { ...previous.status.stages[0]!, status: "executing" };
    next.status.stages[0] = { ...next.status.stages[0]!, status: "executing" };

    expect(notifiableEvents(previous, next)).not.toContainEqual(
      expect.objectContaining({ key: RUN_FINISHED_KEY }),
    );
  });

  it("announces a newly settled run after its final merge", () => {
    const previous = settledSnapshot();
    previous.status.stages[0] = { ...previous.status.stages[0]!, merged: false };
    const next = settledSnapshot();

    expect(notifiableEvents(previous, next)).toEqual([
      {
        key: RUN_FINISHED_KEY,
        title: "loom — run finished",
        body: `${next.status.plan_name ?? "the run"} finished; 7 stages merged`,
      },
    ]);
  });

  it("does not repeat a run-finished event for an already settled run", () => {
    const previous = settledSnapshot();
    const next = settledSnapshot();

    expect(notifiableEvents(previous, next)).toEqual([]);
  });

  it("does not announce when merge conflicts remain", () => {
    const previous = structuredClone(fixture);
    const next = settledSnapshot();
    next.status.merge.conflicts = ["docs"];

    expect(notifiableEvents(previous, next)).not.toContainEqual(
      expect.objectContaining({ key: RUN_FINISHED_KEY }),
    );
  });

  it("announces when the final executing stage completes between frames", () => {
    const previous = settledSnapshot();
    previous.status.stages[0] = {
      ...previous.status.stages[0]!,
      status: "executing",
      merged: false,
    };
    const next = settledSnapshot();

    expect(notifiableEvents(previous, next)).toMatchObject([{ key: RUN_FINISHED_KEY }]);
  });

  it.each<StageStatus>(["blocked", "merge-conflict", "merge-blocked", "completed-with-failures"])(
    "does not announce a run while a stage is %s",
    (status) => {
      const previous = structuredClone(fixture);
      const next = settledSnapshot();
      next.status.stages[0] = { ...next.status.stages[0]!, status };

      expect(notifiableEvents(previous, next)).not.toContainEqual(
        expect.objectContaining({ key: RUN_FINISHED_KEY }),
      );
    },
  );

  it("does not announce while a completed stage remains unmerged", () => {
    const previous = structuredClone(fixture);
    const next = settledSnapshot();
    next.status.stages[0] = { ...next.status.stages[0]!, merged: false };

    expect(notifiableEvents(previous, next)).not.toContainEqual(
      expect.objectContaining({ key: RUN_FINISHED_KEY }),
    );
  });

  it("announces when a knowledge-distill stage is the last one to settle", () => {
    const previous = settledSnapshot();
    previous.status.stages[0] = {
      ...previous.status.stages[0]!,
      stage_type: "knowledge-distill",
      status: "executing",
      merged: false,
    };
    const next = structuredClone(previous);
    next.status.stages[0] = {
      ...next.status.stages[0]!,
      status: "completed",
      merged: true,
    };

    expect(notifiableEvents(previous, next)).toMatchObject([{ key: RUN_FINISHED_KEY }]);
  });

  it("settles a run when the remaining stage is skipped rather than completed", () => {
    const previous = structuredClone(fixture);
    const next = settledSnapshot();
    next.status.stages[0] = { ...next.status.stages[0]!, status: "skipped", merged: false };

    expect(notifiableEvents(previous, next)).toEqual([
      {
        key: RUN_FINISHED_KEY,
        title: "loom — run finished",
        body: `${next.status.plan_name ?? "the run"} finished; 6 stages merged`,
      },
    ]);
  });
});
