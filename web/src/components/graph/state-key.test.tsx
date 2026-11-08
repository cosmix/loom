import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { StateKey } from "@/components/graph/state-key";
import { completionBlocker } from "@/test/completion-blocker";

const template = snapshotSchema.parse(fixture).status.stages[0]!;

function stage(id: string, overrides: Partial<StageSummary> = {}): StageSummary {
  return { ...template, id, ...overrides };
}

function renderKey(stages: StageSummary[]) {
  render(<StateKey stages={stages} pinned={null} onHover={() => {}} onPin={() => {}} />);
}

afterEach(cleanup);

describe("StateKey contract chip", () => {
  it("counts the stages writing contracts in a non-interactive chip", () => {
    renderKey([
      stage("a", { status: "executing", session_type: "contract" }),
      stage("b", { status: "executing", session_type: "stage" }),
    ]);

    const chip = screen.getByText("writing contracts").closest(".key-chip");
    expect(chip?.tagName).toBe("SPAN");
    expect(chip?.classList.contains("tone-contract")).toBe(true);
    expect(chip?.querySelector(".key-count")?.textContent).toBe("1");
  });

  it("counts stages testing contracts apart from those still writing", () => {
    const testing = { done: 9, total: 11, queued: false, running_secs: 372 };
    renderKey([
      stage("a", { status: "executing", session_type: "contract" }),
      stage("b", { status: "executing", session_type: "contract", contract_testing: testing }),
    ]);

    const writing = screen.getByText("writing contracts").closest(".key-chip");
    const chip = screen.getByText("testing contracts").closest(".key-chip");
    expect(writing?.querySelector(".key-count")?.textContent).toBe("1");
    expect(chip?.tagName).toBe("SPAN");
    expect(chip?.classList.contains("tone-contract")).toBe(true);
    expect(chip?.querySelector(".key-count")?.textContent).toBe("1");
  });

  it("shows no such chip when no stage is in the contract phase", () => {
    renderKey([stage("a", { status: "executing", session_type: "stage" })]);

    expect(screen.queryByText("writing contracts")).toBeNull();
  });
});

describe("StateKey completion chip", () => {
  it("counts a completion-pending stage apart from the executing ones", () => {
    renderKey([
      stage("a", { status: "executing", session_type: "stage" }),
      stage("b", { status: "executing", completion_blocker: completionBlocker() }),
    ]);

    const pending = screen.getByText("Completion pending").closest(".key-chip");
    const executing = screen.getByText("Executing").closest(".key-chip");
    expect(pending?.classList.contains("tone-warning")).toBe(true);
    expect(pending?.querySelector(".key-count")?.textContent).toBe("1");
    expect(executing?.classList.contains("tone-executing")).toBe(true);
    expect(executing?.querySelector(".key-count")?.textContent).toBe("1");
  });
});
