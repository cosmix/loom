import { useMemo } from "react";
import { MemoryRouter, useInRouterContext } from "react-router";
import { Rows3Icon, WorkflowIcon } from "lucide-react";
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
import { StateKey } from "@/components/graph/state-key";
import { LedgerTable } from "@/components/ledger-table";
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
const verify = {
  ...root,
  id: "verify",
  name: "Integration tests",
  status: "waiting-for-deps" as const,
  dependencies: ["api"],
  merged: false,
  activity_status: "Idle" as const,
};
const DEMO_STAGES = [root, api, verify];
const LEDGER_SNAPSHOT = {
  ...HELP_SNAPSHOT,
  status: { ...HELP_SNAPSHOT.status, stages: DEMO_STAGES },
};
const SINGLE_STAGE = [HELP_STAGE];
const nodeTypes = { stage: StageNode };
const edgeTypes = { thread: ThreadEdge };

export function GraphDemo({ single = false }: { single?: boolean }) {
  return (
    <HelpExample>
      <div className="help-graph-figure">
        {!single && (
          <div inert>
            <StateKey stages={DEMO_STAGES} pinned={null} onHover={() => {}} onPin={() => {}} />
          </div>
        )}
        <ReactFlowProvider>
          <ExampleCanvas single={single} />
        </ReactFlowProvider>
      </div>
    </HelpExample>
  );
}

/// The ledger over the same three stages. Its rows link into the app, so a
/// render outside the app's router (tests) supplies an inert one.
export function LedgerDemo() {
  const routed = useInRouterContext();
  const table = (
    <HelpExample snapshot={LEDGER_SNAPSHOT}>
      <div className="help-ledger" inert>
        <span className="view-switch" data-pin="views">
          <span className="view-tab">
            <WorkflowIcon className="size-3.5" />
            graph
          </span>
          <span className="view-tab is-active">
            <Rows3Icon className="size-3.5" />
            ledger
          </span>
        </span>
        <LedgerTable />
      </div>
    </HelpExample>
  );
  return routed ? table : <MemoryRouter>{table}</MemoryRouter>;
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
    : "Build the API depends on Define the schema and unlocks Integration tests.";
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
    </>
  );
}
