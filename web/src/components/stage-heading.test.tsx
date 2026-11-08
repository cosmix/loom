import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { StageHeading } from "@/components/stage-heading";
import { completionBlocker } from "@/test/completion-blocker";

const server = snapshotSchema.parse(fixture).status.stages.find((stage) => stage.id === "server")!;

function badge(label: string): HTMLElement {
  const element = screen.getByText(label).parentElement;
  if (!element) throw new Error(`the ${label} badge is missing`);
  return element;
}

afterEach(cleanup);

describe("StageHeading state line", () => {
  it("heads a completion-pending executing stage with the blocker and its reason", () => {
    const stage: StageSummary = { ...server, completion_blocker: completionBlocker() };
    render(<StageHeading stage={stage} />);

    const pending = badge("Completion pending");
    expect(pending.classList.contains("tone-warning")).toBe(true);
    expect(pending.getAttribute("title")).toBe("completion pending: test-integrity gate failed");
    expect(screen.queryByText("Executing")).toBeNull();
  });

  it("heads an executing stage without a blocker as executing", () => {
    render(<StageHeading stage={server} />);

    expect(badge("Executing").classList.contains("tone-executing")).toBe(true);
    expect(screen.queryByText("Completion pending")).toBeNull();
  });
});
