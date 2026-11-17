import { useAtomValue, useStore } from "jotai/react";
import { RotateCcw, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";

import type { TraceFinding, TraceView } from "@/plan/api/schema-trace";
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
import { ledgerBlock, type LedgerHash } from "./ledger-hash";
import { loadTrace } from "./state";

// The two status changes of a finding, as the dialog's foot offers them: Dismiss (with its reason
// form) for a finding that waits on the reader, Reopen for one an address run could not fix or the
// reader dismissed. Both post through `setFindingStatus` with the ledger hash the page holds.

/** What every finding control needs from the page. */
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
 * findings run takes it while its evidence still verifies. The finding moves to another lane and
 * this control is gone, so focus moves to the dialog's title (`onReopened`).
 */
export function Reopen(props: Context & { entry: TraceFinding; onReopened: () => void }) {
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
        look="secondary"
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
 * Dismiss for one finding that waits on the reader. Cancelling the form unmounts the control the
 * keyboard was on, so focus goes back to the Dismiss button once it is drawn again; a saved
 * dismissal moves it to the dialog's title (`onDismissed`), since the finding changes lane and
 * the control is gone.
 */
export function Actions(props: Context & { entry: TraceFinding; onDismissed: () => void }) {
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
