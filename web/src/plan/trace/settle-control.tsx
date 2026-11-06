import { useAtomValue } from "jotai/react";
import { useId, useRef, type JSX } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from "@/components/ui/dialog";
import type { TraceDoc } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { plural } from "@/plan/ui/plural";

import { docOfPath, settleLabel } from "./model";
import { useSettle, type SettlePhase } from "./settle-flow";
import { ChangesSinceSettled } from "./settled-changes";
import { useTrace } from "./state";

import "./trace.css";

/** A document locks when its plan starts running or is done or retired, or it sits in the archive. */
const LOCKED_NAME = /^(?:IN_PROGRESS|DONE|RETIRED)-/;

function isLocked(path: string): boolean {
  const parts = path.split("/");
  return LOCKED_NAME.test(parts.at(-1) ?? "") || parts.slice(0, -1).includes("archive");
}

/** "1 My call and 2 open questions": only the kinds the document holds. */
function pointsText(myCalls: number, openQuestions: number): string {
  const points = [
    ...(myCalls > 0 ? [plural(myCalls, "My call")] : []),
    ...(openQuestions > 0 ? [plural(openQuestions, "open question")] : []),
  ];
  return points.join(" and ");
}

function ConfirmDialog(props: {
  phase: Extract<SettlePhase, { kind: "confirm" }>;
  onConfirm: () => void;
  onCancel: () => void;
  returnFocus: () => void;
}): JSX.Element {
  const { phase, onConfirm, onCancel, returnFocus } = props;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <DialogContent
        showCloseButton={false}
        onCloseAutoFocus={(event) => {
          // Nothing opened the dialog by a trigger, so the default would drop focus on the body.
          event.preventDefault();
          returnFocus();
        }}
      >
        <DialogTitle>Settle with open points?</DialogTitle>
        <DialogDescription>
          This document holds {pointsText(phase.myCalls, phase.openQuestions)}. Settling records it
          as read with those points still open.
        </DialogDescription>
        <DialogFooter>
          <Button variant="outline" onClick={onCancel}>
            Cancel
          </Button>
          <Button onClick={onConfirm}>Settle anyway</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

type Offer = { settle: boolean; unsettle: boolean; changes: boolean };

/**
 * Which controls a state offers: settling anything not settled (a moved document is settled
 * again), clearing anything that carries a line, and the comparison only when the body moved.
 */
function offerFor(doc: TraceDoc): Offer {
  return {
    settle: doc.settle !== "settled",
    unsettle: doc.settle !== "draft",
    changes: doc.settle === "revised",
  };
}

function SettleBody({
  feature,
  doc,
  hash,
}: {
  feature: string;
  doc: TraceDoc;
  hash: string | null;
}): JSX.Element {
  const flow = useSettle(feature, doc.path, hash);
  const blocked = useAtomValue(authorBlockReasonAtom);
  const reasonId = useId();
  const state = useRef<HTMLSpanElement>(null);
  const working = flow.phase.kind === "working";
  const reason = working ? "Working" : blocked;
  // Locked documents, and a document whose hash has not loaded, show the state alone.
  const shown = !isLocked(doc.path) && hash !== null ? offerFor(doc) : null;
  const buttons = shown !== null && (shown.settle || shown.unsettle);
  const focusState = () => state.current?.focus();
  // A written line removes the button that was pressed; focus moves to the state it now reads.
  const run = (go: () => Promise<boolean>) =>
    void go().then((written) => {
      if (written) focusState();
    });
  const button = {
    reason,
    reasonId,
    look: "quiet",
    size: "sm",
    reasonPlacement: "inline",
  } as const;
  return (
    <div className="trace-settle-control">
      <span ref={state} tabIndex={-1} className="trace-settle-state" data-settle={doc.settle}>
        {settleLabel(doc.settle)}
      </span>
      {shown?.settle && (
        <DisabledReason {...button} onClick={() => run(flow.settle)}>
          Mark settled
        </DisabledReason>
      )}
      {shown?.unsettle && (
        <DisabledReason {...button} onClick={() => run(flow.unsettle)}>
          Unsettle
        </DisabledReason>
      )}
      {buttons && reason !== null && (
        <span id={reasonId} className="plan-fine text-muted-foreground">
          <Untrusted text={reason} />
        </span>
      )}
      {shown?.changes && <ChangesSinceSettled path={doc.path} />}
      {flow.phase.kind === "notice" && (
        <span role="alert" className="trace-settle-note">
          <Untrusted text={flow.phase.message} />
        </span>
      )}
      {flow.phase.kind === "confirm" && (
        <ConfirmDialog
          phase={flow.phase}
          onConfirm={() => run(flow.confirm)}
          onCancel={flow.dismiss}
          returnFocus={focusState}
        />
      )}
    </div>
  );
}

/**
 * A document's settle state with the controls that change it. `path` is the repository path and
 * `hash` the opened document's save hash. Renders nothing for a path the trace does not know
 * (a brief, a file outside the feature's documents).
 */
export function SettleControl(props: {
  feature: string;
  path: string;
  hash: string | null;
}): JSX.Element | null {
  const { trace } = useTrace(props.feature);
  const doc = trace ? docOfPath(trace, props.path) : undefined;
  if (!doc) return null;
  // Keyed by document: a phase or lookup in flight for one must not show under the next.
  return <SettleBody key={doc.path} feature={props.feature} doc={doc} hash={props.hash} />;
}
