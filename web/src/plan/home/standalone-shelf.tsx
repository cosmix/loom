import { useAtomValue } from "jotai";
import { ChevronRight, FolderInput, Plus, Wrench } from "lucide-react";
import { useId, useState, type SyntheticEvent } from "react";
import { Link, useNavigate } from "react-router";

import type { StandalonePlanView } from "@/plan/api/schema";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { plural } from "@/plan/feature/steps";
import { toAddress } from "@/plan/files/address";
import { MoveIntoFeatureDialog } from "@/plan/home/move-into-feature";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  movesAtom,
  standaloneAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import "./standalone-shelf.css";

const CHIPS_SHOWN = 24;

function PlanChip({ plan }: { plan: StandalonePlanView }) {
  return (
    <li>
      <Link to={toAddress(plan.path)} className="shelf__chip">
        {/* The chip shows the slug; the title and state are there for screen readers. */}
        <span className="sr-only" style={{ unicodeBidi: "isolate", direction: "ltr" }}>
          <SentenceText text={visibleControls(plan.title)} />
        </span>{" "}
        <span className="shelf__slug">
          <Untrusted text={plan.slug} />
        </span>{" "}
        <span className="shelf__dot" data-state={plan.state} aria-hidden="true" />
        <span className="sr-only">
          <Untrusted text={plan.state} />
        </span>
      </Link>
    </li>
  );
}

function PlanChips({ plans }: { plans: StandalonePlanView[] }) {
  const [all, setAll] = useState(false);
  const shown = all ? plans : plans.slice(0, CHIPS_SHOWN);
  const hidden = plans.length - shown.length;

  return (
    <ul className="shelf__chips">
      {shown.map((plan) => (
        <PlanChip key={plan.path} plan={plan} />
      ))}
      {hidden > 0 && (
        <li>
          <button type="button" className="shelf__more" onClick={() => setAll(true)}>
            {`+${hidden} more`}
          </button>
        </li>
      )}
    </ul>
  );
}

function MoveRows({ onMove, reason }: { onMove: (path: string) => void; reason: string | null }) {
  const moves = useAtomValue(movesAtom);
  if (moves.length === 0) return null;
  return (
    <ul className="shelf__moves">
      {moves.map((offer) => {
        const path = offer.target;
        if (path === null) return null;
        return (
          <li key={offer.id} className="shelf__move">
            <span>
              Move <Untrusted text={path} /> into a feature
            </span>
            <DisabledReason reason={reason} onClick={() => onMove(path)} size="sm">
              Move into a feature…
            </DisabledReason>
          </li>
        );
      })}
    </ul>
  );
}

/** The actions live in the summary row; their clicks must not open or close the shelf. */
function ShelfActions({ reason, onMove }: { reason: string | null; onMove: () => void }) {
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
      <DisabledReason reason={reason} size="sm" onClick={onMove}>
        <FolderInput aria-hidden="true" />
        Move into a feature…
      </DisabledReason>
    </div>
  );
}

/**
 * Collapsed by default; an empty shelf opens so its explanation shows. `defaultOpen` lets a host
 * (the help scene) show the chips without a click.
 */
export function StandaloneShelf({ defaultOpen = false }: { defaultOpen?: boolean }) {
  const headingId = useId();
  const plans = useAtomValue(standaloneAtom);
  const moves = useAtomValue(movesAtom);
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const [dialog, setDialog] = useState<{ path: string | null } | null>(null);
  const reason = canAuthor ? null : "Authoring is unavailable";
  return (
    <section aria-labelledby={headingId} className="shelf">
      {/* The summary row carries the visible count; this names the region. */}
      <h2 id={headingId} className="sr-only">
        Standalone plans
      </h2>
      <MoveRows reason={reason} onMove={(path) => setDialog({ path })} />
      <details className="shelf__details" open={defaultOpen || plans.length === 0}>
        <summary className="shelf__summary">
          <ChevronRight aria-hidden="true" className="shelf__chevron" />
          <Wrench aria-hidden="true" className="shelf__wrench" />
          <span>{plural(plans.length, "standalone plan")}</span>
          <span className="shelf__hint">
            · bugfixes, performance work and small features in doc/plans/
          </span>
          <ShelfActions reason={reason} onMove={() => setDialog({ path: null })} />
        </summary>
        {plans.length === 0 && moves.length === 0 && (
          <p className="shelf__empty">
            Bugfixes and small changes that need no product spec live here.
          </p>
        )}
        {plans.length > 0 && <PlanChips plans={plans} />}
      </details>
      {!canAuthor && blockReason && (
        <p className="shelf__reason">
          <Untrusted text={blockReason} />
        </p>
      )}
      <MoveIntoFeatureDialog
        path={dialog?.path ?? null}
        open={dialog !== null}
        onOpenChange={(open) => {
          if (!open) setDialog(null);
        }}
      />
    </section>
  );
}
