import { cn } from "cn";
import { useAtomValue } from "jotai/react";

import { useOpenStage } from "@/components/stage-modal";
import { toneClass } from "@/components/state-badge";
import { stateMeta } from "@/lib/format";
import { STAGE_STATES } from "@/lib/states";
import { orderedStagesAtom } from "@/state/atoms";

/// The plan as a warp: one segment per stage in dependency order, coloured
/// by state. A segment opens its stage.
export function StageStrip({ className }: { className?: string }) {
  const ordered = useAtomValue(orderedStagesAtom);
  const open = useOpenStage();
  if (ordered.length === 0) return null;
  return (
    <ol className={cn("stage-strip", className)} aria-label="stages in dependency order">
      {ordered.map(({ stage }) => (
        <li key={stage.id} className={toneClass(stateMeta(stage.status).tone)}>
          <button
            type="button"
            className="stage-strip-seg"
            data-live={stage.status === "executing" || undefined}
            title={`${stage.name} · ${STAGE_STATES[stage.status].label}`}
            aria-label={`${stage.name}, ${STAGE_STATES[stage.status].label}`}
            onClick={() => open(stage.id)}
          />
        </li>
      ))}
    </ol>
  );
}
