import { ArrowUpRight, X } from "lucide-react";
import { useEffect, useRef, type CSSProperties, type ReactNode, type RefObject } from "react";
import { Link } from "react-router";

import { Dialog, DialogClose, DialogContent, DialogTitle } from "@/components/ui/dialog";
import type { NodeRef, TraceFinding } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { docColour } from "@/plan/concept/marks";
import { featureSourceHref } from "@/plan/feature/source-href";
import { docOfPath } from "@/plan/trace/model";

import { LANE_TEXT } from "./findings-cards";
import { Actions, Reopen, type Context } from "./findings-controls";
import { SeverityMark, StateMark } from "./findings-marks";
import { cardState, findingSource, laneOf, nodeLabel, type CardState } from "./findings-model";

import "./findings-sheet.css";

// One finding in a dialog over the board (a bottom sheet under 760px): its head names the lane,
// state and severity; the body holds the quote it rests on, its text and where it points; the
// foot says what happens next and holds the actions. The shared dialog primitive supplies the
// focus trap, Escape and the scrim; focus lands on the title and returns to the finding's card.

const NEXT: Record<CardState, string> = {
  open: "The next address run takes this finding unless you dismiss it.",
  addressing: "The address run is working on this finding.",
  unverified: "A review of the feature checks the quote, or drops the finding.",
  stale: "A review of the feature checks the quote against the passage again.",
  unaddressed: "The address run could not fix it. Reopen it and the next run tries again.",
  addressed: "A team fixed this in the documents; the Changed pills name where.",
  dismissed: "You set this aside. Reopen it to put it back in the next address run.",
  duplicate: "A repeat of an earlier finding, so it counts nowhere.",
};

/** The passages a finding points at (or changed), as links to their place in the documents. */
function Chips(props: Context & { nodes: NodeRef[]; label: string }) {
  const { trace, feature, nodes, label } = props;
  return (
    <span className="findings-chips" role="group" aria-label={label}>
      {nodes.map((node) => (
        <span key={`${node.path}#${node.anchor ?? ""}`}>
          <Link
            className="findings-chip"
            to={featureSourceHref(feature, node.path, { anchor: node.anchor })}
          >
            <Untrusted text={nodeLabel(trace, node, feature)} />
          </Link>{" "}
        </span>
      ))}
    </span>
  );
}

/**
 * The quote the finding rests on, under an edge in its document's hue. A stale quote is struck
 * through under a dashed edge; a finding with no quote yet says so.
 */
function Evidence(props: Context & { entry: TraceFinding }) {
  const { entry, trace, feature } = props;
  const quote = entry.finding.evidence;
  const first = entry.about[0];
  const place = first ? nodeLabel(trace, first, feature) : null;
  const hue = docColour(first ? (docOfPath(trace, first.path)?.number ?? 0) : 0);
  if (quote === null) {
    return (
      <figure className="findings-evidence" data-missing="true">
        <figcaption>No evidence yet</figcaption>
        <blockquote>
          No quote has been checked against the documents. A review of the feature finds one or
          drops the finding.
        </blockquote>
      </figure>
    );
  }
  const stale = entry.check.kind === "stale";
  return (
    <figure
      className="findings-evidence"
      data-stale={stale ? "true" : undefined}
      style={{ "--evidence-hue": hue } as CSSProperties}
    >
      <figcaption>
        {stale ? "The quote is no longer in " : "Quoted from "}
        {place === null ? "its passage" : <Untrusted text={place} />}
      </figcaption>
      <blockquote>
        <Untrusted text={quote} />
      </blockquote>
    </figure>
  );
}

function Meta(props: Context & { entry: TraceFinding; state: CardState }) {
  const { entry, state, ...rest } = props;
  const reason =
    state === "dismissed" || state === "unaddressed" ? entry.finding.status.detail : null;
  return (
    <dl className="findings-meta">
      {entry.about.length > 0 && (
        <>
          <dt>About</dt>
          <dd>
            <Chips {...rest} nodes={entry.about} label="About" />
          </dd>
        </>
      )}
      {entry.changed.length > 0 && (
        <>
          <dt>Changed</dt>
          <dd>
            <Chips {...rest} nodes={entry.changed} label="Changed" />
          </dd>
        </>
      )}
      <dt>Found by</dt>
      <dd>
        <Untrusted text={findingSource(entry.finding.source, rest.feature).words} />
      </dd>
      {reason !== null && (
        <>
          <dt>Reason</dt>
          <dd>
            <Untrusted text={reason} />
          </dd>
        </>
      )}
    </dl>
  );
}

/** The actions each state offers; a quiet link opens the first passage where the mock draws one. */
function Foot(props: Context & { entry: TraceFinding; state: CardState; settled: () => void }) {
  const { entry, state, settled, ...rest } = props;
  const waits = state === "open" || state === "addressing";
  const first = entry.about[0];
  const passage = (waits || state === "addressed" || state === "duplicate") && first && (
    <Link
      className="plan-button"
      data-look="quiet"
      to={featureSourceHref(rest.feature, first.path, { anchor: first.anchor })}
    >
      <ArrowUpRight aria-hidden="true" />
      Open the passage
    </Link>
  );
  let controls: ReactNode = null;
  if (waits || state === "unverified" || state === "stale") {
    controls = <Actions {...rest} entry={entry} onDismissed={settled} />;
  } else if (state === "unaddressed" || state === "dismissed") {
    controls = <Reopen {...rest} entry={entry} onReopened={settled} />;
  }
  return (
    <footer className="findings-sheet__foot">
      <p className="plan-fine">{NEXT[state]}</p>
      <div className="findings-sheet__actions">
        {controls}
        {passage || null}
      </div>
    </footer>
  );
}

/** Focus the title when the dialog switches to another finding (a duplicate's link). */
function useRefocusOnSwitch(id: string, title: RefObject<HTMLHeadingElement | null>) {
  const shown = useRef(id);
  useEffect(() => {
    if (shown.current === id) return;
    shown.current = id;
    title.current?.focus();
  }, [id, title]);
}

function SheetBody(props: Context & { entry: TraceFinding }) {
  const { entry, ...rest } = props;
  const { finding } = entry;
  const state = cardState(entry, rest.addressing);
  const title = useRef<HTMLHeadingElement>(null);
  useRefocusOnSwitch(finding.id, title);
  const original = entry.check.duplicate_of;
  return (
    <DialogContent
      showCloseButton={false}
      data-loom-plan=""
      className="findings-sheet"
      aria-describedby={undefined}
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        title.current?.focus();
      }}
      onCloseAutoFocus={(event) => {
        // The card is looked up now: a status change may have moved it to another lane.
        event.preventDefault();
        document.getElementById(`finding-${finding.id}`)?.querySelector("button")?.focus();
      }}
    >
      <header className="findings-sheet__head">
        <span className="findings-id">
          <Untrusted text={finding.id} />
        </span>
        <span className="findings-sheet__lane">in {LANE_TEXT[laneOf(state)].name}</span>
        <StateMark entry={entry} state={state}>
          {state === "duplicate" && original !== null ? (
            <>
              Duplicate of <Link to={{ hash: `#finding-${original}` }}>{original}</Link>
            </>
          ) : undefined}
        </StateMark>
        <SeverityMark severity={finding.severity} />
        <DialogClose className="plan-button" data-look="quiet" aria-label="Close">
          <X aria-hidden="true" />
        </DialogClose>
      </header>
      <div className="findings-sheet__body">
        <DialogTitle ref={title} tabIndex={-1} className="findings-sheet__title">
          <Untrusted text={finding.title} />
        </DialogTitle>
        <Evidence {...rest} entry={entry} />
        <div className="findings-text">
          {finding.text.split(/\n\s*\n/).map((paragraph, index) => (
            <p key={index}>
              <Untrusted text={paragraph} />
            </p>
          ))}
        </div>
        <Meta {...rest} entry={entry} state={state} />
      </div>
      <Foot {...rest} entry={entry} state={state} settled={() => title.current?.focus()} />
    </DialogContent>
  );
}

/** The open finding's dialog; `entry` undefined (nothing chosen, or gone from the ledger) is closed. */
export function FindingSheet(
  props: Context & { entry: TraceFinding | undefined; onClose: () => void },
) {
  const { entry, onClose, ...rest } = props;
  return (
    <Dialog open={entry !== undefined} onOpenChange={(open) => !open && onClose()}>
      {entry && <SheetBody {...rest} entry={entry} />}
    </Dialog>
  );
}
