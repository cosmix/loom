import type { Node, NodeProps } from "@xyflow/react";

export interface RankNodeData extends Record<string, unknown> {
  rank: number;
}

export type RankNodeType = Node<RankNodeData, "rank">;

/// The row caption in the margin beside each dependency level, drawn as a
/// node so it pans and zooms with the sheet.
export function RankNode({ data }: NodeProps<RankNodeType>) {
  return (
    <div className="rank-caption" aria-hidden="true">
      level {data.rank}
    </div>
  );
}
