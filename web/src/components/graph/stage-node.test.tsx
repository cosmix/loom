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
    data: { stage, index: 0, emphasis: "plain" },
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
  it("tags a contract-writer stage and marks its card with the contract phase", () => {
    const { container, getByText } = renderNode(
      stage({ status: "executing", session_type: "contract" }),
    );

    expect(getByText("contracts")).toBeTruthy();
    expect(container.querySelector('.stage-node[data-phase="contract"]')).toBeTruthy();
  });

  it("shows neither the tag nor the phase marker for a plain executing stage", () => {
    const { container, queryByText } = renderNode(
      stage({ status: "executing", session_type: "stage" }),
    );

    expect(queryByText("contracts")).toBeNull();
    expect(container.querySelector(".stage-node")?.getAttribute("data-phase")).toBeNull();
  });
});
