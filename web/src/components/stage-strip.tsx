import { cn } from "cn";
import { useAtomValue } from "jotai/react";

import type { StageSummary } from "@/api/schema";
import { useOpenStage } from "@/components/stage-modal";
import { toneClass } from "@/components/state-badge";
import { isContractPhase, stateMeta, type Tone } from "@/lib/format";
import { STAGE_STATES } from "@/lib/states";
import { orderedStagesAtom } from "@/state/atoms";

/// A contract-phase segment gets its own tone and a suffixed label, so the
/// phase reads even though the wire status stays "executing".
function segmentMeta(stage: StageSummary): { tone: Tone; label: string } {
  const label = STAGE_STATES[stage.status].label;
  return isContractPhase(stage)
    ? { tone: "contract", label: `${label} · contract phase` }
    : { tone: stateMeta(stage.status).tone, label };
}

/// The plan as a warp: one segment per stage in dependency order, coloured
/// by state. A segment opens its stage.
export function StageStrip({ className }: { className?: string }) {
  const ordered = useAtomValue(orderedStagesAtom);
  const open = useOpenStage();
  if (ordered.length === 0) return null;
  return (
    <ol className={cn("stage-strip", className)} aria-label="stages in dependency order">
      {ordered.map(({ stage }) => {
        const { tone, label } = segmentMeta(stage);
        return (
          <li key={stage.id} className={toneClass(tone)}>
            <button
              type="button"
              className="stage-strip-seg"
              data-live={stage.status === "executing" || undefined}
              title={`${stage.name} · ${label}`}
              aria-label={`${stage.name}, ${label}`}
              onClick={() => open(stage.id)}
            />
          </li>
        );
      })}
    </ol>
  );
}
