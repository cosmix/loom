import { adoptUserNodes } from "@xyflow/system";
import { Position, type InternalNode } from "@xyflow/react";
import { describe, expect, it } from "vitest";

import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, STAGE_STATUSES, type StageStatus } from "@/api/schema";
import { buildEdges, buildNodes } from "@/components/graph/stage-graph";
import { layoutStages } from "@/lib/graph";

const stages = snapshotSchema.parse(fixture).status.stages;

describe("buildNodes", () => {
  it("keeps the handle bounds React Flow needs to draw edges when the frame is rebuilt", () => {
    // Fails if `measured` is dropped from the built nodes: without it,
    // React Flow's adoptUserNodes wipes handleBounds on every rebuilt frame.
    const layout = layoutStages(stages);
    const nodeLookup = new Map<string, InternalNode>();
    const parentLookup = new Map();

    adoptUserNodes(buildNodes(layout, stages, null), nodeLookup, parentLookup, {});

    // Simulate what the ResizeObserver path writes after first measuring.
    for (const [id, node] of nodeLookup) {
      node.internals.handleBounds = {
        source: [
          {
            id: null,
            nodeId: id,
            type: "source",
            position: Position.Bottom,
            x: 0,
            y: 0,
            width: 9,
            height: 9,
          },
        ],
        target: [
          {
            id: null,
            nodeId: id,
            type: "target",
            position: Position.Top,
            x: 0,
            y: 0,
            width: 9,
            height: 9,
          },
        ],
      };
    }

    // A fresh frame: a new node array built the same way a snapshot update does.
    adoptUserNodes(buildNodes(layout, stages, null), nodeLookup, parentLookup, {});

    for (const node of nodeLookup.values()) {
      expect(node.internals.handleBounds).toBeDefined();
    }
  });
});

describe("focus on a status no stage is in", () => {
  const missingStatus = STAGE_STATUSES.find(
    (status) => !stages.some((stage) => stage.status === status),
  );

  it("dims nothing: every node and edge is left plain", () => {
    // Guard the guard: an undefined status here would match no stage either,
    // and the assertions below would hold for the wrong reason.
    expect(missingStatus).toBeDefined();

    const layout = layoutStages(stages);
    const focus = { kind: "status" as const, status: missingStatus as StageStatus };

    const nodes = buildNodes(layout, stages, focus);
    const edges = buildEdges(layout, stages, focus);

    for (const node of nodes) {
      if (node.type === "stage") expect(node.data.emphasis).toBe("plain");
    }
    for (const edge of edges) {
      expect(edge.data?.emphasis).toBe("plain");
    }
  });
});
