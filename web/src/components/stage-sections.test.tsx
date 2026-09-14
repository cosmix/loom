import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { stageSections } from "@/components/stage-sections";

const fixture = snapshotSchema.parse(fixtureJson);

function fixtureStage(overrides: Partial<StageSummary> = {}): StageSummary {
  const stage = fixture.status.stages.find((candidate) => candidate.id === "server");
  if (!stage) throw new Error("fixture server stage is missing");
  return { ...stage, ...overrides };
}

function sessionLabels(stage: StageSummary): string[] {
  const session = stageSections(stage, null).find((section) => section.title === "session");
  return session?.rows.map((entry) => entry.label) ?? [];
}

function rowValue(stage: StageSummary, sectionTitle: string, label: string) {
  const section = stageSections(stage, null).find((candidate) => candidate.title === sectionTitle);
  return section?.rows.find((entry) => entry.label === label)?.value;
}

describe("stage session details", () => {
  it("omits the generated tool activity that repeats last tool", () => {
    const stage = fixtureStage({ last_tool: "Bash", last_activity: "Tool executed: Bash" });

    expect(sessionLabels(stage)).toContain("last tool");
    expect(sessionLabels(stage)).not.toContain("last activity");
  });

  it("keeps a non-tool heartbeat activity", () => {
    const stage = fixtureStage({ last_tool: null, last_activity: "Session started" });

    expect(sessionLabels(stage)).toContain("last activity");
  });

  it("shows the completion blocker evidence and next action", () => {
    const stage = fixtureStage({
      completion_blocker: {
        state: "blocked",
        fingerprint: "fp-blocked",
        failure_code: "criteria-blocked",
        summary: "Completion verification failed repeatedly",
        commit: "abc123",
        repeat_count: 3,
        first_observed_at: "2026-09-14T08:00:00Z",
        last_observed_at: "2026-09-14T08:05:00Z",
        next_action: "review the completion evidence",
      },
    });
    const completion = stageSections(stage, null).find((section) => section.title === "completion");

    expect(completion?.rows.map((entry) => entry.label)).toEqual([
      "state",
      "fingerprint",
      "failure code",
      "repeats",
      "commit",
      "first seen",
      "last seen",
      "next action",
    ]);
    expect(rowValue(stage, "completion", "next action")).toBe("review the completion evidence");
  });

  it("keeps context ceiling and stalled exit reasons distinct from context measurements", () => {
    const contextCeiling = fixtureStage({ outgoing_session_exit_reason: "context-ceiling" });
    const stalled = fixtureStage({ outgoing_session_exit_reason: "stalled" });
    const contextLabels =
      stageSections(contextCeiling, null)
        .find((section) => section.title === "stage orchestrator context")
        ?.rows.map((entry) => entry.label) ?? [];

    expect(rowValue(contextCeiling, "session", "exit reason")).toBe("context ceiling");
    expect(rowValue(stalled, "session", "exit reason")).toBe("stalled");
    expect(contextLabels).not.toContain("exit reason");
  });
});
