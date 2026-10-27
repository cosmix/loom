import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { StageSectionGrid, stageSections } from "@/components/stage-sections";
import { TooltipProvider } from "@/components/ui/tooltip";

afterEach(cleanup);

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

function renderSections(stage: StageSummary) {
  render(
    <TooltipProvider>
      <StageSectionGrid stage={stage} level={null} />
    </TooltipProvider>,
  );
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

describe("what the details leave to the now card", () => {
  it("lists no failure and no review notes, whatever the stage carries", () => {
    const failure = fixture.status.stages.find((stage) => stage.id === "client")?.failure_info;
    if (!failure) throw new Error("fixture client failure is missing");
    renderSections(
      fixtureStage({ status: "blocked", failure_info: failure, review_notes: "needs a look" }),
    );

    expect(screen.queryByText(/^failure · /)).toBeNull();
    expect(screen.queryByText("needs a look")).toBeNull();
    expect(screen.queryByText("review reason")).toBeNull();
  });
});

describe("a blocked stage's acceptance list", () => {
  it("numbers each criterion from 0, the index loom stage amend takes", () => {
    const blocked = fixture.status.stages.find((candidate) => candidate.id === "local-runtime");
    if (!blocked) throw new Error("fixture local-runtime stage is missing");

    renderSections(blocked);

    const items = within(screen.getByRole("region", { name: "acceptance criteria" })).getAllByRole(
      "listitem",
    );
    expect(items.map((item) => item.getAttribute("data-index"))).toEqual(["0", "1", "2", "3", "4"]);
    expect(items[4]?.textContent).toBe(
      "4TROOFOO_STACK=ephemeral bun run stack:ephemeral --with objectstore -- bun test packages/db/src/ephemeral-services",
    );
    expect(screen.getByText(/index for loom stage amend --index, counted from 0/)).toBeTruthy();
  });

  it("is absent for a stage that carries no criteria", () => {
    renderSections(fixtureStage());

    expect(screen.queryByRole("region", { name: "acceptance criteria" })).toBeNull();
  });
});
