import { cn } from "cn";
import { MaximizeIcon } from "lucide-react";
import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import type { CSSProperties } from "react";

import type { StageFlow, StageSummary } from "@/api/schema";
import { ActivityRoundel } from "@/components/activity-roundel";
import { ContextMeter } from "@/components/context-meter";
import { useGraphActions, type Emphasis } from "@/components/graph/context";
import { StateBadge, toneClass } from "@/components/state-badge";
import { Sparkline } from "@/components/sparkline";
import { TerminalGlyph } from "@/components/terminal/terminal-glyph";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  activityText,
  hazardTone,
  isContractPhase,
  mergeText,
  stateMeta,
  timeText,
} from "@/lib/format";
import { hasFooter, NODE_WIDTH, nodeHeight } from "@/lib/graph";
import { compactTokens, fresh } from "@/lib/tokens";

export interface StageNodeData extends Record<string, unknown> {
  stage: StageSummary;
  /// Position in the reveal sequence, for the staggered first paint.
  index: number;
  emphasis: Emphasis;
  /// The stage's token flow; the layout reserved the card's token line for it.
  tokens: StageFlow | null;
}

export type StageNodeType = Node<StageNodeData, "stage">;

/// Statuses whose card gets a tinted ground so they read at a glance.
const ATTENTION = new Set<StageSummary["status"]>([
  "blocked",
  "completed-with-failures",
  "merge-conflict",
  "merge-blocked",
  "needs-human-review",
  "needs-adjudication",
  "waiting-for-input",
  "needs-handoff",
]);

/// The stage type, shown only when it is not the plain kind.
const TYPE_TAG: Partial<Record<StageSummary["stage_type"], string>> = {
  knowledge: "knowledge",
  "integration-verify": "verify",
  "knowledge-distill": "distill",
};

/// An index card on the sheet: state and type along the top with the
/// terminal glyph in the corner, name and id, then the live row. The card
/// re-keys on status so a change washes it.
export function StageNode({ data }: NodeProps<StageNodeType>) {
  const { stage, index, emphasis, tokens } = data;
  const { open, openTerminal } = useGraphActions();
  const tone = stateMeta(stage.status).tone;
  const contractPhase = isContractPhase(stage);
  const style = {
    "--i": index,
    width: NODE_WIDTH,
    height: nodeHeight(stage, tokens !== null),
  } as CSSProperties;

  return (
    <div
      className={cn("stage-node", toneClass(contractPhase ? "contract" : tone))}
      style={style}
      data-emphasis={emphasis}
      data-live={stage.status === "executing" || undefined}
      data-attention={ATTENTION.has(stage.status) || undefined}
      data-hazard={hazardTone(stage.status) ?? undefined}
    >
      <Handle
        type="target"
        position={Position.Top}
        isConnectable={false}
        className="stage-handle"
      />
      <div key={stage.status} className="stage-card">
        <header className="flex items-center gap-1.5">
          <StateBadge status={stage.status} className="text-xs" />
          {TYPE_TAG[stage.stage_type] && (
            <span className="stage-tag">{TYPE_TAG[stage.stage_type]}</span>
          )}
          {stage.held && <span className={cn("stage-tag", toneClass("warning"))}>held</span>}
          {stage.incoherence !== null && (
            <Tooltip>
              <TooltipTrigger asChild>
                <span className={cn("stage-tag cursor-help", toneClass("blocked"))}>
                  incoherent
                </span>
              </TooltipTrigger>
              <TooltipContent className="font-mono text-[11px]">{stage.incoherence}</TooltipContent>
            </Tooltip>
          )}
          <button
            type="button"
            className="stage-open nodrag"
            aria-label={`open ${stage.name}`}
            onClick={(event) => {
              event.stopPropagation();
              open(stage.id);
            }}
          >
            <MaximizeIcon className="size-3" />
          </button>
          <TerminalGlyph stage={stage} onOpen={openTerminal} className="nodrag" />
        </header>
        <p className="mt-1.5 truncate text-[15px] leading-5 font-medium text-foreground">
          {stage.name}
        </p>
        <p className="truncate font-mono text-[11px] leading-4 text-muted-foreground">{stage.id}</p>
        {(hasFooter(stage) || tokens !== null) && <Footer stage={stage} tokens={tokens} />}
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

/// Activity, merge and time on one line; the context meter, when the stage
/// has one, and the token flow, each on a line of its own so the activity
/// text keeps its room.
function Footer({ stage, tokens }: { stage: StageSummary; tokens: StageFlow | null }) {
  const activity = activityText(stage);
  const merge = mergeText(stage);
  const time = timeText(stage);
  return (
    <footer className="mt-auto flex flex-col gap-0.5 text-[11px] leading-4">
      <div className="flex items-center gap-2">
        <ActivityRoundel stage={stage} size={13} />
        {activity && (
          <span
            className={cn("min-w-0 truncate", toneClass(activity.tone))}
            title={stage.completion_blocker?.next_action}
          >
            {activity.text}
          </span>
        )}
        {merge && <span className={cn("shrink-0", toneClass(merge.tone))}>{merge.text}</span>}
        {time && (
          <span className="ml-auto shrink-0 font-mono text-muted-foreground tabular-nums">
            {time}
          </span>
        )}
      </div>
      <ContextMeter stage={stage} className="shrink-0" />
      {tokens && <TokenLine stage={stage} tokens={tokens} />}
    </footer>
  );
}

/// An executing stage shows its rate beside a sparkline; any other stage
/// shows what it spent in all.
function TokenLine({ stage, tokens }: { stage: StageSummary; tokens: StageFlow }) {
  if (stage.status !== "executing") {
    return (
      <div className="text-muted-foreground tabular-nums">
        {compactTokens(fresh(tokens.total))} total
      </div>
    );
  }
  return (
    <div className="flex items-center gap-2 tabular-nums">
      <Sparkline
        values={tokens.series.slice(-20)}
        width={56}
        height={12}
        label={`${stage.name} tokens, last 10 min`}
        className="shrink-0 text-(--tone-executing)"
      />
      <span className="font-mono">{compactTokens(fresh(tokens.rate))}/m</span>
    </div>
  );
}
