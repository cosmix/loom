import { LockIcon } from "lucide-react";
import { useRef } from "react";

import type { PlanStats } from "@/api/schema";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogPortal,
  DialogTitle,
} from "@/components/ui/dialog";

import { ModelsTable } from "./models-table";
import { PlanFooter } from "./plan-footer";
import { countsText, stageCounts } from "./plan-math";
import { PlanTimeline } from "./plan-timeline";
import { PlanKpis, TokenMix } from "./plan-totals";
import "./plan-overview.css";
import "./plan-overview-data.css";

const FRAME =
  "plan-overview max-h-[calc(100dvh-2rem)] grid-rows-[auto_minmax(0,1fr)_auto] gap-0 overflow-hidden p-0 sm:max-w-[980px]";

/// The locked end-of-plan overview. It has no way out: no close control, and Escape and outside
/// events are swallowed. It goes away only when the plan record does (`loom clean --state`, `loom init`).
///
/// The dialog is non-modal to Radix on purpose: a modal Radix dialog hides the page through
/// aria-hidden's `hideOthers`, which keeps every `[aria-live]` element and its ancestors exposed
/// (the header's busy roundel has one), so the page stayed reachable behind it. Instead the
/// shell makes the page `inert` while this dialog is mounted, and this component draws the scrim
/// and pulls focus back when anything else takes it.
export function PlanOverviewDialog({ record }: { record: PlanStats }) {
  const content = useRef<HTMLDivElement>(null);
  const noneDone = stageCounts(record).completed === 0;
  return (
    <Dialog open modal={false}>
      <DialogPortal>
        <div
          data-slot="dialog-overlay"
          className="fixed inset-0 isolate z-50 bg-(--dialog-backdrop) supports-backdrop-filter:backdrop-blur-(--dialog-backdrop-blur)"
        />
      </DialogPortal>
      <DialogContent
        ref={content}
        role="alertdialog"
        aria-modal="true"
        showCloseButton={false}
        className={FRAME}
        onEscapeKeyDown={(event) => event.preventDefault()}
        onInteractOutside={(event) => event.preventDefault()}
        onFocusOutside={(event) => {
          event.preventDefault();
          content.current?.focus();
        }}
      >
        <header className="po-head">
          <div className="po-head-main">
            <div className="eyebrow">Plan complete</div>
            <DialogTitle className="po-title">{record.plan_name}</DialogTitle>
            <DialogDescription className="po-lock">
              <LockIcon aria-hidden="true" />
              <span>
                This view stays open. Start the next plan with <code>loom init</code>, or clear this
                one with <code>loom clean --state</code>.
              </span>
            </DialogDescription>
          </div>
          <span className="po-pill" data-tone={noneDone ? "dimmed" : undefined}>
            {countsText(record)}
          </span>
        </header>
        <div className="po-body" tabIndex={0} role="region" aria-label="Plan statistics">
          <PlanKpis record={record} />
          <TokenMix record={record} />
          <PlanTimeline record={record} />
          <ModelsTable record={record} />
        </div>
        <PlanFooter record={record} />
      </DialogContent>
    </Dialog>
  );
}
