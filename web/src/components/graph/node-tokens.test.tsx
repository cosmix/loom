import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type AgentFlow, type StageFlow, type StageSummary } from "@/api/schema";
import { AgentPips, NodeTokens } from "@/components/graph/node-tokens";

const status = snapshotSchema.parse(fixture).status;
const server = status.stages.find((stage) => stage.id === "server")!;
const flow = status.tokens!.stages.find((stage) => stage.stage_id === "server")!;

function renderTokens(stage: StageSummary, tokens: StageFlow = flow) {
  return render(<NodeTokens stage={stage} tokens={tokens} />);
}

function agents(count: number, live: number): AgentFlow[] {
  return Array.from({ length: count }, (_, index) => ({
    ...flow.agents[0]!,
    id: `agent-${index}`,
    active: index < live,
  }));
}

afterEach(cleanup);

describe("NodeTokens", () => {
  it("shows an executing stage's rate beside its trace and its live agents", () => {
    const { container } = renderTokens(server);

    expect(container.textContent).toContain("43.2k/m");
    const trace = screen.getByRole("img", { name: "Rust server tokens, last 10 min" });
    expect(trace.querySelector('path[fill="none"]')?.getAttribute("d")?.split(" L ")).toHaveLength(
      20,
    );
    expect(screen.getByRole("img", { name: "3 of 3 agents live" })).toBeTruthy();
  });

  it("left-pads a short series with zeros so the trace keeps its ten minutes", () => {
    renderTokens(server, { ...flow, series: [5] });

    const line = screen
      .getByRole("img", { name: "Rust server tokens, last 10 min" })
      .querySelector('path[fill="none"]')
      ?.getAttribute("d");
    const points = line?.split(" L ") ?? [];
    expect(points).toHaveLength(20);
    expect(points[0]).toBe("M 0 14");
    expect(points.at(-1)).toBe("108 0");
  });

  it("states what a stage that is not executing spent in all, with no trace", () => {
    const { container } = renderTokens({ ...server, status: "completed" });

    expect(container.textContent).toContain("2.23M total");
    expect(container.textContent).not.toContain("/m");
    expect(screen.queryAllByRole("img")).toHaveLength(0);
  });
});

describe("AgentPips", () => {
  it("draws one filled pip per live agent", () => {
    const { container } = render(<AgentPips agents={agents(3, 2)} />);

    expect(screen.getByRole("img", { name: "2 of 3 agents live" })).toBeTruthy();
    expect(container.querySelectorAll(".node-pip")).toHaveLength(2);
    expect(container.querySelectorAll(".node-pip[data-active]")).toHaveLength(2);
  });

  it("draws a single hollow pip when no agent is live", () => {
    const { container } = render(<AgentPips agents={agents(1, 0)} />);

    expect(screen.getByRole("img", { name: "0 of 1 agent live" })).toBeTruthy();
    expect(container.querySelectorAll(".node-pip")).toHaveLength(1);
    expect(container.querySelector(".node-pip[data-active]")).toBeNull();
  });

  it("collapses live agents past four into a count", () => {
    const { container } = render(<AgentPips agents={agents(9, 7)} />);

    expect(screen.getByRole("img", { name: "7 of 9 agents live" })).toBeTruthy();
    expect(container.querySelectorAll(".node-pip[data-active]")).toHaveLength(4);
    expect(container.textContent).toBe("+3");
  });

  it("renders nothing for a stage with no agents", () => {
    const { container } = render(<AgentPips agents={[]} />);

    expect(container.innerHTML).toBe("");
  });
});
