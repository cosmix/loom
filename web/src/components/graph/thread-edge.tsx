import { BaseEdge, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";

import type { Emphasis } from "@/components/graph/context";
import { toneClass } from "@/components/state-badge";
import type { Thread } from "@/lib/graph";

export interface ThreadEdgeData extends Record<string, unknown> {
  thread: Thread;
  emphasis: Emphasis;
}

export type ThreadEdgeType = Edge<ThreadEdgeData, "thread">;

/// A dependency drawn as a thread from the dependency's right anchor to the
/// dependent's left one. Colour and dash come from the source stage's state;
/// a running thread carries a second, moving dash over the base line.
export function ThreadEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
}: EdgeProps<ThreadEdgeType>) {
  const [path] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    curvature: 0.32,
  });
  const thread = data?.thread ?? { tone: "pending", style: "hairline" };
  const emphasis = data?.emphasis ?? "plain";
  return (
    <g className={toneClass(thread.tone)} data-thread={thread.style} data-emphasis={emphasis}>
      <BaseEdge id={id} path={path} className="thread-base" interactionWidth={0} />
      {thread.style === "running" && <path d={path} className="thread-run" />}
    </g>
  );
}
