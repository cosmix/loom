import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type Snapshot, type StageStatus } from "@/api/schema";
import { activityEntries } from "@/lib/activity";

const fixture = snapshotSchema.parse(fixtureJson);

function withEvents(...transitions: [from: StageStatus, to: StageStatus][]): Snapshot {
  const activity = transitions.map(([from, to], index) => ({
    at: new Date(Date.UTC(2026, 8, 4, 12, index)).toISOString(),
    stage_id: "changing",
    from,
    to,
    session: null,
    reason: null,
  }));
  return { ...fixture, status: { ...fixture.status, activity } };
}

describe("activityEntries", () => {
  it("distinguishes contract writing and implementation at their recorded times", () => {
    const snapshot = withEvents(["queued", "executing"], ["executing", "executing"]);
    snapshot.status.activity[0]!.session_type = "contract";
    snapshot.status.activity[1]!.session_type = "stage";
    // Parse the wire data too: losing session_type would collapse both messages.
    const entries = activityEntries(snapshotSchema.parse(snapshot));
    expect(entries.map((entry) => entry.message)).toEqual([
      "changing execution started",
      "changing contract writing started",
    ]);
    expect(entries[0]!.at - entries[1]!.at).toBe(60_000);
  });

  it("does not label a resolver session change as stage execution", () => {
    const snapshot = withEvents(["executing", "executing"]);
    snapshot.status.activity[0]!.session_type = "merge";
    expect(activityEntries(snapshot)).toEqual([]);
  });

  it("is empty without a snapshot", () => {
    expect(activityEntries(null)).toEqual([]);
  });

  it("lists the recorded history of a snapshot, newest first, at the event time", () => {
    expect(activityEntries(fixture)).toEqual([
      {
        at: Date.parse("2026-09-04T11:58:00Z"),
        stageId: "server",
        status: "queued",
        message: "server verdict applied, requeued",
      },
      {
        at: Date.parse("2026-09-04T11:50:00Z"),
        stageId: "server",
        status: "needs-adjudication",
        message: "server entered adjudication",
      },
    ]);
  });

  it("words each transition", () => {
    const entries = activityEntries(
      withEvents(
        ["queued", "executing"],
        ["executing", "needs-handoff"],
        ["executing", "blocked"],
        ["executing", "completed"],
        ["needs-adjudication", "needs-human-review"],
        ["completed", "queued"],
      ),
    );
    expect(entries.map((entry) => entry.message).reverse()).toEqual([
      "changing started",
      "changing needs handoff",
      "changing blocked",
      "changing completed",
      "changing needs human review",
      "changing ready",
    ]);
  });

  it("leaves out transitions that have no message", () => {
    expect(activityEntries(withEvents(["executing", "waiting-for-input"]))).toEqual([]);
  });
});
