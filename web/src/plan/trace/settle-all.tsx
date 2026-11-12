import { useAtomValue, useStore } from "jotai/react";
import { useId, useRef, useState, type JSX } from "react";

import type { SettleAllResult } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { refreshDocView } from "@/plan/concept/passages";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { BusyText } from "@/plan/ui/busy-text";
import { HEADING_BUTTON } from "@/plan/ui/doc-heading";
import { plural } from "@/plan/ui/plural";

import { postSettleAll } from "./settle-flow";
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
  return (
    <button
      type="button"
      {...HEADING_BUTTON}
      className={`${HEADING_BUTTON.className} trace-settle-button`}
      aria-disabled={flow.reason !== null || flow.working || undefined}
      aria-busy={flow.working || undefined}
      aria-describedby={flow.reason !== null ? reasonId : undefined}
      onClick={() => void flow.send(confirm)}
    >
      {flow.working ? <BusyText>Settling…</BusyText> : label}
    </button>
  );
}

function BulkNotes({ flow, reasonId }: { flow: Flow; reasonId: string }): JSX.Element {
  const unresolved = flow.answer?.skipped.filter((doc) => doc.reason === "unresolved").length ?? 0;
  const other = (flow.answer?.skipped.length ?? 0) - unresolved;
  return (
    <>
      {flow.reason !== null && (
        <span id={reasonId} className="plan-fine text-muted-foreground">
          <Untrusted text={flow.reason} />
        </span>
      )}
      {unresolved > 0 && (
        <>
          <span role="status" className="trace-settle-note">
            {plural(unresolved, "document")} with unresolved points.
          </span>
          <BulkButton flow={flow} reasonId={reasonId} confirm label="Settle anyway" />
        </>
      )}
      {other > 0 && (
        <span role="status" className="trace-settle-note">
          {plural(other, "document")} could not be settled. Reload and try again.
        </span>
      )}
      {flow.notice !== null && (
        <span role="alert" className="trace-settle-note">
          <Untrusted text={flow.notice} />
        </span>
      )}
    </>
  );
}

function BulkControl(props: Props & { count: number }): JSX.Element {
  const { role, count } = props;
  const flow = useBulkSettle(props);
  const reasonId = useId();
  return (
    <div className="trace-settle-control">
      <BulkButton
        flow={flow}
        reasonId={reasonId}
        confirm={false}
        label={role === "section" ? "Settle all specs" : "Settle all plans"}
      />
      <span className="plan-fine text-muted-foreground">
        {plural(count, role === "section" ? "spec" : "plan")} need{count === 1 ? "s" : ""} settling
      </span>
      <BulkNotes flow={flow} reasonId={reasonId} />
    </div>
  );
}

/** The overview's bulk settle action, shown only for written documents that need settling. */
export function SettleAllButton(props: Props): JSX.Element | null {
  const { trace } = useTrace(props.feature);
  const count =
    trace?.documents.filter(
      (doc) => doc.role === props.role && doc.settleable && doc.settle !== "settled",
    ).length ?? 0;
  if (count === 0) return null;
  return <BulkControl key={`${props.feature}/${props.role}`} {...props} count={count} />;
}
