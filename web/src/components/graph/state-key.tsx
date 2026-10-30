import { cn } from "cn";

import type { StageStatus, StageSummary } from "@/api/schema";
import type { Focus } from "@/components/graph/context";
import { StateGlyph, toneClass } from "@/components/state-badge";
import { stateMeta } from "@/lib/format";
import { contractTesting, isContractPhase } from "@/lib/format-activity";
import { LEGEND } from "@/lib/states";

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
  onPin: (status: StageStatus) => void;
}) {
  const counts = new Map<StageStatus, number>();
  for (const stage of stages) {
    counts.set(stage.status, (counts.get(stage.status) ?? 0) + 1);
  }
  const testingCount = stages.filter((stage) => contractTesting(stage) !== null).length;
  const contractCount = stages.filter(isContractPhase).length - testingCount;
  const present = LEGEND.filter((entry) => counts.has(entry.status));
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
