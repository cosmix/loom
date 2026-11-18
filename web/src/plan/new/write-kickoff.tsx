import { useAtomValue } from "jotai";
import { Feather } from "lucide-react";

import type { KickoffSize } from "@/plan/api/schema-kickoff";
import { sessionAtom } from "@/plan/chat/model";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useProposalActions } from "@/plan/new/proposal";
import { canAuthorAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";

function useKickoffWrite(size: KickoffSize | null, name: string | null, target: string | null) {
  const canAuthor = useAtomValue(canAuthorAtom);
  const session = useAtomValue(sessionAtom);
  const actions = useProposalActions();
  const label = size === "plan" ? "Write the plan" : "Write the vision";
  const unnamed = target === null && (size === null || name === null);
  const reason = !canAuthor
    ? "Writing requires authoring"
    : unnamed
      ? "Settle the name in the conversation first"
      : session?.state !== "idle"
        ? "Wait for the current turn to finish"
        : actions.reason !== null
          ? "Sending the write request"
          : null;
  // The first click confirms the size and name, which arms the session's one write grant; once
  // the grant names a target, a click asks the agent to write it. Chat agreement never arms it.
  const write = () => {
    if (reason !== null) return;
    if (target === null && size !== null && name !== null) actions.confirm(size, name);
    else actions.message(`${label}.`);
  };
  return { label, reason, authorReason: actions.blockReason, error: actions.status, write };
}

/** The write button: it confirms the named work first, then asks for the document. */
export function WriteKickoff({
  size,
  name,
  target,
}: {
  size: KickoffSize | null;
  name: string | null;
  target: string | null;
}) {
  const { label, reason, authorReason, error, write } = useKickoffWrite(size, name, target);
  return (
    <div className="grid gap-2">
      <DisabledReason reason={reason} look="primary" onClick={write}>
        <Feather aria-hidden="true" />
        {label}
      </DisabledReason>
      {authorReason !== null && (
        <p className="plan-fine">
          <Untrusted text={authorReason} />
        </p>
      )}
      {error !== null && (
        <p role="status">
          <Untrusted text={error} />
        </p>
      )}
    </div>
  );
}
