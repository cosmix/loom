import { useEffect, useMemo, useState, type ReactNode } from "react";

import type { StandalonePlanView } from "@/plan/api/schema";
import {
  activeStates,
  STATE_TONE,
  stateCounts,
  toggleState,
  type PlanState,
  type StateCount,
} from "@/plan/home/standalone-order";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { PLAN_STATE_WORD_GLYPHS } from "@/plan/ui/glyphs";

import "./state-filter.css";

const STATE_LABELS: Record<PlanState, string> = {
  running: "Running",
  written: "Written",
  done: "Done",
  retired: "Retired",
};

function Toggle({
  pressed,
  count,
  onClick,
  children,
}: {
  pressed: boolean;
  count: number;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button type="button" aria-pressed={pressed} className="state-filter__toggle" onClick={onClick}>
      {children} <span className="state-filter__count">{count}</span>
    </button>
  );
}

/**
 * "All", then one toggle per state the list holds, each with its count. Toggles combine: the
 * table shows the plans in any pressed state, and with none pressed, All is pressed and every
 * plan shows. Nothing renders while the list holds fewer than two states.
 */
export function StateFilter({
  counts,
  total,
  picked,
  onToggle,
  onAll,
}: {
  counts: readonly StateCount[];
  total: number;
  picked: ReadonlySet<PlanState>;
  onToggle: (state: PlanState) => void;
  onAll: () => void;
}) {
  if (counts.length < 2) return null;
  return (
    <div role="group" aria-label="Filter by state" className="state-filter">
      <Toggle pressed={picked.size === 0} count={total} onClick={onAll}>
        All
      </Toggle>
      {counts.map(({ state, count }) => {
        const Glyph = PLAN_STATE_WORD_GLYPHS[state];
        return (
          <Toggle
            key={state}
            pressed={picked.has(state)}
            count={count}
            onClick={() => onToggle(state)}
          >
            <Glyph aria-hidden="true" className={STATE_TONE[state]} />
            {STATE_LABELS[state]}
          </Toggle>
        );
      })}
    </div>
  );
}

const EVERY_STATE: ReadonlySet<PlanState> = new Set();

/**
 * The table's state filter: the counts of the whole list, the states in force (none, meaning
 * every state, while the list holds fewer than two), and the toggles. A selected plan the filter
 * hides is unselected, when a toggle hides it or when the list changes under the filter, so the
 * selection bar never acts on a row the reader cannot see.
 */
export function useStateFilter(
  plans: readonly StandalonePlanView[],
  selected: ReadonlySet<string>,
  onSelect: (paths: string[], on: boolean) => void,
) {
  const [picked, setPicked] = useState<ReadonlySet<PlanState>>(EVERY_STATE);
  const counts = useMemo(() => stateCounts(plans), [plans]);
  const states = useMemo(
    () => (counts.length < 2 ? EVERY_STATE : activeStates(picked, counts)),
    [picked, counts],
  );
  const hidden = plans
    .filter((plan) => states.size > 0 && !states.has(plan.state) && selected.has(plan.path))
    .map((plan) => plan.path)
    .join("\n");
  useEffect(() => {
    if (hidden !== "") onSelect(hidden.split("\n"), false);
  }, [hidden, onSelect]);
  return {
    counts,
    states,
    toggle: (state: PlanState) => setPicked(toggleState(states, state)),
    all: () => setPicked(EVERY_STATE),
  };
}

/** The line a state filter with no matching plan leaves, and the way back to every plan. */
export function StateFilterEmpty({ onAll }: { onAll: () => void }) {
  return (
    <div className="state-filter__empty">
      <p className="plan-empty">No plans in this state.</p>
      <DisabledReason reason={null} size="sm" onClick={onAll}>
        Show all states
      </DisabledReason>
    </div>
  );
}
