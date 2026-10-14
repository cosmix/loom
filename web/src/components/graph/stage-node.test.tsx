import { cleanup, render } from "@testing-library/react";
import { ReactFlowProvider, type NodeProps } from "@xyflow/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageFlow, type StageSummary } from "@/api/schema";
import { StageNode, type StageNodeType } from "@/components/graph/stage-node";

const status = snapshotSchema.parse(fixture).status;
const template = status.stages[0]!;
const flow = status.tokens!.stages[0]!;

function stage(overrides: Partial<StageSummary> = {}): StageSummary {
  return { ...template, ...overrides };
}

function renderNode(stage: StageSummary, tokens: StageFlow | null = null) {
  const props: NodeProps<StageNodeType> = {
    id: stage.id,
    data: { stage, index: 0, emphasis: "plain", tokens },
    type: "stage",
    dragging: false,
    zIndex: 0,
    selectable: true,
    deletable: true,
    selected: false,
    draggable: true,
    isConnectable: false,
    positionAbsoluteX: 0,
    positionAbsoluteY: 0,
  };
  return render(
    <ReactFlowProvider>
      <StageNode {...props} />
    </ReactFlowProvider>,
  );
}

afterEach(cleanup);

describe("StageNode contract phase", () => {
  it("glows in the contract tone with no tag while a contract writer runs", () => {
    const { container, queryByText } = renderNode(
      stage({ status: "executing", session_type: "contract" }),
    );

    const node = container.querySelector(".stage-node");
    expect(node?.classList.contains("tone-contract")).toBe(true);
    expect(queryByText("contracts")).toBeNull();
  });

  it("keeps the executing tone for a plain executing stage", () => {
    const { container } = renderNode(stage({ status: "executing", session_type: "stage" }));

    const node = container.querySelector(".stage-node");
    expect(node?.classList.contains("tone-executing")).toBe(true);
    expect(node?.classList.contains("tone-contract")).toBe(false);
  });
});

describe("StageNode tray", () => {
  it("draws no tray on a card with nothing live and no token flow", () => {
    const { container } = renderNode(stage({ status: "waiting-for-deps", elapsed_secs: null }));

    expect(container.querySelector(".stage-tray")).toBeNull();
  });

  it("puts the token row under the activity line when the stage has both", () => {
    const { container } = renderNode(stage({ status: "executing" }), flow);

    const rows = container.querySelector(".stage-tray")?.children ?? [];
    expect(rows[0]?.classList.contains("node-tokens")).toBe(false);
    expect(Array.from(rows).at(-1)?.classList.contains("node-tokens")).toBe(true);
  });

  it("shows the token row alone on a card with no other live line", () => {
    const { container } = renderNode(
      stage({ status: "waiting-for-deps", elapsed_secs: null }),
      flow,
    );

    const tray = container.querySelector(".stage-tray");
    expect(tray?.children).toHaveLength(1);
    expect(tray?.textContent).toContain("2.23M total");
  });

  it("names a merge conflict once on the activity line", () => {
    const { container } = renderNode(stage({ status: "merge-conflict" }));

    const tray = container.querySelector(".stage-tray");
    expect(tray?.textContent?.match(/conflict/g)).toHaveLength(1);
  });
});
