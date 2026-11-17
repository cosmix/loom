import { useAtomValue, useStore } from "jotai/react";
import { useId, useRef, useState, type JSX } from "react";

import type { SettleAllResult } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { BlockedReason, LedgerRow, LedgerStatus } from "@/plan/concept/header-ledger";
import { refreshDocView } from "@/plan/concept/passages";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { BusyText } from "@/plan/ui/busy-text";
import { plural } from "@/plan/ui/plural";

import { SkippedDocuments } from "./settle-all-skipped";
import { postSettleAll } from "./settle-flow";
import { SEAL_ICONS } from "./settle-glyph";
import { loadTrace, useTrace } from "./state";

import "./trace.css";

type Props = { feature: string; role: "section" | "plan" };

function useBulkSettle({ feature, role }: Props) {
  const store = useStore();
  const reason = useAtomValue(authorBlockReasonAtom);
  const busy = useRef(false);
  const [working, setWorking] = useState(false);
  const [answer, setAnswer] = useState<SettleAllResult | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const send = async (confirm: boolean) => {
    if (busy.current || reason !== null) return;
    busy.current = true;
    setWorking(true);
    setNotice(null);
    const result = await postSettleAll(feature, role, confirm);
    if (result.ok) {
      setAnswer(result.value);
      for (const path of result.value.settled) refreshDocView(store, path);
      void loadTrace(store, feature);
    } else {
      setNotice(result.message);
    }
    busy.current = false;
    setWorking(false);
  };
  return { reason, working, answer, notice, send };
}

type Flow = ReturnType<typeof useBulkSettle>;

/** The ledger's state button (DESIGN.md, Buttons): secondary, its label the action. */
function BulkButton({
  flow,
  reasonId,
  confirm,
  label,
}: {
  flow: Flow;
  reasonId: string;
  confirm: boolean;
  label: string;
}): JSX.Element {
  const Seal = SEAL_ICONS.settled;
  return (
    <button
      type="button"
      data-look="secondary"
      className="plan-button inline-flex shrink-0 items-center justify-center trace-settle-button"
      aria-disabled={flow.reason !== null || flow.working || undefined}
      aria-busy={flow.working || undefined}
      aria-describedby={flow.reason !== null ? reasonId : undefined}
      onClick={() => void flow.send(confirm)}
    >
      {flow.working ? (
        <BusyText>Settling…</BusyText>
      ) : (
        <>
          <Seal aria-hidden="true" />
          {label}
        </>
      )}
    </button>
  );
}

/** What the last press left to say: the skipped documents and a failed request, under the status. */
function BulkNotes({ flow, feature }: { flow: Flow; feature: string }): JSX.Element {
  const unresolved = flow.answer?.skipped.filter((doc) => doc.reason === "unresolved").length ?? 0;
  const other = (flow.answer?.skipped.length ?? 0) - unresolved;
  return (
    <div className="header-ledger__notes">
      {unresolved > 0 && (
        <span role="status" className="trace-settle-note">
          {plural(unresolved, "document")} with unresolved points.
        </span>
      )}
      {other > 0 && (
        <span role="status" className="trace-settle-note">
          {plural(other, "document")} could not be settled.
        </span>
      )}
      {flow.answer !== null && flow.answer.skipped.length > 0 && (
        <SkippedDocuments feature={feature} skipped={flow.answer.skipped} />
      )}
      {flow.notice !== null && (
        <span role="alert" className="trace-settle-note">
          <Untrusted text={flow.notice} />
        </span>
      )}
    </div>
  );
}

function BulkControl(props: Props & { count: number }): JSX.Element {
  const { feature, role, count } = props;
  const flow = useBulkSettle(props);
  const reasonId = useId();
  const unresolved = flow.answer?.skipped.some((doc) => doc.reason === "unresolved") ?? false;
  const Seal = SEAL_ICONS.draft;
  const noun = plural(count, role === "section" ? "spec" : "plan");
  return (
    <LedgerRow
      label="Settling"
      action={
        <>
          <BulkButton
            flow={flow}
            reasonId={reasonId}
            confirm={false}
            label={role === "section" ? "Settle all specs" : "Settle all plans"}
          />
          {unresolved && (
            <BulkButton flow={flow} reasonId={reasonId} confirm label="Settle anyway" />
          )}
        </>
      }
    >
      <LedgerStatus tone="pending" glyph={<Seal />}>
        {`${noun} need${count === 1 ? "s" : ""} settling`}
      </LedgerStatus>
      {flow.reason !== null && (
        <BlockedReason id={reasonId}>
          <Untrusted text={flow.reason} />
        </BlockedReason>
      )}
      <BulkNotes flow={flow} feature={feature} />
    </LedgerRow>
  );
}

/**
 * The Settling row of an overview's ledger with the bulk settle action, shown only for written
 * documents that need settling. It sits among a `LedgerRows` block.
 */
export function SettleAllButton(props: Props): JSX.Element | null {
  const { trace } = useTrace(props.feature);
  const count =
    trace?.documents.filter(
      (doc) => doc.role === props.role && doc.settleable && doc.settle !== "settled",
    ).length ?? 0;
  if (count === 0) return null;
  return <BulkControl key={`${props.feature}/${props.role}`} {...props} count={count} />;
}
