import { useAtomValue } from "jotai";
import { ChevronRight, Plus } from "lucide-react";
import { useId, useState, type SyntheticEvent } from "react";
import { useNavigate } from "react-router";

import type { StandalonePlanView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { MoveIntoFeatureDialog } from "@/plan/home/move-into-feature";
import { STATE_TONE, stateCounts } from "@/plan/home/standalone-order";
import { StandaloneTable } from "@/plan/home/standalone-table";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  movesAtom,
  standaloneAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { PLAN_STATE_WORD_GLYPHS } from "@/plan/ui/glyphs";
import { plural } from "@/plan/ui/plural";

import "./standalone-shelf.css";

/** Misplaced documents (visions, sections, metaplans): each keeps its own one-path move. */
function MoveRows({ onMove, reason }: { onMove: (path: string) => void; reason: string | null }) {
  const moves = useAtomValue(movesAtom);
  if (moves.length === 0) return null;
  return (
    <ul className="plan-list">
      {moves.map((offer) => {
        const path = offer.target;
        if (path === null) return null;
        return (
          <li key={offer.id} className="shelf__move">
            <span>
              Move <Untrusted text={path} /> into a feature
            </span>
            <DisabledReason reason={reason} onClick={() => onMove(path)} size="sm">
              Move plan
            </DisabledReason>
          </li>
        );
      })}
    </ul>
  );
}

/** The one action in the summary row; its click must not open or close the shelf. */
function NewPlanAction({ reason }: { reason: string | null }) {
  const navigate = useNavigate();
  const keep = (event: SyntheticEvent) => {
    event.preventDefault();
    event.stopPropagation();
  };
  return (
    <div role="presentation" className="shelf__actions" onClick={keep}>
      <DisabledReason
        reason={reason}
        size="sm"
        onClick={() => navigate("/new", { state: { small: true } })}
      >
        <Plus aria-hidden="true" />
        New standalone plan
      </DisabledReason>
    </div>
  );
}

/** "<n> selected · Move to a feature… · Clear", or, with nothing selected, what selecting does. */
function SelectionBar({
  count,
  reason,
  onMove,
  onClear,
}: {
  count: number;
  reason: string | null;
  onMove: () => void;
  onClear: () => void;
}) {
  if (count === 0) {
    return <p className="shelf-bar__hint">Select plans to move them into a feature together.</p>;
  }
  return (
    <div role="group" aria-label="Selected plans" className="shelf-bar">
      <span className="shelf-bar__count" aria-live="polite">{`${count} selected`}</span>
      <span aria-hidden="true" className="shelf-bar__sep">
        ·
      </span>
      <DisabledReason reason={reason} size="sm" look="quiet" onClick={onMove}>
        Move to a feature…
      </DisabledReason>
      <span aria-hidden="true" className="shelf-bar__sep">
        ·
      </span>
      <DisabledReason reason={null} size="sm" look="quiet" onClick={onClear}>
        Clear
      </DisabledReason>
    </div>
  );
}

/** The selected paths that still name a listed plan, in the list's order. */
function useSelection() {
  const plans = useAtomValue(standaloneAtom);
  const [picked, setPicked] = useState<ReadonlySet<string>>(() => new Set());
  const selected = new Set(plans.filter((plan) => picked.has(plan.path)).map((plan) => plan.path));
  const toggle = (path: string) =>
    setPicked((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  const select = (paths: string[], on: boolean) =>
    setPicked((current) => {
      const next = new Set(current);
      for (const path of paths) {
        if (on) next.add(path);
        else next.delete(path);
      }
      return next;
    });
  return { plans, selected, toggle, select, clear: () => setPicked(new Set()) };
}

/** How many plans are in each state, running first; a state with none is left out. */
function ShelfStates({ plans }: { plans: StandalonePlanView[] }) {
  const counts = stateCounts(plans);
  if (counts.length === 0) return null;
  return (
    <span className="shelf__states">
      {counts.map(({ state, count }) => {
        const Glyph = PLAN_STATE_WORD_GLYPHS[state];
        return (
          <span key={state} data-state={state} className="plan-status">
            <Glyph aria-hidden="true" className={STATE_TONE[state]} />
            {`${count} ${state}`}
          </span>
        );
      })}
    </span>
  );
}

function ShelfSummary({ plans, reason }: { plans: StandalonePlanView[]; reason: string | null }) {
  return (
    <summary className="shelf__summary">
      <ChevronRight aria-hidden="true" className="shelf__chevron" />
      <span className="shelf__head">
        <span className="shelf__count">{plural(plans.length, "standalone plan")}</span>
        <span className="shelf__hint">bugfixes, performance work and small features</span>
      </span>
      <NewPlanAction reason={reason} />
      <ShelfStates plans={plans} />
    </summary>
  );
}

/** The explanation an empty shelf opens with, unless misplaced documents already fill it. */
function ShelfEmpty() {
  const moves = useAtomValue(movesAtom);
  if (moves.length > 0) return null;
  return (
    <p className="plan-empty shelf__empty">
      Bugfixes and small changes that need no product spec live here.
    </p>
  );
}

/**
 * Collapsed by default; an empty shelf opens so its explanation shows. `defaultOpen` lets a host
 * (the help scene) show the table without a click.
 */
export function StandaloneShelf({ defaultOpen = false }: { defaultOpen?: boolean }) {
  const headingId = useId();
  const { plans, selected, toggle, select, clear } = useSelection();
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const [moving, setMoving] = useState<string[] | null>(null);
  // The server's reason is each control's own description; no second line repeats it.
  const reason = canAuthor ? null : (blockReason ?? "Authoring is unavailable");
  return (
    <section aria-labelledby={headingId} className="shelf">
      {/* The summary row carries the visible count; this names the region. */}
      <h2 id={headingId} className="sr-only">
        Standalone plans
      </h2>
      <MoveRows reason={reason} onMove={(path) => setMoving([path])} />
      <details className="shelf__details" open={defaultOpen || plans.length === 0}>
        <ShelfSummary plans={plans} reason={reason} />
        {plans.length === 0 && <ShelfEmpty />}
        {plans.length > 0 && (
          <StandaloneTable
            plans={plans}
            selected={selected}
            onToggle={toggle}
            onSelect={select}
            actions={
              <SelectionBar
                count={selected.size}
                reason={reason}
                onMove={() => setMoving([...selected])}
                onClear={clear}
              />
            }
          />
        )}
      </details>
      <MoveIntoFeatureDialog
        paths={moving ?? []}
        open={moving !== null}
        onOpenChange={(open) => {
          if (!open) setMoving(null);
        }}
      />
    </section>
  );
}
