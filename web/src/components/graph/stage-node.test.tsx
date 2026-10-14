import { cleanup, render } from "@testing-library/react";
import { ReactFlowProvider, type NodeProps } from "@xyflow/react";
import { afterEach, describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { StageNode, type StageNodeType } from "@/components/graph/stage-node";

const template = snapshotSchema.parse(fixture).status.stages[0]!;

function stage(overrides: Partial<StageSummary> = {}): StageSummary {
  return { ...template, ...overrides };
}

function renderNode(stage: StageSummary) {
  const props: NodeProps<StageNodeType> = {
    id: stage.id,
    data: { stage, index: 0, emphasis: "plain", tokens: null },
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
