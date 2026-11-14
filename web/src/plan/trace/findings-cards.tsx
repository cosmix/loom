import { useAtomValue, useStore } from "jotai/react";
import { RotateCcw, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link, useLocation } from "react-router";

import type { NodeRef, TraceFinding, TraceView } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { hashAnchor } from "@/plan/files/address";
import { featureSourceHref } from "@/plan/feature/source-href";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import {
  dismissReason,
  failureOf,
  REASON_MAX,
  setFindingStatus,
  type StatusChange,
} from "./findings-actions";
import {
  cardState,
  groupTitle,
  isFullCard,
  nodeLabel,
  stateText,
  type CardState,
  type FindingGroup,
} from "./findings-model";
import { ledgerBlock, type LedgerHash } from "./ledger-hash";
import { loadTrace } from "./state";

// The cards of the Findings page: one per finding. An open one shows what it is about, its quoted
// evidence, its text and Dismiss; anything else folds to one line, an addressed one with the
// headings it changed, an unaddressed or dismissed one with Reopen. Every ledger string is drawn
// as text (`Untrusted`).

/** What every finding card needs from the page. */
export interface Context {
  trace: TraceView;
  feature: string;
  /** The ledger hash that goes with `trace`; a change is sent only with it. */
  ledger: LedgerHash;
  /** Whether this feature's address run is live. */
  addressing: boolean;
}

const DISMISS_LABEL = "Dismiss";
const REOPEN_LABEL = "Reopen";
const ADDRESSING = "The address run is working on these findings";

function NodeChips(props: { trace: TraceView; feature: string; nodes: NodeRef[]; label: string }) {
  const { trace, feature, nodes, label } = props;
  if (nodes.length === 0) return null;
  return (
    <p className="maps-about" data-label={label} role="group" aria-label={label}>
      {nodes.map((node) => (
        <span key={`${node.path}#${node.anchor ?? ""}`}>
          <Link
            className="maps-chip"
            to={featureSourceHref(feature, node.path, { anchor: node.anchor })}
          >
            <Untrusted text={nodeLabel(trace, node, feature)} />
          </Link>{" "}
        </span>
      ))}
    </p>
  );
}

/** The quote the finding rests on, as the ledger holds it. */
function Evidence({ quote }: { quote: string | null }) {
  if (quote === null) return null;
  return (
    <figure className="maps-evidence">
      <figcaption className="plan-fine">Evidence</figcaption>
      <blockquote>
        <Untrusted text={quote} />
      </blockquote>
    </figure>
  );
}

/**
 * The reason field and its confirm. The field takes focus when the form opens; the confirm stays
 * blocked until the page holds the ledger's hash, the reason has text, and no change is in flight.
 */
function DismissForm(props: {
  busy: boolean;
  ledger: LedgerHash;
  onConfirm: (reason: string) => void;
  onCancel: () => void;
}) {
  const [text, setText] = useState("");
  const field = useRef<HTMLInputElement>(null);
  const reason = dismissReason(text);
  const authorBlock = useAtomValue(authorBlockReasonAtom);
  const blocked =
    authorBlock ??
    ledgerBlock(props.ledger) ??
    (reason === null ? "Write a reason first" : null) ??
    (props.busy ? "Saving the dismissal" : null);
  const confirm = () => {
    if (blocked === null && reason !== null) props.onConfirm(reason);
  };
  useEffect(() => field.current?.focus(), []);
  return (
    <form
      className="maps-dismiss"
      onSubmit={(event) => {
        event.preventDefault();
        confirm();
      }}
    >
      <label className="maps-reason">
        <span className="plan-fine">Reason for dismissing</span>
        <input
          ref={field}
          type="text"
          value={text}
          maxLength={REASON_MAX}
          onChange={(event) => setText(event.target.value)}
        />
      </label>
      <DisabledReason reason={blocked} look="quiet" onClick={confirm}>
        Confirm dismissal
      </DisabledReason>
      <DisabledReason reason={null} look="quiet" onClick={props.onCancel}>
        Cancel
      </DisabledReason>
    </form>
  );
}

/** Sets one finding's status; `set` resolves true when the ledger accepted the change. */
function useSetStatus(
  feature: string,
  id: string,
  ledger: LedgerHash,
  setFailure: (failure: string | null) => void,
) {
  const store = useStore();
  const [busy, setBusy] = useState(false);
  const set = async (status: StatusChange): Promise<boolean> => {
    if (ledger.hash === null) return false;
    setBusy(true);
    const result = await setFindingStatus(feature, id, ledger.hash, status);
    setBusy(false);
    const failed = failureOf(result);
    setFailure(failed);
    // A conflict means the ledger moved; the page shows what it holds now.
    void loadTrace(store, feature);
    return failed === null;
  };
  return { busy, set };
}

/**
 * Reopen for an unaddressed or dismissed finding: it reads open again, and the next Address the
 * findings run takes it while its evidence still verifies. The card turns into a full one, so
 * focus moves to its heading (`onReopened`).
 */
function Reopen(props: Context & { entry: TraceFinding; onReopened: () => void }) {
  const { entry, feature, ledger, addressing, onReopened } = props;
  const [failure, setFailure] = useState<string | null>(null);
  const authorBlock = useAtomValue(authorBlockReasonAtom);
  const { busy, set } = useSetStatus(feature, entry.finding.id, ledger, setFailure);
  const reason =
    authorBlock ??
    ledgerBlock(ledger) ??
    (addressing ? ADDRESSING : null) ??
    (busy ? "Reopening the finding" : null);
  const reopen = async () => {
    if (reason === null && (await set({ kind: "open", detail: null }))) onReopened();
  };
  return (
    <div className="maps-actions">
      <DisabledReason
        label={`${REOPEN_LABEL} ${entry.finding.id}`}
        reason={reason}
        look="quiet"
        onClick={() => void reopen()}
      >
        <RotateCcw aria-hidden="true" />
        {REOPEN_LABEL}
      </DisabledReason>
      {failure !== null && <ActionError message={failure} />}
    </div>
  );
}

/**
 * Dismiss for one open finding. Cancelling the form unmounts the control the keyboard was on, so
 * focus goes back to the Dismiss button once it is drawn again; a saved dismissal moves it to the
 * card's heading (`onDismissed`), since the card turns into a line.
 */
function Actions(props: Context & { entry: TraceFinding; onDismissed: () => void }) {
  const { entry, feature, ledger, addressing, onDismissed } = props;
  const [open, setOpen] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const authorBlock = useAtomValue(authorBlockReasonAtom);
  const opener = useRef<HTMLDivElement>(null);
  const refocus = useRef(false);
  useEffect(() => {
    if (open || !refocus.current) return;
    refocus.current = false;
    opener.current?.querySelector("button")?.focus();
  }, [open]);
  const cancel = () => {
    refocus.current = true;
    setOpen(false);
  };
  const dismissed = () => {
    setOpen(false);
    onDismissed();
  };
  const { busy, set } = useSetStatus(feature, entry.finding.id, ledger, setFailure);
  // A refused change keeps the reason in the form.
  const dismiss = async (reason: string) => {
    if (await set({ kind: "dismissed", detail: reason })) dismissed();
  };
  return (
    <div className="maps-actions" ref={opener}>
      {open ? (
        <DismissForm busy={busy} ledger={ledger} onConfirm={dismiss} onCancel={cancel} />
      ) : (
        <DisabledReason
          label={`${DISMISS_LABEL} ${entry.finding.id}`}
          reason={authorBlock ?? (addressing ? ADDRESSING : null)}
          look="secondary"
          onClick={() => setOpen(true)}
        >
          <X aria-hidden="true" />
          {DISMISS_LABEL}
        </DisabledReason>
      )}
      {failure !== null && <ActionError message={failure} />}
    </div>
  );
}

/**
 * The state in words. One span: the status line is a flex row, and the words must flow as one run
 * beside the glyph. A line cuts that span with an ellipsis, so it carries the whole state as its
 * `title`. A duplicate links to the finding it repeats.
 */
function StateWords({ entry, state }: { entry: TraceFinding; state: CardState }) {
  const words = stateText(entry, state);
  const original = entry.check.duplicate_of;
  if (state === "duplicate" && original !== null) {
    return (
      <span title={words}>
        Duplicate of <Link to={{ hash: `#finding-${original}` }}>{original}</Link>
      </span>
    );
  }
  return (
    <span title={isFullCard(state) ? undefined : words}>
      <Untrusted text={words} />
    </span>
  );
}

/** The parts of an open finding under its heading. */
function CardBody(props: Context & { entry: TraceFinding; onDismissed: () => void }) {
  const { entry, trace, feature } = props;
  return (
    <>
      <NodeChips trace={trace} feature={feature} nodes={entry.about} label="About" />
      <Evidence quote={entry.finding.evidence} />
      <p className="maps-finding-text">
        <Untrusted text={entry.finding.text} />
      </p>
      <Actions {...props} />
    </>
  );
}

/**
 * One finding. The heading element is the same as a card and as a line, so the focus a dismissal
 * leaves there survives the reload that turns the card into a line.
 */
function FindingCard(props: Context & { entry: TraceFinding }) {
  const { entry, trace, feature, addressing } = props;
  const { finding } = entry;
  const state = cardState(entry, addressing);
  const full = isFullCard(state);
  const { hash } = useLocation();
  const heading = useRef<HTMLHeadingElement>(null);
  // `data-addressed` washes the card the address names; `:target` does not follow router pushes.
  const addressed = hashAnchor(hash) === `finding-${finding.id}`;
  return (
    <li
      id={`finding-${finding.id}`}
      className="maps-finding"
      data-status={finding.status.kind}
      data-state={state}
      data-line={full ? undefined : "true"}
      data-addressed={addressed ? "true" : undefined}
    >
      <div className="maps-finding-head">
        <span className="plan-fine maps-finding-id">
          <Untrusted text={finding.id} />
        </span>
        <h3 className="maps-finding-title" ref={heading} tabIndex={-1}>
          <Untrusted text={finding.title} />
        </h3>
        <p className="plan-fine maps-finding-status">
          <StateWords entry={entry} state={state} />
        </p>
      </div>
      {full && <CardBody {...props} onDismissed={() => heading.current?.focus()} />}
      {state === "addressed" && (
        <NodeChips trace={trace} feature={feature} nodes={entry.changed} label="Changed" />
      )}
      {(state === "unaddressed" || state === "dismissed") && (
        <Reopen {...props} onReopened={() => heading.current?.focus()} />
      )}
    </li>
  );
}

/** One run's (or the Reviews) findings under its heading. */
export function Group(props: Context & { group: FindingGroup }) {
  const { group, ...rest } = props;
  return (
    <section className="maps-finding-group" data-run={group.run ?? undefined}>
      <h2 className="maps-finding-group__title">
        <Untrusted text={groupTitle(group, rest.feature)} />
      </h2>
      <ul className="maps-findings-list">
        {group.findings.map((entry) => (
          <FindingCard key={entry.finding.id} entry={entry} {...rest} />
        ))}
      </ul>
    </section>
  );
}
