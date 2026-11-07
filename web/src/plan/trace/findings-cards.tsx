import { useAtomValue, useStore } from "jotai/react";
import { CornerDownRight, FilePlus2, X } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { Link, useLocation } from "react-router";

import type { OfferView } from "@/plan/api/schema";
import type { NodeRef, TraceFinding, TraceView } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { hashAnchor } from "@/plan/files/address";
import { featureSourceHref } from "@/plan/feature/source-href";
import { runOffer } from "@/plan/offers/actions";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { ActionError } from "@/plan/ui/action-error";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import {
  dismissReason,
  failureOf,
  foldSeed,
  planSeed,
  REASON_MAX,
  setFindingStatus,
} from "./findings-actions";
import { FoldPicker } from "./findings-fold";
import {
  foldChoices,
  groupTitle,
  nodeLabel,
  statusText,
  type FindingGroup,
} from "./findings-model";
import { ledgerBlock, type LedgerHash } from "./ledger-hash";
import { loadTrace } from "./state";

// The cards of the Findings page: one per finding, with the three ways to settle an open one.
// Every ledger string is drawn as text (`Untrusted`).

/** What every finding card needs from the page. */
export interface Context {
  trace: TraceView;
  feature: string;
  /** The feature's Plan follow-ups offer, undefined when it has none. */
  offer: OfferView | undefined;
  /** The ledger hash that goes with `trace`; a change is sent only with it. */
  ledger: LedgerHash;
}

const NOT_STARTED = "The planner chat is not available, so no session started.";
const NO_OFFER = "This feature has no metaplan, so no Plan follow-ups can start.";
const FOLD_LABEL = "Fold into entry";
const DISMISS_LABEL = "Dismiss";

function AboutChips(props: { trace: TraceView; feature: string; nodes: NodeRef[] }) {
  const { trace, feature, nodes } = props;
  if (nodes.length === 0) return null;
  return (
    <p className="maps-about" role="group" aria-label="About">
      {nodes.map((node) => (
        <span key={`${node.path}#${node.anchor ?? ""}`}>
          <Link
            className="maps-chip"
            to={featureSourceHref(feature, node.path, { anchor: node.anchor })}
          >
            <Untrusted text={nodeLabel(trace, node)} />
          </Link>{" "}
        </span>
      ))}
    </p>
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

type Mode = "idle" | "fold" | "dismiss";

const OPENERS: Partial<Record<Mode, string>> = { fold: FOLD_LABEL, dismiss: DISMISS_LABEL };

/**
 * Closing the picker or the form unmounts the control the keyboard was on. `remember(mode)`, called
 * as the mode goes back to idle, returns focus to the button that opened it once it is drawn again.
 */
function useRestoreFocus(mode: Mode) {
  const root = useRef<HTMLDivElement>(null);
  const pending = useRef<string | null>(null);
  useEffect(() => {
    if (mode !== "idle" || pending.current === null) return;
    const label = pending.current;
    pending.current = null;
    const buttons = [...(root.current?.querySelectorAll("button") ?? [])];
    buttons.find((button) => button.textContent === label)?.focus();
  }, [mode]);
  const remember = (from: Mode) => {
    pending.current = OPENERS[from] ?? null;
  };
  return { root, remember };
}

/** The three buttons; Plan this and Fold into entry share one blocked reason, drawn once. */
function ActionButtons(props: {
  /** Names each button after its finding: the same three buttons are drawn on every open card. */
  findingId: string;
  offerBlock: string | null;
  authorBlock: string | null;
  onPlan: () => void;
  onFold: () => void;
  onDismiss: () => void;
}) {
  const { offerBlock, authorBlock, findingId } = props;
  const reasonId = useId();
  const dismissShares = authorBlock !== null && authorBlock === offerBlock;
  return (
    <>
      <DisabledReason
        reason={offerBlock}
        reasonId={reasonId}
        look="secondary"
        label={`Plan this ${findingId}`}
        onClick={props.onPlan}
      >
        <FilePlus2 aria-hidden="true" />
        Plan this
      </DisabledReason>
      <DisabledReason
        reason={offerBlock}
        reasonId={reasonId}
        look="secondary"
        label={`${FOLD_LABEL} ${findingId}`}
        onClick={props.onFold}
      >
        <CornerDownRight aria-hidden="true" />
        {FOLD_LABEL}
      </DisabledReason>
      <DisabledReason
        label={`${DISMISS_LABEL} ${findingId}`}
        reason={authorBlock}
        reasonId={dismissShares ? reasonId : undefined}
        look="secondary"
        onClick={props.onDismiss}
      >
        <X aria-hidden="true" />
        {DISMISS_LABEL}
      </DisabledReason>
      {offerBlock !== null && (
        <span id={reasonId} className="plan-fine text-muted-foreground">
          <Untrusted text={offerBlock} />
        </span>
      )}
    </>
  );
}

/** Dismisses one finding with a reason; `close` runs only when the ledger accepted the change. */
function useDismiss(
  feature: string,
  id: string,
  ledger: LedgerHash,
  close: () => void,
  setFailure: (failure: string | null) => void,
) {
  const store = useStore();
  const [busy, setBusy] = useState(false);
  const dismiss = async (reason: string) => {
    if (ledger.hash === null) return;
    setBusy(true);
    const result = await setFindingStatus(feature, id, ledger.hash, {
      kind: "dismissed",
      detail: reason,
    });
    setBusy(false);
    const failed = failureOf(result);
    setFailure(failed);
    // A refused change keeps the reason in the form.
    if (failed === null) close();
    // A conflict means the ledger moved; the page shows what it holds now.
    void loadTrace(store, feature);
  };
  return { busy, dismiss };
}

/** Plan this, Fold into entry and Dismiss for one open finding. */
function Actions(props: Context & { entry: TraceFinding; onDismissed: () => void }) {
  const { entry, trace, feature, offer, ledger, onDismissed } = props;
  const [mode, setMode] = useState<Mode>("idle");
  const [failure, setFailure] = useState<string | null>(null);
  const authorBlock = useAtomValue(authorBlockReasonAtom);
  const { root, remember } = useRestoreFocus(mode);
  const offerBlock = offer === undefined ? NO_OFFER : authorBlock;

  const close = () => {
    remember(mode);
    setMode("idle");
  };
  const start = (seed: string) => {
    if (offer === undefined) return;
    close();
    setFailure(runOffer(offer, seed) ? null : NOT_STARTED);
  };
  // The card leaves the open state and this control unmounts, so focus goes to the card's heading.
  const dismissed = () => {
    setMode("idle");
    onDismissed();
  };
  const { busy, dismiss } = useDismiss(feature, entry.finding.id, ledger, dismissed, setFailure);

  return (
    <div className="maps-actions" ref={root}>
      {mode === "dismiss" ? (
        <DismissForm busy={busy} ledger={ledger} onConfirm={dismiss} onCancel={close} />
      ) : (
        <ActionButtons
          findingId={entry.finding.id}
          offerBlock={offerBlock}
          authorBlock={authorBlock}
          onPlan={() => start(planSeed(entry.finding))}
          onFold={() => setMode("fold")}
          onDismiss={() => setMode("dismiss")}
        />
      )}
      {mode === "fold" && (
        <FoldPicker
          choices={foldChoices(trace, entry.about)}
          onPick={(chosen) => start(foldSeed(entry.finding, chosen))}
          onCancel={close}
        />
      )}
      {failure !== null && <ActionError message={failure} />}
    </div>
  );
}

/**
 * One finding. An open one shows its text, the nodes it is about and its actions; a decided one
 * folds to its heading and the outcome in words. The heading element is the same in both, so the
 * focus a dismissal leaves there survives the reload that turns the card into a decided line.
 */
function FindingCard(props: Context & { entry: TraceFinding }) {
  const { entry, trace, feature, ...context } = props;
  const { finding } = entry;
  const open = finding.status.kind === "open";
  const { hash } = useLocation();
  const heading = useRef<HTMLHeadingElement>(null);
  // `data-addressed` washes the card the address names; `:target` does not follow router pushes.
  const addressed = hashAnchor(hash) === `finding-${finding.id}`;
  return (
    <li
      id={`finding-${finding.id}`}
      className="maps-finding"
      data-status={finding.status.kind}
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
          <Untrusted text={statusText(finding.status)} />
        </p>
      </div>
      {open && (
        <>
          <AboutChips trace={trace} feature={feature} nodes={entry.about} />
          <p className="maps-finding-text">
            <Untrusted text={finding.text} />
          </p>
          <Actions
            entry={entry}
            trace={trace}
            feature={feature}
            {...context}
            onDismissed={() => heading.current?.focus()}
          />
        </>
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
