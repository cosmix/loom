import {
  Background,
  BackgroundVariant,
  Handle,
  Position,
  ReactFlow,
  ReactFlowProvider,
  type EdgeTypes,
  type Node,
  type NodeMouseHandler,
  type NodeProps,
  type NodeTypes,
} from "@xyflow/react";
import { useMemo } from "react";

import { stageTypeSchema, type StageSummary } from "@/api/schema";
// Edges are built here: importing buildEdges from components/graph/stage-graph
// drags the dashboard's stage modal and terminal into the plan bundle.
import { ThreadEdge, type ThreadEdgeType } from "@/components/graph/thread-edge";
import { layoutStages, threadOf } from "@/lib/graph";
import type { StageRow } from "@/plan/api/schema-files";
import { Untrusted } from "@/plan/chat/visible-controls";

type PlanNode = Node<{ stage: StageSummary }, "plan-stage">;

/// A planned stage has no live state: every field the dashboard reads for a
/// running stage takes its neutral value, so threads draw as hairlines.
export function toSummary(row: StageRow): StageSummary {
  return {
    id: row.id,
    name: row.name,
    description: row.description,
    summary: row.summary,
    status: "waiting-for-deps",
    stage_type: stageTypeSchema.safeParse(row.stage_type).data ?? "standard",
    dependencies: row.dependencies,
    context_tokens: null,
    elapsed_secs: null,
    execution_secs: null,
    base_branch: null,
    base_merged_from: [],
    failure_info: null,
    activity_status: "Idle",
    last_tool: null,
    last_activity: null,
    staleness_secs: null,
    context_ceiling_tokens: null,
    review_reason: null,
    review_notes: null,
    merged: false,
    merge_assumed: false,
    held: false,
    retry_count: 0,
    max_retries: null,
    pid: null,
    session_alive: false,
    model: "",
    session_type: null,
    incoherence: null,
    execution_models: [],
    dispute_count: 0,
    judge_heartbeat_secs: null,
    session_backend: null,
    history: [],
  };
}

function PlanStageNode({ data }: NodeProps<PlanNode>) {
  return (
    <div className="flex h-full flex-col justify-center rounded-md bg-card px-3">
      <Handle
        type="target"
        position={Position.Top}
        isConnectable={false}
        className="stage-handle"
      />
      <div className="truncate text-[13px] font-medium">
        <Untrusted text={data.stage.name} />
      </div>
      <div className="truncate font-mono text-xs text-muted-foreground">
        <Untrusted text={data.stage.id} />
      </div>
      <Handle
        type="source"
        position={Position.Bottom}
        isConnectable={false}
        className="stage-handle"
      />
    </div>
  );
}

const nodeTypes: NodeTypes = { "plan-stage": PlanStageNode };
const edgeTypes: EdgeTypes = { thread: ThreadEdge };

function useStageFlow(stages: StageRow[]): { nodes: PlanNode[]; edges: ThreadEdgeType[] } {
  const summaries = useMemo(() => stages.map(toSummary), [stages]);
  const layout = useMemo(() => layoutStages(summaries), [summaries]);
  const byId = useMemo(() => {
    const result = new Map<string, StageSummary>();
    for (const stage of summaries) if (!result.has(stage.id)) result.set(stage.id, stage);
    return result;
  }, [summaries]);
  const nodes = useMemo<PlanNode[]>(
    () =>
      layout.nodes.map(({ id, x, y, width, height }) => ({
        id,
        type: "plan-stage",
        position: { x, y },
        width,
        height,
        measured: { width, height },
        data: { stage: byId.get(id)! },
      })),
    [layout, byId],
  );
  const edges = useMemo<ThreadEdgeType[]>(() => {
    return layout.edges.map(({ source, target, points }) => ({
      id: `${source}->${target}`,
      source,
      target,
      type: "thread",
      data: { thread: threadOf(byId.get(source)!), emphasis: "plain", points },
    }));
  }, [layout, byId]);
  return { nodes, edges };
}

// React Flow's stylesheet is unlayered and sets `touch-action: none` on the pane, which would
// trap touch scrolling of the column on a phone. A layered utility loses to it whatever its
// specificity, so the override is `!important`. Tailwind reads `_` in a variant as a space; the
// backslashes keep the two in `react-flow__pane` (String.raw leaves them in the class name).
const WRAPPER_CLASS = String.raw`sheet h-80 rounded-md border border-(--hairline) [&_.react-flow\_\_pane]:[touch-action:pan-y_pinch-zoom]!`;

/// The planned stages as a dependency graph. Clicking a node reports its id.
export function StageGraph({
  stages,
  onSelect,
}: {
  stages: StageRow[];
  onSelect: (id: string) => void;
}) {
  const { nodes, edges } = useStageFlow(stages);
  const onNodeClick: NodeMouseHandler<PlanNode> = (_, node) => onSelect(node.id);
  return (
    <div className={WRAPPER_CLASS} data-testid="stage-graph">
      <ReactFlowProvider>
        <ReactFlow<PlanNode, ThreadEdgeType>
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          fitView
          fitViewOptions={{ padding: 0.12, maxZoom: 1 }}
          minZoom={0.2}
          maxZoom={1.6}
          // Read-only: nodes are clicked, never dragged, and a finger that starts on the graph
          // must scroll the reading column. Pinch and ctrl-wheel still zoom.
          panOnDrag={false}
          nodesDraggable={false}
          nodesConnectable={false}
          edgesFocusable={false}
          zoomOnDoubleClick={false}
          // The graph sits inside the scrolling reading column: a plain wheel must scroll the
          // column. Pinch and ctrl-wheel still zoom.
          zoomOnScroll={false}
          preventScrolling={false}
          onNodeClick={onNodeClick}
        >
          <Background variant={BackgroundVariant.Lines} gap={24} color="var(--sheet-rule)" />
        </ReactFlow>
      </ReactFlowProvider>
    </div>
  );
}
