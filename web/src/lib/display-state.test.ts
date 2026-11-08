import { describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { attentionHazard, attentionStatus } from "@/components/attention-status";
import { displayState } from "@/lib/display-state";
import { hazardTone, stateMeta, summaryCounts } from "@/lib/format";
import { threadOf } from "@/lib/graph";
import { DISPLAY_LEGEND, DISPLAY_STATES } from "@/lib/states";
import { completionBlocker } from "@/test/completion-blocker";

const snapshot = snapshotSchema.parse(fixture);
const server = snapshot.status.stages.find((stage) => stage.id === "server")!;

function pending(overrides: Partial<StageSummary> = {}): StageSummary {
  return { ...server, completion_blocker: completionBlocker(), ...overrides };
}

describe("displayState", () => {
  it("shows an executing stage's completion blocker in place of executing", () => {
    expect(server.status).toBe("executing");
    expect(displayState(pending())).toBe("completion-pending");
    expect(displayState(pending({ completion_blocker: completionBlocker("blocked") }))).toBe(
      "completion-blocked",
    );
  });

  it("keeps executing for an executing stage without a blocker", () => {
    expect(displayState(server)).toBe("executing");
  });

  it("keeps a parked stage's own state, which already asks for the operator", () => {
    const parked = pending({
      status: "needs-human-review",
      completion_blocker: completionBlocker("blocked"),
    });
    expect(displayState(parked)).toBe("needs-human-review");
  });

  it("draws completion pending in the attention family, labelled as loom status does", () => {
    expect(stateMeta("completion-pending").tone).toBe("warning");
    expect(hazardTone("completion-pending")).toBe("warning");
    expect(stateMeta("completion-blocked").tone).toBe("blocked");
    expect(hazardTone("completion-blocked")).toBe("error");
    expect(DISPLAY_STATES["completion-pending"].label).toBe("Completion pending");
    expect(DISPLAY_LEGEND.map((entry) => entry.status)).toContain("completion-pending");
  });

  it("styles the thread leaving a completion-pending stage as stopped", () => {
    expect(threadOf(pending())).toEqual({ tone: "warning", style: "dashed" });
    expect(threadOf(server)).toEqual({ tone: "executing", style: "running" });
  });

  it("counts a completion-pending stage under attention, not executing", () => {
    const stages = snapshot.status.stages;
    const executing = summaryCounts(stages, 0).executing;
    const swapped = stages.map((stage) => (stage.id === "server" ? pending() : stage));
    expect(summaryCounts(swapped, 0).executing).toBe(executing - 1);
  });

  it("gives the stage's attention entry the stage's completion state", () => {
    // `loom status` files a completion blocker's entry under the blocked status.
    const entry = {
      ...snapshot.attention[0]!,
      label: "COMPLETION PENDING",
      status: "blocked" as const,
      automatic: true,
    };
    expect(attentionStatus(entry, displayState(pending()))).toBe("completion-pending");
    expect(attentionStatus(entry, "needs-human-review")).toBe("blocked");
    expect(attentionStatus(entry, null)).toBe("blocked");
    expect(attentionHazard(entry)).toBe("warning");
  });
});
