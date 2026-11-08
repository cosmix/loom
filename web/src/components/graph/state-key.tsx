import { cn } from "cn";

import type { StageSummary } from "@/api/schema";
import type { Focus } from "@/components/graph/context";
import { StateGlyph, toneClass } from "@/components/state-badge";
import { displayState, type DisplayState } from "@/lib/display-state";
import { stateMeta } from "@/lib/format";
import { contractTesting, isContractPhase } from "@/lib/format-activity";
import { DISPLAY_LEGEND } from "@/lib/states";

/// One chip per state present in the plan, with its count. Hovering a chip
/// lights every stage in that state; clicking pins it.
export function StateKey({
  stages,
  pinned,
  onHover,
  onPin,
}: {
  stages: readonly StageSummary[];
  pinned: Focus;
  onHover: (focus: Focus) => void;
  onPin: (status: DisplayState) => void;
}) {
  const counts = new Map<DisplayState, number>();
  for (const stage of stages) {
    const state = displayState(stage);
    counts.set(state, (counts.get(state) ?? 0) + 1);
  }
  const testingCount = stages.filter((stage) => contractTesting(stage) !== null).length;
  const contractCount = stages.filter(isContractPhase).length - testingCount;
  const present = DISPLAY_LEGEND.filter((entry) => counts.has(entry.status));
  return (
    <ul className="key" aria-label="stage states in this plan">
      {present.map(({ status, label }) => {
        const isPinned = pinned?.kind === "status" && pinned.status === status;
        return (
          <li key={status}>
            <button
              type="button"
              className={cn("key-chip", toneClass(stateMeta(status).tone))}
              data-pinned={isPinned || undefined}
              aria-pressed={isPinned}
              onMouseEnter={() => onHover({ kind: "status", status })}
              onMouseLeave={() => onHover(null)}
              onFocus={() => onHover({ kind: "status", status })}
              onBlur={() => onHover(null)}
              onClick={() => onPin(status)}
            >
              <StateGlyph status={status} className="text-[11px]" />
              <span>{label}</span>
              <span className="key-count">{counts.get(status)}</span>
            </button>
          </li>
        );
      })}
      {contractCount > 0 && <ContractChip label="writing contracts" count={contractCount} />}
      {testingCount > 0 && <ContractChip label="testing contracts" count={testingCount} />}
    </ul>
  );
}

function ContractChip({ label, count }: { label: string; count: number }) {
  return (
    <li>
      <span className={cn("key-chip", toneClass("contract"))}>
        <StateGlyph status="executing" className="text-[11px]" />
        <span>{label}</span>
        <span className="key-count">{count}</span>
      </span>
    </li>
  );
}
