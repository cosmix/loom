import { useAtomValue } from "jotai";
import { useId, useState } from "react";
import { Link, useNavigate } from "react-router";

import type { StandalonePlanView } from "@/plan/api/schema";
import type { PlanStateView } from "@/plan/api/schema-files";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";
import { MoveIntoFeatureDialog } from "@/plan/home/move-into-feature";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  movesAtom,
  standaloneAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

const STATE_TONES: Record<PlanStateView, string> = {
  written: "text-muted-foreground",
  running: "text-(--tone-executing)",
  done: "text-(--tone-completed)",
  retired: "text-(--tone-dimmed)",
};

function PlanRow({ plan }: { plan: StandalonePlanView }) {
  return (
    <li className="border-b border-border last:border-0">
      <Link
        to={toAddress(plan.path)}
        className="flex flex-wrap items-center justify-between gap-2 rounded-sm py-3 focus-visible:outline-2 focus-visible:outline-ring"
      >
        <span className="min-w-0">
          <span className="block font-medium" style={{ unicodeBidi: "isolate", direction: "ltr" }}>
            <SentenceText text={visibleControls(plan.title)} />
          </span>{" "}
          <span className="block text-xs text-muted-foreground">
            <Untrusted text={plan.slug} />
          </span>
        </span>{" "}
        <span className={`text-xs ${STATE_TONES[plan.state]}`}>
          <Untrusted text={plan.state} />
        </span>
      </Link>
    </li>
  );
}

function MoveRows({ onMove, reason }: { onMove: (path: string) => void; reason: string | null }) {
  const moves = useAtomValue(movesAtom);
  return (
    <ul className="space-y-3">
      {moves.map((offer) => {
        const path = offer.target;
        if (path === null) return null;
        return (
          <li key={offer.id} className="flex flex-wrap items-center justify-between gap-2">
            <span className="break-words text-sm">
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

function ShelfActions({ reason, onMove }: { reason: string | null; onMove: () => void }) {
  const navigate = useNavigate();
  return (
    <div className="flex flex-wrap gap-3">
      <DisabledReason reason={reason} onClick={() => navigate("/new", { state: { small: true } })}>
        New standalone plan
      </DisabledReason>
      <DisabledReason reason={reason} onClick={onMove}>
        Move into a feature…
      </DisabledReason>
    </div>
  );
}

export function StandaloneShelf() {
  const plans = useAtomValue(standaloneAtom);
  const moves = useAtomValue(movesAtom);
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const headingId = useId();
  const [dialog, setDialog] = useState<{ path: string | null } | null>(null);
  const reason = canAuthor ? null : "Authoring is unavailable";
  return (
    <section aria-labelledby={headingId} className="space-y-4 border-t border-border pt-6">
      <h2 id={headingId} className="font-heading text-lg font-medium">
        Standalone plans
      </h2>
      {plans.length === 0 && moves.length === 0 && (
        <p className="text-sm text-muted-foreground">
          Bugfixes and small changes that need no product spec live here.
        </p>
      )}
      {plans.length > 0 && (
        <ul>
          {plans.map((plan) => (
            <PlanRow key={plan.path} plan={plan} />
          ))}
        </ul>
      )}
      <MoveRows reason={reason} onMove={(path) => setDialog({ path })} />
      <ShelfActions reason={reason} onMove={() => setDialog({ path: null })} />
      {!canAuthor && blockReason && (
        <p className="text-xs text-muted-foreground">
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
