import { useAtomValue } from "jotai/react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";

import type { DocView } from "@/plan/api/schema-files";
import { toAddress } from "@/plan/files/address";
import { canAuthorAtom } from "@/plan/state/atoms";

import { useEditFlow, type EditFlow } from "./edit-flow";
import { useLeaveGuard } from "./leave-guard";

function useLeaving(doc: DocView, flow: EditFlow, dirty: boolean) {
  const navigate = useNavigate();
  const { state } = flow;
  const [leaveTo, setLeaveTo] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const [closing, setClosing] = useState(false);

  useLeaveGuard(dirty && leaveTo === null);
  useEffect(() => {
    if (leaveTo !== null) navigate(leaveTo);
  }, [leaveTo, navigate]);
  useEffect(() => {
    if (!closing || state.status === "saving") return;
    if (state.status === "saved") setLeaveTo(toAddress(doc.path));
    else setClosing(false);
  }, [closing, state.status, doc.path]);

  const leave = () => setLeaveTo(toAddress(doc.path));
  return {
    asking,
    done: () => (dirty ? setAsking(true) : leave()),
    keepEditing: () => setAsking(false),
    saveAndClose: () => {
      setAsking(false);
      setClosing(true);
      flow.save(false);
    },
    discardAndLeave: () => {
      flow.discard();
      leave();
    },
    discardToFiles: () => {
      flow.discard();
      setLeaveTo("/files");
    },
  };
}

/** The save flow plus the leave handling (Done, Save and close, Discard) for one document. */
export function useEditSession(doc: DocView) {
  const canAuthor = useAtomValue(canAuthorAtom);
  const flow = useEditFlow(doc, canAuthor);
  const dirty = flow.state.text !== flow.state.savedText;
  const leaving = useLeaving(doc, flow, dirty);
  return { flow, canAuthor, ...leaving };
}

export type EditSession = ReturnType<typeof useEditSession>;
