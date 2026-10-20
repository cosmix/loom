import { useMemo } from "react";
import {
  Background,
  BackgroundVariant,
  Controls,
  ReactFlow,
  ReactFlowProvider,
} from "@xyflow/react";
import { StageNode, type StageNodeType } from "@/components/graph/stage-node";
import { ThreadEdge, type ThreadEdgeType } from "@/components/graph/thread-edge";
import { buildEdges, buildNodes } from "@/components/graph/stage-graph";
import { layoutStages } from "@/lib/graph";
import { HELP_SNAPSHOT, HELP_STAGE, HelpExample } from "@/components/help-example";
import { useMediaQuery } from "@/lib/use-media-query";

const root = {
  ...HELP_STAGE,
  id: "schema",
  name: "Define the schema",
  status: "completed" as const,
  dependencies: [],
  merged: true,
  context_tokens: null,
  session_alive: false,
  elapsed_secs: null,
};
const api = { ...HELP_STAGE, id: "api", name: "Build the API", dependencies: ["schema"] };
const docs = {
  ...HELP_STAGE,
  id: "docs",
  name: "Write the docs",
  dependencies: ["schema"],
  context_tokens: 120000,
};
const verify = {
  ...root,
  id: "verify",
  name: "Integration tests",
  status: "waiting-for-deps" as const,
  dependencies: ["api"],
  merged: false,
  activity_status: "Idle" as const,
};
const DEMO_STAGES = [root, api, docs, verify];
const SINGLE_STAGE = [HELP_STAGE];
const nodeTypes = { stage: StageNode };
const edgeTypes = { thread: ThreadEdge };

export function GraphDemo({ single = false }: { single?: boolean }) {
  return (
    <HelpExample>
      <figure className="help-figure help-graph-figure">
        <div className="help-figure-label">
          <span>{single ? "Reading a stage card" : "Dependency graph"}</span>
          <span>Static example</span>
        </div>
        <ReactFlowProvider>
          <ExampleCanvas single={single} />
        </ReactFlowProvider>
        {!single && (
          <dl className="help-control-key">
            <div>
              <dt>Click a card</dt>
              <dd>Trace its dependencies and dependents.</dd>
            </div>
            <div>
              <dt>+ / −</dt>
              <dd>Zoom in and out with the graph controls.</dd>
            </div>
            <div>
              <dt>Fit view</dt>
              <dd>Use the corner-frame control to see all stages.</dd>
            </div>
          </dl>
        )}
      </figure>
    </HelpExample>
  );
}

function useExampleGraph(single: boolean) {
  const mobile = useMediaQuery("(max-width: 767px)");
  const stages = single ? SINGLE_STAGE : DEMO_STAGES;
  const flow = single ? HELP_SNAPSHOT.status.tokens : undefined;
  const layout = useMemo(
    () => layoutStages(stages, new Set(flow?.stages.map((s) => s.stage_id))),
    [stages, flow],
  );
  const focus = single ? null : { kind: "stage" as const, id: "api" };
  const nodes = buildNodes(layout, stages, focus, flow).filter(
    (node): node is StageNodeType => node.type === "stage",
  );
  return { nodes, edges: buildEdges(layout, stages, focus), mobile };
}

function ExampleCanvas({ single }: { single: boolean }) {
  const { nodes, edges, mobile } = useExampleGraph(single);
  const description = single
    ? "Stage card with state, open and terminal controls, activity, context meter, and token trace."
    : "Build the API depends on Define the schema and unlocks Integration tests. The parallel Write the docs stage is dimmed.";
  return (
    <>
      <div
        className={`help-real-graph sheet ${single ? "help-single-stage" : ""}`}
        role="img"
        aria-label={description}
      >
        <div inert className="h-full">
          <ReactFlow<StageNodeType, ThreadEdgeType>
            id={single ? "help-stage-example" : "help-graph-example"}
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            fitView
            fitViewOptions={{ padding: mobile ? 0.08 : 0.12, maxZoom: 1 }}
            nodesDraggable={false}
            nodesConnectable={false}
            nodesFocusable={false}
            edgesFocusable={false}
            elementsSelectable={false}
            panOnDrag={false}
            panOnScroll={false}
            zoomOnScroll={false}
            zoomOnPinch={false}
            zoomOnDoubleClick={false}
            autoPanOnNodeFocus={false}
            preventScrolling={false}
            minZoom={0.25}
            maxZoom={1.6}
          >
            <Background variant={BackgroundVariant.Lines} gap={24} color="var(--sheet-rule)" />
            {!single && <Controls showInteractive={false} />}
          </ReactFlow>
        </div>
      </div>
      <figcaption>{description}</figcaption>
    </>
  );
}
